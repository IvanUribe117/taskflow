//! DTOs: lo que reciben las interfaces (§11.3).
//!
//! Las interfaces nunca reciben entidades del dominio sino estas estructuras serializables:
//! el mismo resultado sirve para la terminal, `--json`, Tauri o una API web.

use std::path::PathBuf;

use chrono::{DateTime, NaiveDateTime, Utc};
use domain::context::ContextSection;
use domain::project::{Project, ProjectKey, ProjectResolver};
use domain::task::{Comment, StatusOption, Task, UserId};

use domain::session::{Publication, ReportOutcome, WorkSession};

use crate::ports::{
    ChangedFile, CommitMessage, HistoryEntry, PlanTask, PrDescription, PullRequest, RecordSubject,
    TaskSummary, TimeEntry,
};
use serde::Serialize;

/// Una tarea en una lista (`today`, `plan`, el detalle de `show`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskView {
    /// El id de la tarea en el proveedor (`86b2x1abc`).
    pub id: String,
    /// El título tal como está en el proveedor.
    pub title: String,
    /// `Status` mostrado con `Display` ("in progress", "Code Review"...).
    pub status: String,
    /// `Priority` mostrado con `Display` ("urgent", "high"...).
    pub priority: String,
    /// Fecha límite, si tiene.
    pub due: Option<DateTime<Utc>>,
    /// Si `due` ya pasó respecto de `now` al armar la vista.
    pub overdue: bool,
    /// Comentarios sin resolver asignados a `me`.
    pub pending_comments: usize,
    /// Enlace para abrir la tarea en el proveedor.
    pub url: String,
}

impl TaskView {
    /// Arma la vista a partir de la tarea. `me` decide qué comentarios cuentan como pendientes
    /// y `now`, si está vencida.
    #[must_use]
    pub fn from_task(task: &Task, me: &UserId, now: DateTime<Utc>) -> Self {
        Self {
            id: task.id.as_str().to_string(),
            title: task.title.clone(),
            status: task.status.to_string(),
            priority: task.priority.to_string(),
            due: task.due,
            overdue: task.is_overdue(now),
            pending_comments: task.pending_comments_for(me).count(),
            url: task.url.clone(),
        }
    }
}

/// Un proyecto resumido: lo justo para encabezar un grupo o una tarea.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectView {
    /// Identificador estable del proyecto (`acme`): el que se usa en los comandos.
    pub slug: String,
    /// Nombre para mostrar.
    pub name: String,
    /// "client" o "internal".
    pub kind: String,
}

impl From<&Project> for ProjectView {
    fn from(project: &Project) -> Self {
        Self {
            slug: project.slug.to_string(),
            name: project.name.clone(),
            kind: project.kind.to_string(),
        }
    }
}

/// Las tareas de un proyecto, para mostrarlas bajo un mismo encabezado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskGroup {
    /// `None` = "Sin proyecto" o lista sin agrupar.
    pub project: Option<ProjectView>,
    /// En el orden en que se deben mostrar (ya ordenadas por el caso de uso).
    pub tasks: Vec<TaskView>,
}

impl TaskGroup {
    /// Un único grupo sin proyecto, con todas las tareas en el mismo orden.
    #[must_use]
    pub fn flat(tasks: &[Task], me: &UserId, now: DateTime<Utc>) -> Self {
        Self {
            project: None,
            tasks: tasks
                .iter()
                .map(|task| TaskView::from_task(task, me, now))
                .collect(),
        }
    }

    /// Grupo de un proyecto. Con `ProjectKey::Unassigned`, o si el slug no existe en
    /// `projects`, el grupo queda con `project: None`.
    #[must_use]
    pub fn for_project(
        key: &ProjectKey,
        projects: &ProjectResolver,
        tasks: &[&Task],
        me: &UserId,
        now: DateTime<Utc>,
    ) -> Self {
        let project = match key {
            ProjectKey::Project(slug) => projects.get(slug).map(ProjectView::from),
            ProjectKey::Unassigned => None,
        };
        Self {
            project,
            tasks: tasks
                .iter()
                .map(|&task| TaskView::from_task(task, me, now))
                .collect(),
        }
    }
}

/// El resultado de `today` (UC-01): las tareas del día agrupadas por proyecto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TodayReport {
    /// Cuándo se armó el reporte (UTC).
    pub generated_at: DateTime<Utc>,
    /// Sin agrupar, un solo grupo sin proyecto. Por proyecto, el primero es el de la tarea más
    /// prioritaria.
    pub groups: Vec<TaskGroup>,
}

/// Un proyecto con todas sus reglas, para `project list` y `project show` (UC-06).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectDetail {
    /// Identificador estable del proyecto (`acme`).
    pub slug: String,
    /// Nombre para mostrar.
    pub name: String,
    /// "client" o "internal".
    pub kind: String,
    /// Cada regla con su `Display`: `"list 901100200300"`, `"tag acme"`.
    pub rules: Vec<String>,
    /// La ruta como texto (`Path::display`), o `None`.
    pub repo_path: Option<String>,
    /// Puntos extra en la priorización de sus tareas.
    pub priority_boost: i64,
    /// Los estados sugeridos propios, por etapa: `"ready: needs approval"`. Vacío si usa
    /// los globales.
    pub statuses: Vec<String>,
    /// `[preview]`: la URL (plantilla, con `{port}`) y el primer puerto.
    pub preview: Option<String>,
}

impl From<&Project> for ProjectDetail {
    fn from(project: &Project) -> Self {
        Self {
            kind: project.kind.to_string(),
            slug: project.slug.to_string(),
            name: project.name.clone(),
            priority_boost: project.priority_boost,
            rules: project.rules.iter().map(ToString::to_string).collect(),
            repo_path: project
                .repo_path
                .as_ref()
                .map(|path| path.display().to_string()),
            statuses: [
                ("ready", &project.statuses.ready),
                ("in_progress", &project.statuses.in_progress),
                ("done", &project.statuses.done),
            ]
            .into_iter()
            .filter(|(_, names)| !names.is_empty())
            .map(|(stage, names)| format!("{stage}: {}", names.join(", ")))
            .collect(),
            preview: project
                .preview
                .as_ref()
                .map(|p| format!("{} (puertos desde {})", p.url, p.port)),
        }
    }
}

/// Un estado disponible para la tarea, para mostrarlo en un selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusOptionView {
    /// El nombre exacto del estado en el proveedor: el que se envía al cerrar.
    pub name: String,
    /// `"open"`, `"custom"`, `"done"` o `"closed"`.
    pub kind: String,
}

impl From<&StatusOption> for StatusOptionView {
    fn from(option: &StatusOption) -> Self {
        Self {
            kind: option.kind.to_string(),
            name: option.name.clone(),
        }
    }
}

/// Lo que la interfaz necesita para preguntar cómo cerrar una tarea (UC-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloseOptions {
    /// El id de la tarea en el proveedor.
    pub task_id: String,
    /// El título de la tarea.
    pub title: String,
    /// El estado actual, con el `Display` de `Status`.
    pub current_status: String,
    /// Todos los estados de la lista de la tarea, en el orden del proveedor.
    pub statuses: Vec<StatusOptionView>,
    /// El nombre del estado sugerido (`suggest_final`), si hay alguno.
    pub suggested: Option<String>,
}

/// El resultado de cerrar una tarea.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloseResult {
    /// El id de la tarea en el proveedor.
    pub task_id: String,
    /// El nombre exacto con que quedó en el proveedor.
    pub final_status: String,
    /// Si se publicó el comentario de cierre.
    pub commented: bool,
}

/// Una sección de contexto lista para enviarse a la IA: solo lo que la IA necesita.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextBlock {
    /// La clave de la sección (`negocio`).
    pub key: String,
    /// El título de la sección, que encabeza el bloque.
    pub title: String,
    /// El texto en Markdown, sin el front matter.
    pub body: String,
}

impl From<&ContextSection> for ContextBlock {
    fn from(section: &ContextSection) -> Self {
        Self {
            key: section.key.to_string(),
            title: section.title.clone(),
            body: section.body.clone(),
        }
    }
}

/// El contexto para una llamada a la IA (UC-07), separado por rol (§14.3).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BuiltContext {
    /// Secciones `instruction`: van al *system prompt*.
    pub instructions: Vec<ContextBlock>,
    /// Secciones `knowledge`: van en el mensaje, dentro de `<project_context>`.
    pub knowledge: Vec<ContextBlock>,
    /// Claves de las secciones que no entraron en el presupuesto (FR-38).
    pub omitted: Vec<String>,
}

/// Una sección tal como la ve `context list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SectionView {
    /// La clave de la sección (`negocio`): con ella se edita o se borra.
    pub key: String,
    /// El título de la sección.
    pub title: String,
    /// `"knowledge"` o `"instruction"`.
    pub role: String,
    /// Cada uso con su `Display`, en el orden de declaración de `Usage`.
    pub usage: Vec<String>,
    /// Menor número = más importante: al pasar del presupuesto se omiten primero las de número
    /// más alto.
    pub priority: u8,
    /// Si es `true`, nunca se envía a la IA (FR-39).
    pub private: bool,
    /// Largo del cuerpo en caracteres: lo que cuenta para el presupuesto.
    pub chars: usize,
    /// `"global"`, `"project"` o `"override"` (del proyecto, reemplaza a una global).
    pub origin: String,
}

/// Lo que recibiría la IA para una tarea (`context preview`, FR-40).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContextPreview {
    /// El id de la tarea en el proveedor.
    pub task_id: String,
    /// El título de la tarea.
    pub title: String,
    /// El proyecto al que pertenece la tarea, si alguno la reclama.
    pub project: Option<ProjectView>,
    /// El uso para el que se armó el contexto (`summary`, `plan`, `agent_prompt`...).
    pub usage: String,
    /// Las secciones que recibiría la IA, separadas por rol.
    pub context: BuiltContext,
}

/// `show --ai`: el detalle de la tarea más el análisis de la IA (UC-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskInsight {
    /// Lo mismo que muestra `show` sin `--ai`.
    pub detail: TaskDetail,
    /// El análisis de la IA.
    pub summary: TaskSummary,
}

/// `plan`: las tareas del día y, si la IA respondió, el plan (UC-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DailyPlan {
    /// Cuándo se armó el plan (UTC).
    pub generated_at: DateTime<Utc>,
    /// En orden de prioridad.
    pub tasks: Vec<PlanTask>,
    /// El plan en Markdown, o `None` si no hay tareas o si la IA falló.
    pub plan: Option<String>,
    /// Por qué no hay plan, si la IA falló (degradación elegante: la lista sigue sirviendo).
    pub ai_error: Option<String>,
    /// Secciones de contexto que no entraron en el presupuesto.
    pub omitted_sections: Vec<String>,
}

/// Detalle de una tarea para `show` (UC-03). Con `--ai`, va dentro de [`TaskInsight`] junto
/// con el análisis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskDetail {
    /// Los datos de la lista: título, estado, prioridad, vencimiento.
    pub task: TaskView,
    /// La descripción en Markdown, o `None` si la tarea no tiene.
    pub description: Option<String>,
    /// Ruta legible, de la raíz a la lista: `["90110001", "Backend", "ACME – Sprint"]`.
    /// Cada tramo usa su nombre o, si no tiene (el espacio de `ClickUp`), su id.
    pub location: Vec<String>,
    /// Las etiquetas tal como están en el proveedor.
    pub tags: Vec<String>,
    /// El proyecto al que pertenece la tarea, si alguno la reclama.
    pub project: Option<ProjectView>,
    /// En el mismo orden en que los entrega el proveedor.
    pub comments: Vec<CommentView>,
}

impl TaskDetail {
    /// `task.comments` ya debe tener los comentarios cargados.
    #[must_use]
    pub fn new(task: &Task, project: Option<&Project>, me: &UserId, now: DateTime<Utc>) -> Self {
        Self {
            task: TaskView::from_task(task, me, now),
            description: task.description.clone(),
            location: task
                .location
                .segments()
                .iter()
                .map(|s| {
                    if s.name.is_empty() {
                        s.id.clone()
                    } else {
                        s.name.clone()
                    }
                })
                .collect(),
            tags: task.tags.clone(),
            project: project.map(ProjectView::from),
            comments: task
                .comments
                .iter()
                .map(|c| CommentView::new(c, me))
                .collect(),
        }
    }
}

/// Un comentario de la tarea, para el detalle de `show`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommentView {
    /// El nombre del autor tal como lo muestra el proveedor.
    pub author: String,
    /// El texto del comentario.
    pub body: String,
    /// Cuándo se escribió (UTC).
    pub created_at: DateTime<Utc>,
    /// Si ya se marcó como resuelto en el proveedor.
    pub resolved: bool,
    /// Asignado a `me`: lo que la interfaz resalta como pendiente.
    pub assigned_to_me: bool,
}

impl CommentView {
    /// Arma la vista; `me` decide `assigned_to_me`.
    #[must_use]
    pub fn new(comment: &Comment, me: &UserId) -> Self {
        Self {
            author: comment.author.clone(),
            body: comment.body.clone(),
            created_at: comment.created_at,
            resolved: comment.resolved,
            assigned_to_me: comment.assignee.as_ref() == Some(me),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::project::{MatchRule, ProjectKind};
    use domain::task::{LocationLevel, Priority, Status};
    use domain::testing::{at, comment, project, task, user};

    const NOW: &str = "2026-10-10T15:00:00Z";

    #[test]
    fn task_view_maps_every_field() {
        let me = user("me");
        let mut t = task("86b2x1abc");
        t.title = "Fix invoice rounding".into();
        t.status = Status::InProgress;
        t.priority = Priority::High;
        t.due = Some(at("2026-10-09T12:00:00Z"));
        t.url = "https://app.clickup.com/t/86b2x1abc".into();
        t.comments = vec![
            comment(Some(&me), false),
            comment(Some(&me), true),
            comment(Some(&user("other")), false),
        ];

        let view = TaskView::from_task(&t, &me, at(NOW));

        assert_eq!(
            view,
            TaskView {
                id: "86b2x1abc".into(),
                title: "Fix invoice rounding".into(),
                status: "in progress".into(),
                priority: "high".into(),
                due: Some(at("2026-10-09T12:00:00Z")),
                overdue: true,
                pending_comments: 1,
                url: "https://app.clickup.com/t/86b2x1abc".into(),
            }
        );
    }

    #[test]
    fn task_view_without_due_date_is_not_overdue() {
        let view = TaskView::from_task(&task("1"), &user("me"), at(NOW));

        assert_eq!(view.due, None);
        assert!(!view.overdue);
    }

    #[test]
    fn project_view_uses_slug_name_and_kind() {
        let mut p = project("acme", vec![]);
        p.name = "ACME Corp".into();

        let view = ProjectView::from(&p);
        assert_eq!(
            view,
            ProjectView {
                slug: "acme".into(),
                name: "ACME Corp".into(),
                kind: "client".into(),
            }
        );

        p.kind = ProjectKind::Internal;
        assert_eq!(ProjectView::from(&p).kind, "internal");
    }

    #[test]
    fn flat_group_keeps_order_and_has_no_project() {
        let tasks = vec![task("b"), task("a"), task("c")];

        let group = TaskGroup::flat(&tasks, &user("me"), at(NOW));

        assert_eq!(group.project, None);
        let ids: Vec<&str> = group.tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["b", "a", "c"]);
    }

    #[test]
    fn project_group_carries_the_project_view() {
        let rule = MatchRule::Location {
            level: LocationLevel::Space,
            id: "S1".into(),
        };
        let projects = ProjectResolver::new(vec![project("acme", vec![rule])]);
        let t = task("1");
        let key = ProjectKey::Project("acme".parse().unwrap());

        let group = TaskGroup::for_project(&key, &projects, &[&t], &user("me"), at(NOW));

        assert_eq!(group.project.map(|p| p.slug), Some("acme".to_owned()));
        assert_eq!(group.tasks.len(), 1);
    }

    #[test]
    fn unassigned_group_has_no_project() {
        let projects = ProjectResolver::default();
        let t = task("1");

        let group = TaskGroup::for_project(
            &ProjectKey::Unassigned,
            &projects,
            &[&t],
            &user("me"),
            at(NOW),
        );

        assert_eq!(group.project, None);
        assert_eq!(group.tasks.len(), 1);
    }

    #[test]
    fn project_detail_lists_rules_and_repo() {
        let mut p = project(
            "acme",
            vec![
                MatchRule::Location {
                    level: LocationLevel::List,
                    id: "L1".into(),
                },
                MatchRule::Tag("acme".into()),
            ],
        );
        p.name = "ACME Corp".into();
        p.repo_path = Some("/code/acme".into());
        p.priority_boost = 20;

        assert_eq!(
            ProjectDetail::from(&p),
            ProjectDetail {
                slug: "acme".into(),
                name: "ACME Corp".into(),
                kind: "client".into(),
                rules: vec!["list L1".into(), "tag acme".into()],
                repo_path: Some("/code/acme".into()),
                priority_boost: 20,
                statuses: vec![],
                preview: None,
            }
        );
    }

    #[test]
    fn status_option_view_uses_name_and_kind() {
        let option = StatusOption::new("Code Review", domain::task::StatusKind::Custom);

        assert_eq!(
            StatusOptionView::from(&option),
            StatusOptionView {
                name: "Code Review".into(),
                kind: "custom".into(),
            }
        );
    }

    #[test]
    fn context_block_keeps_key_title_and_body() {
        let section = domain::context::ContextSection {
            key: "negocio".parse().unwrap(),
            title: "Modelo de negocio".into(),
            role: domain::context::SectionRole::Knowledge,
            usage: enumset::EnumSet::all(),
            priority: 10,
            private: false,
            body: "ACME vende software.".into(),
        };

        assert_eq!(
            ContextBlock::from(&section),
            ContextBlock {
                key: "negocio".into(),
                title: "Modelo de negocio".into(),
                body: "ACME vende software.".into(),
            }
        );
    }

    #[test]
    fn comment_view_marks_comments_assigned_to_me() {
        let me = user("me");
        let mut mine = comment(Some(&me), false);
        mine.author = "Client PM".into();
        mine.body = "can you check?".into();

        let view = CommentView::new(&mine, &me);

        assert_eq!(
            view,
            CommentView {
                author: "Client PM".into(),
                body: "can you check?".into(),
                created_at: at("2026-10-01T00:00:00Z"),
                resolved: false,
                assigned_to_me: true,
            }
        );
        assert!(!CommentView::new(&comment(Some(&user("other")), false), &me).assigned_to_me);
        assert!(!CommentView::new(&comment(None, false), &me).assigned_to_me);
    }

    #[test]
    fn task_detail_maps_every_field() {
        let me = user("me");
        let mut t = task("86b2x1abc");
        t.description = Some("Totals differ by one cent".into());
        t.tags = vec!["backend".into()];
        t.location = domain::task::TaskLocation::new(vec![
            domain::task::LocationSegment {
                level: LocationLevel::Space,
                id: "S1".into(),
                name: String::new(),
            },
            domain::task::LocationSegment {
                level: LocationLevel::List,
                id: "L1".into(),
                name: "Sprint".into(),
            },
        ]);
        t.comments = vec![comment(Some(&me), false), comment(None, true)];
        let p = project("acme", vec![]);

        let detail = TaskDetail::new(&t, Some(&p), &me, at(NOW));

        assert_eq!(detail.task, TaskView::from_task(&t, &me, at(NOW)));
        assert_eq!(
            detail.description.as_deref(),
            Some("Totals differ by one cent")
        );
        assert_eq!(detail.location, ["S1", "Sprint"]);
        assert_eq!(detail.tags, ["backend"]);
        assert_eq!(detail.project, Some(ProjectView::from(&p)));
        assert_eq!(detail.comments.len(), 2);
        assert!(detail.comments[0].assigned_to_me);
        assert!(detail.comments[1].resolved);
    }

    #[test]
    fn today_report_serializes_to_json() {
        let report = TodayReport {
            generated_at: at(NOW),
            groups: vec![TaskGroup {
                project: None,
                tasks: vec![],
            }],
        };

        let json = serde_json::to_value(&report).unwrap();

        assert_eq!(json["generated_at"], "2026-10-10T15:00:00Z");
        assert_eq!(json["groups"][0]["project"], serde_json::Value::Null);
    }
}

/// Un registro del historial, para `history`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistoryItem {
    /// "analysis" o "plan".
    pub kind: &'static str,
    /// Solo en los análisis.
    pub task_id: Option<String>,
    /// El slug del proyecto, si la tarea o el plan tenían uno.
    pub project: Option<String>,
    /// Hora local del usuario.
    pub saved_at: NaiveDateTime,
    /// Título para la lista: el de la tarea o el del plan.
    pub title: String,
    /// El archivo Markdown del registro, para abrirlo.
    pub path: PathBuf,
}

impl From<&HistoryEntry> for HistoryItem {
    fn from(entry: &HistoryEntry) -> Self {
        let (kind, task_id) = match &entry.subject {
            RecordSubject::Analysis(id) => ("analysis", Some(id.as_str().to_owned())),
            RecordSubject::Plan => ("plan", None),
        };
        Self {
            kind,
            task_id,
            project: entry.project.as_ref().map(ToString::to_string),
            saved_at: entry.saved_at,
            title: entry.title.clone(),
            path: entry.path.clone(),
        }
    }
}

/// La sesión de trabajo, para `session show` y la salida de cada comando.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionView {
    /// El id de la tarea en el proveedor.
    pub task_id: String,
    /// El título de la tarea al empezar la sesión.
    pub title: String,
    /// Enlace a la tarea en el proveedor.
    pub url: String,
    /// El slug del proyecto de la tarea, si tiene.
    pub project: Option<String>,
    /// `analyzing`, `awaiting_approval`, `in_progress`, `paused`, `closed` o `aborted`.
    pub state: &'static str,
    /// Cuándo empezó la sesión (UTC).
    pub started_at: DateTime<Utc>,
    /// Minutos con el contador corriendo (sin las pausas).
    pub elapsed_minutes: i64,
    /// Si el contador de tiempo está corriendo ahora.
    pub timer_running: bool,
    /// `None` mientras se analiza; después, `ready` o `unavailable`.
    pub report: Option<&'static str>,
    /// Por qué falló la IA, si falló.
    pub report_error: Option<String>,
    /// Cuándo se publicó el reporte en la tarea, o `None` si sigue como borrador.
    pub published_at: Option<DateTime<Utc>>,
    /// `report.md`: el análisis de la IA; se puede editar antes de publicarlo.
    pub report_path: PathBuf,
    /// `agent-prompt.md`: el prompt que recibe el agente de código.
    pub agent_prompt_path: PathBuf,
    /// `work-report.md`: lo que se hizo, para publicar (existe después de armarlo).
    pub work_report_path: PathBuf,
    /// Lo que quedó sin aplicar tras un fallo, en palabras (`session retry`).
    pub pending: Vec<String>,
    /// Si la sesión cree que el contador corre pero el proveedor no lo tiene en esta tarea
    /// (lo detuviste o lo cambiaste fuera de Taskflow). Solo lo calcula `session show`.
    pub timer_out_of_sync: bool,
    /// La carpeta de trabajo guardada en la sesión (el worktree de la tarea), si hay.
    pub work_dir: Option<PathBuf>,
}

impl SessionView {
    /// Arma la vista de `session` en `now`. Las rutas las pone quien llama (las conoce el
    /// `SessionStore`); `timer_out_of_sync` queda en `false` hasta que `session show` lo calcule.
    #[must_use]
    pub fn new(
        session: &WorkSession,
        now: DateTime<Utc>,
        report_path: PathBuf,
        agent_prompt_path: PathBuf,
        work_report_path: PathBuf,
    ) -> Self {
        let (report, report_error) = match &session.report {
            None => (None, None),
            Some(ReportOutcome::Ready) => (Some("ready"), None),
            Some(ReportOutcome::Unavailable { reason }) => {
                (Some("unavailable"), Some(reason.clone()))
            }
        };
        let published_at = match session.publication {
            Publication::Draft => None,
            Publication::Published { at } => Some(at),
        };
        Self {
            task_id: session.task_id.as_str().to_owned(),
            title: session.title.clone(),
            url: session.url.clone(),
            project: session.project.as_ref().map(ToString::to_string),
            state: session.state.name(),
            started_at: session.started_at,
            elapsed_minutes: session.elapsed(now).num_minutes(),
            timer_running: session.timer_running(),
            report,
            report_error,
            published_at,
            report_path,
            agent_prompt_path,
            work_report_path,
            pending: session
                .pending
                .iter()
                .map(crate::session::effect_label)
                .collect(),
            timer_out_of_sync: false,
            work_dir: session.work_dir.clone(),
        }
    }
}

/// El resultado de un comando de la sesión.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionOutcome {
    /// La sesión tal como quedó después del comando.
    pub session: SessionView,
    /// Lo que no salió pero no detiene la sesión (una notificación, un estado sugerido que
    /// no existe en la lista...).
    pub warnings: Vec<String>,
    /// Lo que conviene saber y salió bien (la rama se puso al día con la base...).
    pub notes: Vec<String>,
    /// Al terminar (`done`, `abort`): el worktree de la tarea, si quedó limpio y se puede
    /// quitar. La interfaz pregunta y lo quita con `SessionWorkDir::remove`.
    pub removable_worktree: Option<RemovableWorktree>,
    /// Dónde quedó archivada, si terminó (`done` o `abort`).
    pub archived_to: Option<PathBuf>,
}

/// Un contador del proveedor que ya está corriendo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TimerView {
    /// La tarea del contador, o `None` si corre sin tarea.
    pub task_id: Option<String>,
    /// El título de esa tarea, si el proveedor lo informa.
    pub task_title: Option<String>,
    /// Cuándo arrancó (UTC).
    pub started_at: DateTime<Utc>,
}

impl From<&TimeEntry> for TimerView {
    fn from(entry: &TimeEntry) -> Self {
        Self {
            task_id: entry.task_id.as_ref().map(|id| id.as_str().to_owned()),
            task_title: entry.task_title.clone(),
            started_at: entry.started_at,
        }
    }
}

/// Lo que hay que saber antes de `start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StartCheck {
    /// Una sesión sin terminar: `start` no puede seguir.
    pub active: Option<SessionView>,
    /// Un contador corriendo **en otra tarea**: `start` lo detendría (hay que preguntar).
    pub running_timer: Option<TimerView>,
    /// La última sesión de esta tarea, si ya se trabajó: `start` la continúa.
    pub previous: Option<PreviousView>,
}

/// La sesión anterior de una tarea.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviousView {
    /// Cuándo terminó (UTC).
    pub ended_at: DateTime<Utc>,
    /// `closed` o `aborted`.
    pub state: &'static str,
    /// Si llegó a publicar el reporte en la tarea.
    pub published: bool,
    /// Si tiene una conversación del agente para retomar.
    pub has_agent: bool,
}

impl From<&WorkSession> for PreviousView {
    fn from(session: &WorkSession) -> Self {
        Self {
            ended_at: session.ended_at(),
            state: session.state.name(),
            published: matches!(session.publication, Publication::Published { .. }),
            has_agent: session.agent_session.is_some(),
        }
    }
}

/// Lo que recibe el agente de código (`taskflow agent`, y al aprobar).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentBrief {
    /// El prompt de la IA + el reporte con tus ediciones + tus reglas + la tarea.
    pub prompt: String,
    /// Dónde abrir el agente: el worktree de la tarea o, sin él, el repositorio del
    /// proyecto, si existe en disco.
    pub repo_path: Option<PathBuf>,
    /// El worktree de la tarea, si se trabaja en uno.
    pub worktree: Option<WorkDirSetup>,
    /// La vista previa del worktree, si el proyecto tiene `[preview]`.
    pub preview: Option<PreviewView>,
    /// Lo que no salió pero no impide abrir el agente (no se pudo crear el worktree…).
    pub warnings: Vec<String>,
    /// Lo que conviene saber y salió bien (la rama se puso al día con la base…).
    pub notes: Vec<String>,
    /// `agent-prompt.md`, solo el prompt de la IA.
    pub prompt_path: PathBuf,
    /// La conversación del agente, si ya se abrió antes: se retoma en vez de empezar otra.
    pub agent_session: Option<String>,
    /// Si esa conversación ya recibió el prompt de esta sesión. Si no (una tarea devuelta),
    /// se retoma **con** el prompt nuevo.
    pub briefed: bool,
    /// Una tarea devuelta: cuántos tokens tiene la conversación anterior, si se sabe.
    pub previous_tokens: Option<u64>,
    /// Si pasa de `session.resume_max_tokens`: conviene una conversación nueva.
    pub suggest_fresh: bool,
    /// El prompt para una conversación nueva en una tarea devuelta: `prompt` más lo que se
    /// hizo en la sesión anterior (el agente nuevo no lo recuerda).
    pub fresh_prompt: String,
}

/// La carpeta donde se trabaja la tarea.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkDirSetup {
    /// La carpeta: un worktree o el repo principal.
    pub path: PathBuf,
    /// Su rama (`CU-86b1`), si se sabe.
    pub branch: Option<String>,
    /// `false` si es el repo principal (la rama de la tarea está ahí).
    pub is_worktree: bool,
    /// Si se acaba de crear.
    pub created: bool,
    /// Si se puso al día con la base, y cómo salió. `None` si no se intentó.
    pub sync: Option<crate::ports::SyncOutcome>,
}

/// La vista previa del worktree: dónde se ve y cómo se levanta y se detiene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewView {
    /// Dónde se ve la vista previa, con el puerto de la tarea ya puesto.
    pub url: String,
    /// El comando para levantarla.
    pub start: String,
    /// El comando para detenerla, si el proyecto lo define.
    pub stop: Option<String>,
}

impl From<domain::project::RenderedPreview> for PreviewView {
    fn from(preview: domain::project::RenderedPreview) -> Self {
        Self {
            url: preview.url,
            start: preview.start,
            stop: preview.stop,
        }
    }
}

/// Un worktree que se puede quitar: no tiene cambios sin commit y no es el repo principal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemovableWorktree {
    /// El repo principal, desde donde se quita (`git -C <repo> worktree remove`).
    pub repo: PathBuf,
    /// La carpeta del worktree que se quitaría.
    pub path: PathBuf,
}

/// Otro worktree con cambios, donde parece que trabajó el agente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorktreeCandidate {
    /// La carpeta del worktree.
    pub path: PathBuf,
    /// Su rama, o `None` si está en *detached HEAD*.
    pub branch: Option<String>,
    /// Archivos sin commit.
    pub changed_files: usize,
    /// Commits que no están en la base.
    pub commits: usize,
}

/// El reporte de lo que se hizo, listo para revisar y publicar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkReportView {
    /// El reporte en Markdown, tal como se publicaría.
    pub markdown: String,
    /// `work-report.md`: se puede editar antes de publicar.
    pub path: PathBuf,
    /// `true` si lo escribió el mismo agente (retomando su conversación).
    pub from_agent: bool,
    /// Lo que no salió pero no impide publicar.
    pub warnings: Vec<String>,
}

/// Lo que propone `taskflow commit`, para que elijas y confirmes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommitProposal {
    /// El repositorio donde se haría el commit.
    pub repo: PathBuf,
    /// La rama actual, o `None` si está en *detached HEAD*.
    pub branch: Option<String>,
    /// Los archivos con cambios, para elegir cuáles entran.
    pub files: Vec<ChangedFile>,
    /// El mensaje que propone la IA; lo puedes editar.
    pub message: CommitMessage,
    /// Lo que no salió pero no impide hacer el commit.
    pub warnings: Vec<String>,
}

/// El estado de la rama para `taskflow pr`, sin IA.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BranchStatus {
    /// El repositorio de la rama.
    pub repo: PathBuf,
    /// La rama actual (`CU-86b1`).
    pub branch: String,
    /// La rama a la que se integra, sin `origin/` (`master`).
    pub base: String,
    /// Los commits de la rama respecto de la base (`git log --oneline`).
    pub commits: Vec<String>,
    /// El PR abierto de la rama, si ya hay.
    pub pr: Option<PullRequest>,
    /// Si quedan cambios sin commit (no se suben).
    pub uncommitted: bool,
}

/// Lo que propone `taskflow pr` para que lo revises antes de abrirlo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrProposal {
    /// La rama, su base, sus commits y su PR, si tiene.
    pub status: BranchStatus,
    /// El título y la descripción que propone la IA. `None` si la rama ya tiene PR (solo
    /// hace falta el push).
    pub description: Option<PrDescription>,
    /// `true` si lo escribió el mismo agente (retomando su conversación).
    pub from_agent: bool,
    /// Lo que no salió pero no impide abrir el PR.
    pub warnings: Vec<String>,
}
