//! UC-04 `CloseTask` (§6): cerrar una tarea sin abrir el proveedor.
//!
//! Dos casos de uso, porque la interfaz pregunta **entre** los dos:
//!
//! 1. [`PrepareClose`]: qué estados acepta la tarea y cuál se sugiere.
//! 2. La interfaz elige el estado, pide un comentario opcional y **confirma** (FR-12).
//! 3. [`CloseTask`]: valida y escribe.
//!
//! La confirmación es de la interfaz, no de esta capa (UC-04): aquí solo se ejecuta lo ya
//! confirmado.

use domain::task::{StatusOption, TaskId, find_status, suggest_final};

use crate::dto::{CloseOptions, CloseResult, StatusOptionView};
use crate::{AppContext, AppError};

/// Las opciones para cerrar una tarea.
#[derive(Debug)]
pub struct PrepareClose<'a> {
    ctx: &'a AppContext,
}

impl<'a> PrepareClose<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// 1. Pedir **a la vez** la tarea y sus estados disponibles (`futures::try_join!`).
    /// 2. Calcular la sugerencia con `suggest_final(&statuses, preferred)`.
    /// 3. Armar `CloseOptions`: id y título de la tarea, su estado actual con `to_string()`,
    ///    cada estado como `StatusOptionView` y el **nombre** del sugerido.
    ///
    /// `preferred` son los nombres de `provider.status_map.done` de la configuración.
    ///
    /// # Errors
    ///
    /// [`AppError::Provider`] si la tarea no existe o falla el proveedor.
    pub async fn execute(
        &self,
        id: &TaskId,
        preferred: &[String],
    ) -> Result<CloseOptions, AppError> {
        let (task, statuses) = futures::try_join!(
            self.ctx.tasks.task(id),
            self.ctx.tasks.available_statuses(id)
        )?;
        let suggest_final = suggest_final(&statuses, preferred).map(|option| option.name.clone());

        Ok(CloseOptions {
            task_id: task.id.as_str().to_string(),
            title: task.title,
            current_status: task.status.to_string(),
            statuses: statuses.iter().map(StatusOptionView::from).collect(),
            suggested: suggest_final,
        })
    }
}

/// Lo que la interfaz ya decidió y el usuario confirmó.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseCommand {
    pub task_id: TaskId,
    /// El estado final como lo escribió o eligió el usuario (sin distinguir mayúsculas).
    pub status: String,
    /// Comentario de cierre. Vacío o solo espacios cuenta como "sin comentario".
    pub comment: Option<String>,
}

/// Cierra una tarea: comentario opcional y cambio de estado (UC-04, FR-10, FR-11).
#[derive(Debug)]
pub struct CloseTask<'a> {
    ctx: &'a AppContext,
}

impl<'a> CloseTask<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// En este orden; las validaciones van **antes** de cualquier escritura:
    ///
    /// 1. Pedir los estados disponibles de la tarea.
    /// 2. Buscar `command.status` con `find_status`. Si no existe →
    ///    `AppError::InvalidInput` con el mensaje de [`unknown_status`].
    /// 3. Recortar el comentario (`trim`). Si queda texto, publicarlo con `add_comment`.
    /// 4. Cambiar el estado con `set_status`, usando el nombre **exacto** del estado
    ///    encontrado (no el que escribió el usuario).
    /// 5. Devolver `CloseResult` con ese nombre y si se publicó comentario.
    ///
    /// El comentario va primero: si el cambio de estado dispara notificaciones, el
    /// comentario ya está ahí para darles contexto.
    ///
    /// # Errors
    ///
    /// - [`AppError::InvalidInput`] si el estado no existe en la lista (no se escribe nada).
    /// - [`AppError::Provider`] si falla el proveedor. Si falla `set_status` después de
    ///   publicar el comentario, el comentario queda publicado.
    pub async fn execute(&self, command: CloseCommand) -> Result<CloseResult, AppError> {
        let statuses = self.ctx.tasks.available_statuses(&command.task_id).await?;
        let status = find_status(&statuses, &command.status)
            .ok_or_else(|| AppError::InvalidInput(unknown_status(&command.status, &statuses)))?;
        let clean_comment = command
            .comment
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty());

        if let Some(text) = clean_comment {
            self.ctx.tasks.add_comment(&command.task_id, text).await?;
        }
        self.ctx
            .tasks
            .set_status(&command.task_id, &status.name)
            .await?;

        Ok(CloseResult {
            task_id: command.task_id.as_str().to_owned(),
            final_status: status.name.clone(),
            commented: clean_comment.is_some(),
        })
    }
}

/// `unknown status `QA`; available: to do, in progress, complete`. Ya está hecha.
pub(crate) fn unknown_status(name: &str, options: &[StatusOption]) -> String {
    let available: Vec<&str> = options.iter().map(|o| o.name.as_str()).collect();
    format!(
        "unknown status `{name}`; available: {}",
        available.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use domain::task::{Status, StatusKind};
    use domain::testing::{task, user};

    use super::*;
    use crate::ports::ProviderError;
    use crate::testing::{FakeTaskProvider, FixedClock, Write, context};

    const NOW: &str = "2026-10-10T15:00:00Z";

    fn statuses() -> Vec<StatusOption> {
        vec![
            StatusOption::new("to do", StatusKind::Open),
            StatusOption::new("Code Review", StatusKind::Custom),
            StatusOption::new("complete", StatusKind::Done),
        ]
    }

    fn setup() -> (AppContext, std::sync::Arc<FakeTaskProvider>) {
        let mut t = task("86b2x1abc");
        t.title = "Fix invoice rounding".into();
        t.status = Status::InProgress;
        let provider = FakeTaskProvider::new(user("me"))
            .with_tasks(vec![t])
            .with_statuses(statuses());
        context(provider, vec![], FixedClock::at(NOW))
    }

    fn id() -> TaskId {
        TaskId::new("86b2x1abc").unwrap()
    }

    fn command(status: &str, comment: Option<&str>) -> CloseCommand {
        CloseCommand {
            task_id: id(),
            status: status.to_owned(),
            comment: comment.map(str::to_owned),
        }
    }

    #[tokio::test]
    async fn prepare_lists_statuses_and_suggests_one() {
        let (ctx, _) = setup();

        let options = PrepareClose::new(&ctx)
            .execute(&id(), &["complete".to_owned()])
            .await
            .unwrap();

        assert_eq!(options.task_id, "86b2x1abc");
        assert_eq!(options.title, "Fix invoice rounding");
        assert_eq!(options.current_status, "in progress");
        let names: Vec<&str> = options.statuses.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["to do", "Code Review", "complete"]);
        assert_eq!(options.suggested.as_deref(), Some("complete"));
    }

    #[tokio::test]
    async fn prepare_without_a_final_status_suggests_nothing() {
        let provider = FakeTaskProvider::new(user("me"))
            .with_tasks(vec![task("1")])
            .with_statuses(vec![StatusOption::new("to do", StatusKind::Open)]);
        let (ctx, _) = context(provider, vec![], FixedClock::at(NOW));

        let options = PrepareClose::new(&ctx)
            .execute(&TaskId::new("1").unwrap(), &[])
            .await
            .unwrap();

        assert_eq!(options.suggested, None);
    }

    #[tokio::test]
    async fn closes_with_comment_first_and_the_exact_status_name() {
        let (ctx, fake) = setup();

        let result = CloseTask::new(&ctx)
            .execute(command("code review", Some("  Listo, con test.  ")))
            .await
            .unwrap();

        assert_eq!(
            fake.writes(),
            [
                Write::Comment("86b2x1abc".into(), "Listo, con test.".into()),
                Write::Status("86b2x1abc".into(), "Code Review".into()),
            ]
        );
        assert_eq!(
            result,
            CloseResult {
                task_id: "86b2x1abc".into(),
                final_status: "Code Review".into(),
                commented: true,
            }
        );
    }

    #[tokio::test]
    async fn blank_comments_are_not_published() {
        let (ctx, fake) = setup();

        let result = CloseTask::new(&ctx)
            .execute(command("complete", Some("   ")))
            .await
            .unwrap();

        assert_eq!(
            fake.writes(),
            [Write::Status("86b2x1abc".into(), "complete".into())]
        );
        assert!(!result.commented);
    }

    #[tokio::test]
    async fn unknown_status_writes_nothing() {
        let (ctx, fake) = setup();

        let result = CloseTask::new(&ctx)
            .execute(command("QA", Some("hecho")))
            .await;

        let Err(AppError::InvalidInput(message)) = result else {
            panic!("expected InvalidInput, got {result:?}");
        };
        assert_eq!(
            message,
            "unknown status `QA`; available: to do, Code Review, complete"
        );
        assert!(fake.writes().is_empty());
    }

    #[tokio::test]
    async fn missing_task_is_not_found() {
        let (ctx, _) = setup();

        let result = CloseTask::new(&ctx)
            .execute(CloseCommand {
                task_id: TaskId::new("404").unwrap(),
                status: "complete".into(),
                comment: None,
            })
            .await;

        assert!(matches!(
            result,
            Err(AppError::Provider(ProviderError::NotFound(_)))
        ));
    }
}
