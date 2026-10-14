use std::path::PathBuf;

use async_trait::async_trait;
use chrono::NaiveDateTime;
use domain::project::ProjectSlug;
use domain::task::TaskId;

use crate::ports::RepoError;

/// De qué es un registro del historial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordSubject {
    /// El análisis de la IA de una tarea (`show --ai --save`).
    Analysis(TaskId),
    /// Un plan del día (`plan --save`).
    Plan,
}

/// Lo que se guarda: el documento ya armado (Markdown) y dónde ubicarlo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRecord {
    pub subject: RecordSubject,
    /// El proyecto de la tarea, o el de `plan --project`. `None`: va a la carpeta general.
    pub project: Option<ProjectSlug>,
    /// Hora local del usuario: es la que ve en el nombre del archivo.
    pub saved_at: NaiveDateTime,
    pub content: String,
}

/// Un registro ya guardado, sin su contenido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub subject: RecordSubject,
    pub project: Option<ProjectSlug>,
    pub saved_at: NaiveDateTime,
    /// El primer encabezado `# ...` del documento, o el nombre del archivo si no tiene.
    pub title: String,
    pub path: PathBuf,
}

/// Qué registros listar. Los dos criterios se combinan: proyecto **y** tipo.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryFilter {
    /// `None`: todos los proyectos, también la carpeta general.
    pub project: Option<ProjectSlug>,
    pub subject: SubjectFilter,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SubjectFilter {
    #[default]
    Any,
    /// Solo los análisis de esta tarea.
    Task(TaskId),
    /// Solo los planes.
    Plans,
}

/// Registro de análisis y planes (§14.4). Lo implementa `adapter-fs` sobre
/// `history/<proyecto>/{analyses/<tarea>,plans}/<fecha>.md`.
#[async_trait]
pub trait HistoryRepository: Send + Sync {
    /// Guarda `record` en un archivo **nuevo** y devuelve su ruta. Nunca reemplaza uno
    /// anterior, aunque se guarden dos en el mismo segundo.
    async fn save(&self, record: &HistoryRecord) -> Result<PathBuf, RepoError>;

    /// Los registros que cumplen `filter`, del más reciente al más antiguo.
    async fn list(&self, filter: &HistoryFilter) -> Result<Vec<HistoryEntry>, RepoError>;

    /// El contenido de un registro.
    async fn read(&self, entry: &HistoryEntry) -> Result<String, RepoError>;
}

impl HistoryFilter {
    /// Si un registro de `subject` en `project` cumple el filtro. Lo comparten los
    /// adaptadores, para que todos filtren igual.
    #[must_use]
    pub fn matches(&self, subject: &RecordSubject, project: Option<&ProjectSlug>) -> bool {
        let project_ok = self.project.is_none() || self.project.as_ref() == project;
        let subject_ok = match (&self.subject, subject) {
            (SubjectFilter::Any, _) | (SubjectFilter::Plans, RecordSubject::Plan) => true,
            (SubjectFilter::Task(wanted), RecordSubject::Analysis(id)) => wanted == id,
            _ => false,
        };
        project_ok && subject_ok
    }
}

/// El texto del primer encabezado `# ...` de un documento Markdown, sin el `#`.
#[must_use]
pub fn markdown_title(content: &str) -> Option<&str> {
    content
        .lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(str::trim)
        .filter(|title| !title.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(raw: &str) -> TaskId {
        TaskId::new(raw).unwrap()
    }

    fn slug(raw: &str) -> ProjectSlug {
        raw.parse().unwrap()
    }

    #[test]
    fn the_default_filter_matches_everything() {
        let filter = HistoryFilter::default();

        assert!(filter.matches(&RecordSubject::Plan, None));
        assert!(filter.matches(&RecordSubject::Analysis(id("1")), Some(&slug("acme"))));
    }

    #[test]
    fn filters_by_task_and_by_plans() {
        let task = HistoryFilter {
            subject: SubjectFilter::Task(id("1")),
            ..HistoryFilter::default()
        };
        let plans = HistoryFilter {
            subject: SubjectFilter::Plans,
            ..HistoryFilter::default()
        };

        assert!(task.matches(&RecordSubject::Analysis(id("1")), None));
        assert!(!task.matches(&RecordSubject::Analysis(id("2")), None));
        assert!(!task.matches(&RecordSubject::Plan, None));
        assert!(plans.matches(&RecordSubject::Plan, None));
        assert!(!plans.matches(&RecordSubject::Analysis(id("1")), None));
    }

    #[test]
    fn a_project_filter_excludes_other_projects_and_the_general_folder() {
        let filter = HistoryFilter {
            project: Some(slug("acme")),
            ..HistoryFilter::default()
        };

        assert!(filter.matches(&RecordSubject::Plan, Some(&slug("acme"))));
        assert!(!filter.matches(&RecordSubject::Plan, Some(&slug("idk"))));
        assert!(!filter.matches(&RecordSubject::Plan, None));
    }

    #[test]
    fn the_title_is_the_first_level_one_heading() {
        assert_eq!(
            markdown_title("intro\n## Sub\n# Análisis · Fix\n# Otro"),
            Some("Análisis · Fix")
        );
        assert_eq!(markdown_title("## Solo nivel 2"), None);
        assert_eq!(markdown_title("#   \n"), None);
    }
}
