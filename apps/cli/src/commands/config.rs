use application::ports::{ConfigStore, SecretName, SecretStore};
use application::settings::{ConfigKey, SettingsChange};

use crate::args::ConfigCommand;

/// `config show | get | set | check`. (`init` y `path` se atienden antes, en `commands::run`.)
///
/// - `show`: una línea `clave = valor` por cada clave de `ConfigKey::KNOWN`, o
///   `clave = (por defecto)` si no está en el archivo. Al final, el estado de cada secreto
///   (`clickup_token: guardado` o `falta`), **nunca** su valor: la salida se puede pegar
///   en un chat sin filtrar el token.
/// - `get`: el valor, o `(no definido: se usa el valor por defecto)`.
/// - `set`: aplica `SettingsChange::Set`, que conserva los comentarios del archivo, y
///   confirma con `✔ clave = valor`.
/// - `check`: carga la configuración, arma el `AppContext` (`bootstrap::build`) y le pide
///   al proveedor el usuario actual. Si todo responde, la configuración y el token sirven:
///   `✔ Configuración válida · conectado como usuario <id>`.
///
/// # Errors
///
/// Configuración inválida, secreto faltante o fallo del proveedor (en `check`).
pub(crate) async fn run(
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
            let me = application::tasks::CheckConnection::new(&ctx)
                .execute()
                .await?;
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
