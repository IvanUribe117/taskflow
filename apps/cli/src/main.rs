//! `taskflow`: la interfaz de línea de comandos (§12.1).
//!
//! Solo traduce: argumentos → casos de uso → texto en la terminal. La lógica vive en
//! `application`; el armado de dependencias, en `bootstrap`.
#![warn(clippy::pedantic)]

mod args;
mod commands;
mod presenter;
mod wizard;

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    commands::run(args::Cli::parse()).await
}
