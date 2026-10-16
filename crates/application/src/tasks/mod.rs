//! Casos de uso sobre tareas.

mod check_connection;
mod close_task;
mod get_today_tasks;
mod inspect_task;

pub use check_connection::CheckConnection;
pub use close_task::{CloseCommand, CloseTask, PrepareClose};
pub use get_today_tasks::{GetTodayTasks, GroupBy, TodayQuery};
pub use inspect_task::InspectTask;
