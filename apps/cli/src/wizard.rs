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
use domain::task::flatten;

use crate::presenter;

/// 1. Si `store.path()` ya existe, preguntar si se reemplaza (por defecto, no). Si la
///    respuesta es no, terminar sin cambios.
/// 2. Elegir el proveedor entre `registry::task_providers()` (`Select`).
/// 3. Por cada campo del descriptor, preguntar con [`ask`]: los `Text` van a un
///    `FieldValues`; los `Secret`, a una lista `(SecretName, SecretString)`.
/// 4. `AppSettings::new((descriptor.provider_settings)(&values))` y `validate()`
///    **antes** de guardar nada.
/// 5. Guardar cada secreto en `secrets`, y la configuración con `SettingsChange::Replace`.
/// 6. Imprimir dónde quedó y sugerir `taskflow config check`.
///
/// # Errors
///
/// Entrada cancelada (Ctrl+C), configuración inválida o fallo al guardar.
pub fn config_init(store: &impl ConfigStore, secrets: &impl SecretStore) -> anyhow::Result<()> {
    let theme = ColorfulTheme::default();

    if store.path().exists() {
        let replace = Confirm::with_theme(&theme)
            .with_prompt(format!(
                "Ya existe una configuración en {}. ¿Reemplazarla?",
                store.path().display()
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

    // Validar antes de guardar nada: ni el token ni el archivo quedan a medias.
    let settings = AppSettings::new((descriptor.provider_settings)(&values));
    settings.validate()?;
    for (name, value) in new_secrets {
        secrets.set(name, value)?;
    }
    store.apply(&SettingsChange::Replace(Box::new(settings)))?;

    println!("✔ Configuración guardada en {}", store.path().display());
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
/// 1. `BrowseLocations` y `flatten` del árbol. Si está vacío, un error.
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
pub async fn project_add(ctx: &AppContext) -> anyhow::Result<()> {
    let theme = ColorfulTheme::default();

    // 1. El árbol del proveedor, como lista.
    let tree = BrowseLocations::new(ctx).execute().await?;
    let nodes = flatten(&tree);
    anyhow::ensure!(!nodes.is_empty(), "el proveedor no devolvió ubicaciones");

    // 2. Ubicaciones elegidas → reglas de ubicación.
    let labels: Vec<String> = nodes.iter().map(presenter::location_label).collect();
    let chosen = MultiSelect::with_theme(&theme)
        .with_prompt("Ubicaciones del proyecto (espacio para marcar, enter para seguir)")
        .items(&labels)
        .interact()?;
    let mut rules: Vec<MatchRule> = chosen
        .iter()
        .map(|&i| MatchRule::Location {
            level: nodes[i].segment.level.clone(),
            id: nodes[i].segment.id.clone(),
        })
        .collect();

    // 3. Etiquetas → reglas de etiqueta.
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
