//! Casos de uso con IA: UC-02 `PlanMyDay` y el análisis de UC-03 (`show --ai`).

mod brief;
mod plan_my_day;
mod summarize_task;

pub use plan_my_day::{PlanMyDay, PlanQuery};
pub use summarize_task::{AiLimits, SummarizeTask};
