//! `taskflow config init`: el asistente interactivo (FR-19, UC-05).
//!
//! Las preguntas son de la interfaz; las reglas son de `application` y `bootstrap`:
//! el asistente recorre los campos del descriptor sin saber qué proveedor es.

use application::ports::{ConfigStore, SecretName, SecretStore, SecretString};
use application::settings::{AppSettings, SettingsChange};
use bootstrap::registry::{self, FieldKind, FieldSpec, FieldValues};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input, Password, Select};

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
    store.apply(&SettingsChange::Replace(settings))?;

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
