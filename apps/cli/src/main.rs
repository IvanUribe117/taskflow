//! `taskflow`: la interfaz de línea de comandos (§12.1).
//!
//! Solo traduce: argumentos → casos de uso → texto en la terminal. La lógica vive en
//! `application`; el armado de dependencias, en `bootstrap`.
#![warn(clippy::pedantic)]

mod args;
mod commands;
mod markdown;
mod presenter;
mod record;
mod term;
mod wizard;

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Los avisos de los adaptadores (por ejemplo, "esperando N s" ante el límite de
    // ClickUp) llegan por `tracing`. Se muestran en la salida de errores, sin hora ni
    // módulo, para que no se mezclen con la salida normal (`--json`).
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::WARN)
        .without_time()
        .with_target(false)
        .init();

    commands::run(args::Cli::parse()).await
}
