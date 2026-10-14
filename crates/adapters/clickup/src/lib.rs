//! Adaptador de `ClickUp` (API v2): implementa el puerto `TaskProvider`.
//!
//! Capas internas (§16):
//! - `api_types`: structs que reflejan el JSON de `ClickUp` tal cual.
//! - `mapper`: traducción de esos structs al dominio.
//! - `client`: HTTP (autenticación, errores, límite de peticiones, reintentos).
//! - `provider`: `impl TaskProvider for ClickUpProvider`.
//! - `time`: `impl TimeTracker for ClickUpTimeTracker` (el contador).
#![warn(clippy::pedantic)]

mod api_types;
mod client;
mod config;
mod mapper;
mod provider;
mod query;
mod status_map;
mod time;

pub use config::ClickUpConfig;
pub use provider::ClickUpProvider;
pub use status_map::StatusMap;
pub use time::ClickUpTimeTracker;
