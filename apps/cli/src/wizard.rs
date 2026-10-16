//! `taskflow config init`: el asistente interactivo (FR-19, UC-05).
//!
//! Las preguntas son de la interfaz; las reglas son de `application` y `bootstrap`:
//! el asistente recorre los campos del descriptor sin saber qué proveedor es.

use std::path::PathBuf;

use application::AppContext;
use application::ports::{ConfigStore, SecretName, SecretStore, SecretString};
use application::projects::{BrowseLocations, CreateProject, ProjectDraft};
use application::settings::{AppSettings, SettingsChange};
use bootstrap::registry::{self, FieldKind, FieldSpec, FieldValues};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input, MultiSelect, Password, Select};
use domain::project::{MatchRule, ProjectKind, ProjectSlug};
use domain::task::{FlatNode, LocationNode, flatten};

use crate::presenter;

/// `taskflow config init`: crea la configuración desde cero, pregunta a pregunta.
///
/// 1. Si ya hay un archivo en `store.path()`, pregunta si reemplazarlo (por defecto, no).
///    Si la respuesta es no, termina sin tocar nada.
/// 2. El proveedor de tareas, de `registry::task_providers()`.
/// 3. Los campos que pide ese proveedor, con [`ask`]: los de texto van a un
///    `FieldValues`; los secretos (el token) se guardan aparte, porque van al llavero y
///    no al archivo.
/// 4. El proveedor de IA, de `registry::ai_providers()`.
/// 5. Arma `AppSettings` y la valida **antes** de guardar nada: si algo está mal, ni el
///    token ni el archivo quedan a medias.
/// 6. Guarda los secretos en `secrets` y la configuración con `SettingsChange::Replace`,
///    dice dónde quedó y sugiere `taskflow config check` para probar la conexión.
///
/// # Errors
///
/// Entrada cancelada (Ctrl+C), configuración inválida o fallo al guardar.
pub(crate) fn config_init(
    store: &impl ConfigStore,
    secrets: &impl SecretStore,
) -> anyhow::Result<()> {
    crate::term::need_terminal("`config init` es un asistente: córrelo en una terminal")?;
    let theme = ColorfulTheme::default();

    if store.path().exists() {
        let replace = Confirm::with_theme(&theme)
            .with_prompt(format!(
                "Ya existe una configuración en {}. ¿Reemplazarla?",
                presenter::path(store.path())
            ))
            .default(false)
            .interact()?;
        if !replace {
            println!("Sin cambios.");
            return Ok(());
        }
    }

    let providers = registry::task_providers();
    let names: Vec<&str> = providers.iter().map(|p| p.display_name).collect();
    let choice = Select::with_theme(&theme)
        .with_prompt("Proveedor de tareas")
        .items(&names)
        .default(0)
        .interact()?;
    let descriptor = &providers[choice];

    let mut values = FieldValues::new();
    let mut new_secrets = Vec::new();
    for field in descriptor.fields {
        match ask(field, &theme)? {
            Answer::Text(value) => {
                values.insert(field.key, value);
            }
            Answer::Secret(name, value) => new_secrets.push((name, value)),
        }
    }

    // Proveedor de IA (ADR-027). Hoy solo hay uno y no pide nada, pero el asistente ya
    // recorre el registro: agregar otro no cambia este código.
    let ai_providers = registry::ai_providers();
    let ai_names: Vec<&str> = ai_providers.iter().map(|p| p.display_name).collect();
    let ai_choice = Select::with_theme(&theme)
        .with_prompt("Proveedor de IA")
        .items(&ai_names)
        .default(0)
        .interact()?;

    let mut settings = AppSettings::new((descriptor.provider_settings)(&values));
    settings.ai.kind = ai_providers[ai_choice].kind;
    // Validar antes de guardar nada: ni el token ni el archivo quedan a medias.
    settings.validate()?;
    for (name, value) in new_secrets {
        secrets.set(name, value)?;
    }
    store.apply(&SettingsChange::Replace(Box::new(settings)))?;

    println!(
        "✔ Configuración guardada en {}",
        presenter::path(store.path())
    );
    println!("Prueba la conexión con `taskflow config check`.");
    Ok(())
}

/// Lo que respondió el usuario a un campo.
enum Answer {
    Text(String),
    Secret(SecretName, SecretString),
}

/// Pregunta un campo: `Input` para `Text` (con su `default` si tiene), `Password` para
/// `Secret` (no se ve al escribir).
fn ask(field: &FieldSpec, theme: &ColorfulTheme) -> anyhow::Result<Answer> {
    match field.kind {
        FieldKind::Text { default } => {
            let mut input = Input::<String>::with_theme(theme).with_prompt(field.label);
            if let Some(default) = default {
                input = input.default(default.to_owned());
            }
            Ok(Answer::Text(input.interact_text()?))
        }
        FieldKind::Secret(name) => {
            let value = Password::with_theme(theme)
                .with_prompt(field.label)
                .interact()?;
            Ok(Answer::Secret(name, SecretString::from(value)))
        }
    }
}

/// `taskflow project add` (FR-29): crea un proyecto eligiendo sus ubicaciones del proveedor.
///
/// 1. `BrowseLocations::roots` y elegir un espacio (`Select`); después
///    `BrowseLocations::children` de ese espacio y `flatten` del espacio con su árbol.
/// 2. `MultiSelect` con una fila por nodo (`presenter::location_label`). Cada nodo elegido
///    es una regla `MatchRule::Location` con su nivel y su id.
/// 3. Etiquetas opcionales, separadas por coma (`Input` con `allow_empty(true)`): cada una
///    es una regla `MatchRule::Tag`.
/// 4. Slug (`Input` validado con `ProjectSlug::from_str`), nombre (por defecto, el del
///    primer nodo elegido), tipo (`Select`: cliente o interno) y repositorio local
///    opcional.
/// 5. `CreateProject` y mostrar el resultado con `presenter::project_detail`.
///
/// # Errors
///
/// Fallo del proveedor, entrada cancelada o proyecto inválido (por ejemplo, sin reglas).
pub(crate) async fn project_add(ctx: &AppContext) -> anyhow::Result<()> {
    crate::term::need_terminal("`project add` es un asistente: córrelo en una terminal")?;
    let theme = ColorfulTheme::default();

    // 1. Primero el espacio (una sola petición) y después solo su árbol. Pedir el árbol
    //    completo del workspace supera el límite de peticiones de ClickUp.
    let browse = BrowseLocations::new(ctx);
    let roots = browse.roots().await?;
    anyhow::ensure!(!roots.is_empty(), "el proveedor no devolvió ubicaciones");
    let root_labels: Vec<String> = roots
        .iter()
        .map(|segment| presenter::location_label(&FlatNode { depth: 0, segment }))
        .collect();
    let root_index = Select::with_theme(&theme)
        .with_prompt("Espacio del proyecto")
        .items(&root_labels)
        .default(0)
        .interact()?;
    let root = roots[root_index].clone();
    let children = browse.children(&root).await?;

    // El espacio y su árbol, como lista: el espacio mismo también se puede elegir.
    let tree = vec![LocationNode {
        segment: root,
        children,
    }];
    let nodes = flatten(&tree);

    // 2 y 3. Ubicaciones y etiquetas → reglas. Se repite hasta que haya al menos una:
    //    un proyecto sin reglas nunca tendría tareas, y es mejor avisarlo aquí que después
    //    de preguntar todo lo demás.
    let labels: Vec<String> = nodes.iter().map(presenter::location_label).collect();
    let (chosen, rules) = loop {
        let chosen = MultiSelect::with_theme(&theme)
            .with_prompt(
                "Ubicaciones del proyecto (barra espaciadora para marcar, Enter para continuar)",
            )
            .items(&labels)
            .interact()?;
        let mut rules: Vec<MatchRule> = chosen
            .iter()
            .map(|&i| MatchRule::Location {
                level: nodes[i].segment.level.clone(),
                id: nodes[i].segment.id.clone(),
            })
            .collect();

        let tags: String = Input::with_theme(&theme)
            .with_prompt("Etiquetas (separadas por coma, opcional)")
            .allow_empty(true)
            .interact_text()?;
        rules.extend(
            tags.split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(|tag| MatchRule::Tag(tag.to_owned())),
        );

        if !rules.is_empty() {
            break (chosen, rules);
        }
        println!("Marca al menos una ubicación con la barra espaciadora, o escribe una etiqueta.");
    };

    // 4. Slug, nombre, tipo y repositorio.
    let slug: ProjectSlug = Input::<String>::with_theme(&theme)
        .with_prompt("Slug (minúsculas, dígitos y guiones)")
        .validate_with(|raw: &String| raw.parse::<ProjectSlug>().map(|_| ()))
        .interact_text()?
        .parse()?;
    let default_name = chosen
        .first()
        .map(|&i| nodes[i].segment.name.clone())
        .unwrap_or_default();
    let name: String = Input::with_theme(&theme)
        .with_prompt("Nombre")
        .default(default_name)
        .interact_text()?;
    let kinds = [ProjectKind::Client, ProjectKind::Internal];
    let kind = Select::with_theme(&theme)
        .with_prompt("Tipo")
        .items(&["cliente", "interno"])
        .default(0)
        .interact()?;
    let repo: String = Input::with_theme(&theme)
        .with_prompt("Repositorio local (opcional)")
        .allow_empty(true)
        .interact_text()?;

    // 5. Crear y mostrar.
    let detail = CreateProject::new(ctx)
        .execute(ProjectDraft {
            slug,
            name,
            kind: kinds[kind],
            rules,
            repo_path: (!repo.trim().is_empty()).then(|| PathBuf::from(repo.trim())),
        })
        .await?;
    println!(
        "✔ Proyecto creado\n\n{}",
        presenter::project_detail(&detail)
    );
    Ok(())
}
