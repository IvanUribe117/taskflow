//! Historial (§14.4): guardar los análisis y planes de la IA para consultarlos después.

use std::path::PathBuf;

use chrono::Timelike;
use domain::project::ProjectSlug;

use crate::dto::HistoryItem;
use crate::ports::{HistoryFilter, HistoryRecord, RecordSubject};
use crate::{AppContext, AppError};

/// Guarda un documento ya armado (Markdown) con la hora local del usuario.
#[derive(Debug)]
pub struct SaveToHistory<'a> {
    ctx: &'a AppContext,
}

impl<'a> SaveToHistory<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// Devuelve la ruta del archivo nuevo. La hora se guarda sin fracciones de segundo:
    /// es la misma precisión que tiene el nombre del archivo.
    ///
    /// # Errors
    ///
    /// [`AppError::Repo`] si no se puede escribir.
    pub async fn execute(
        &self,
        subject: RecordSubject,
        project: Option<ProjectSlug>,
        content: String,
    ) -> Result<PathBuf, AppError> {
        let now = self.ctx.clock.now().with_timezone(&self.ctx.clock.offset());
        let saved_at = now
            .naive_local()
            .with_nanosecond(0)
            .unwrap_or(now.naive_local());
        let record = HistoryRecord {
            subject,
            project,
            saved_at,
            content,
        };
        Ok(self.ctx.history.save(&record).await?)
    }
}

/// Consulta el historial.
#[derive(Debug)]
pub struct ListHistory<'a> {
    ctx: &'a AppContext,
}

impl<'a> ListHistory<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// Los registros que cumplen `filter`, del más reciente al más antiguo, hasta `limit`.
    ///
    /// # Errors
    ///
    /// [`AppError::Repo`] si no se puede leer la carpeta.
    pub async fn execute(
        &self,
        filter: &HistoryFilter,
        limit: Option<usize>,
    ) -> Result<Vec<HistoryItem>, AppError> {
        let entries = self.ctx.history.list(filter).await?;
        Ok(entries
            .iter()
            .take(limit.unwrap_or(usize::MAX))
            .map(HistoryItem::from)
            .collect())
    }

    /// El registro más reciente que cumple `filter`, con su contenido. `None` si no hay.
    ///
    /// # Errors
    ///
    /// [`AppError::Repo`] si no se puede leer.
    pub async fn latest(
        &self,
        filter: &HistoryFilter,
    ) -> Result<Option<(HistoryItem, String)>, AppError> {
        let Some(entry) = self.ctx.history.list(filter).await?.into_iter().next() else {
            return Ok(None);
        };
        let content = self.ctx.history.read(&entry).await?;
        Ok(Some((HistoryItem::from(&entry), content)))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::task::TaskId;
    use domain::testing::user;

    use super::*;
    use crate::ports::SubjectFilter;
    use crate::testing::{FakeHistoryRepository, FakeTaskProvider, FixedClock, context};

    fn ctx(now: &str, history: Arc<FakeHistoryRepository>) -> AppContext {
        let mut ctx = context(
            FakeTaskProvider::new(user("me")),
            vec![],
            FixedClock::at(now),
        )
        .0;
        ctx.history = history;
        ctx
    }

    fn task(raw: &str) -> RecordSubject {
        RecordSubject::Analysis(TaskId::new(raw).unwrap())
    }

    #[tokio::test]
    async fn saves_with_the_users_local_time_without_fractions() {
        let history = Arc::new(FakeHistoryRepository::default());
        let ctx = ctx("2026-10-14T20:30:12.750Z", history.clone());

        let path = SaveToHistory::new(&ctx)
            .execute(task("1"), Some("acme".parse().unwrap()), "# A".into())
            .await
            .unwrap();

        assert_eq!(path, PathBuf::from("mem/1.md"));
        let saved = history.saved();
        // FixedClock está en Bogotá (UTC-5).
        assert_eq!(saved[0].saved_at.to_string(), "2026-10-14 15:30:12");
        assert_eq!(saved[0].project.as_ref().unwrap().as_str(), "acme");
        assert_eq!(saved[0].content, "# A");
    }

    #[tokio::test]
    async fn lists_newest_first_with_a_limit_and_reads_the_latest() {
        let history = Arc::new(FakeHistoryRepository::default());
        for (now, subject, body) in [
            ("2026-10-13T15:00:00Z", task("1"), "# Primero"),
            ("2026-10-14T15:00:00Z", RecordSubject::Plan, "# Plan"),
            ("2026-10-15T15:00:00Z", task("1"), "# Segundo"),
        ] {
            SaveToHistory::new(&ctx(now, history.clone()))
                .execute(subject, None, body.into())
                .await
                .unwrap();
        }
        let ctx = ctx("2026-10-16T15:00:00Z", history);
        let all = HistoryFilter::default();
        let only_task = HistoryFilter {
            subject: SubjectFilter::Task(TaskId::new("1").unwrap()),
            ..HistoryFilter::default()
        };

        let items = ListHistory::new(&ctx).execute(&all, Some(2)).await.unwrap();
        let (latest, content) = ListHistory::new(&ctx)
            .latest(&only_task)
            .await
            .unwrap()
            .unwrap();

        let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["Segundo", "Plan"]);
        assert_eq!(latest.kind, "analysis");
        assert_eq!(latest.task_id.as_deref(), Some("1"));
        assert_eq!(content, "# Segundo");
    }

    #[tokio::test]
    async fn latest_is_none_when_nothing_matches() {
        let ctx = ctx("2026-10-14T15:00:00Z", Arc::default());
        let plans = HistoryFilter {
            subject: SubjectFilter::Plans,
            ..HistoryFilter::default()
        };

        assert!(
            ListHistory::new(&ctx)
                .latest(&plans)
                .await
                .unwrap()
                .is_none()
        );
    }
}
