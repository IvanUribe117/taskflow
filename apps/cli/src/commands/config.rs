use application::ports::{ConfigStore, SecretName, SecretStore};
use application::settings::{ConfigKey, SettingsChange};

use crate::args::ConfigCommand;

/// `config show | get | set | check`. (`init` y `path` se atienden antes, en `commands::run`.)
///
/// - `show`: por cada clave de `ConfigKey::KNOWN`, una línea `clave = valor`, o
///   `clave = (por defecto)` si no está en el archivo. Al final, por cada `SecretName::ALL`,
///   `clickup_token: guardado` o `clickup_token: falta` (**nunca** el valor).
/// - `get`: el valor, o `(no definido: se usa el valor por defecto)`.
/// - `set`: aplicar `SettingsChange::Set` e imprimir `✔ clave = valor`.
/// - `check`: cargar, `bootstrap::build`, pedir `current_user()` al proveedor e imprimir
///   `✔ Configuración válida · conectado como usuario <id>`.
///
/// # Errors
///
/// Configuración inválida, secreto faltante o fallo del proveedor (en `check`).
pub async fn run(
    command: ConfigCommand,
    store: &impl ConfigStore,
    secrets: &impl SecretStore,
) -> anyhow::Result<()> {
    match command {
        ConfigCommand::Show => {
            for raw in ConfigKey::KNOWN {
                let key: ConfigKey = raw.parse()?;
                let value = store
                    .get_raw(key)?
                    .unwrap_or_else(|| "(por defecto)".to_owned());
                println!("{key} = {value}");
            }
            for name in SecretName::ALL {
                let state = if secrets.get(name)?.is_some() {
                    "guardado"
                } else {
                    "falta"
                };
                println!("{}: {state}", name.key());
            }
        }
        ConfigCommand::Get { key } => match store.get_raw(key)? {
            Some(value) => println!("{value}"),
            None => println!("(no definido: se usa el valor por defecto)"),
        },
        ConfigCommand::Set { key, value } => {
            store.apply(&SettingsChange::Set {
                key,
                value: value.clone(),
            })?;
            println!("✔ {key} = {value}");
        }
        ConfigCommand::Check => {
            let settings = store.load()?;
            let ctx = bootstrap::build(&settings, secrets, store.path())?;
            let me = ctx.tasks.current_user().await?;
            println!(
                "✔ Configuración válida · conectado como usuario {}",
                me.as_str()
            );
        }
        ConfigCommand::Init | ConfigCommand::Path => {
            unreachable!("`config init` y `config path` se atienden en commands::run")
        }
    }
    Ok(())
}
