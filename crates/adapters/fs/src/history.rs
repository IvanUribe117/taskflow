use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use application::ports::{
    HistoryEntry, HistoryFilter, HistoryRecord, HistoryRepository, RecordSubject, RepoError,
    SubjectFilter, markdown_title,
};
use async_trait::async_trait;
use chrono::NaiveDateTime;
use domain::project::ProjectSlug;
use domain::task::TaskId;
use tempfile::NamedTempFile;

/// Carpeta de las tareas sin proyecto y de los planes sin `--project`. El guion bajo no es
/// válido en un slug, así que nunca choca con un proyecto.
const GENERAL: &str = "_general";
const ANALYSES: &str = "analyses";
const PLANS: &str = "plans";
/// `2026-10-14_153012`: se ordena bien como texto y no tiene `:` (inválido en Windows).
const STAMP: &str = "%Y-%m-%d_%H%M%S";
const STAMP_LEN: usize = "2026-10-14_153012".len();

/// `HistoryRepository` sobre archivos Markdown (§14.4):
///
/// ```text
/// <dir>/
/// ├── <slug>/
/// │   ├── analyses/<tarea>/2026-10-14_153012.md
/// │   └── plans/2026-10-14_090010.md
/// └── _general/            tareas sin proyecto y planes de todos los proyectos
/// ```
///
/// Como en los otros repositorios, los métodos son `async` por el puerto pero usan `std::fs`.
#[derive(Debug, Clone)]
pub struct FsHistoryRepository {
    dir: PathBuf,
}

impl FsHistoryRepository {
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// La carpeta de un registro.
    fn folder(&self, subject: &RecordSubject, project: Option<&ProjectSlug>) -> PathBuf {
        let base = self.dir.join(project.map_or(GENERAL, ProjectSlug::as_str));
        match subject {
            RecordSubject::Analysis(id) => base.join(ANALYSES).join(safe_name(id.as_str())),
            RecordSubject::Plan => base.join(PLANS),
        }
    }

    /// Los registros de una carpeta de proyecto (o de `_general`).
    fn entries_in(
        project_dir: &Path,
        project: Option<&ProjectSlug>,
        filter: &HistoryFilter,
        out: &mut Vec<HistoryEntry>,
    ) -> io::Result<()> {
        let plans = project_dir.join(PLANS);
        if filter.matches(&RecordSubject::Plan, project) {
            for (saved_at, path) in documents(&plans)? {
                out.push(entry(RecordSubject::Plan, project, saved_at, path)?);
            }
        }
        for task_dir in subdirs(&project_dir.join(ANALYSES))? {
            let Some(id) = file_name(&task_dir).and_then(|name| TaskId::new(name).ok()) else {
                continue;
            };
            let subject = RecordSubject::Analysis(id);
            if !filter.matches(&subject, project) {
                continue;
            }
            for (saved_at, path) in documents(&task_dir)? {
                out.push(entry(subject.clone(), project, saved_at, path)?);
            }
        }
        Ok(())
    }
}

#[async_trait]
impl HistoryRepository for FsHistoryRepository {
    /// Escribe en un temporal y lo renombra con `persist_noclobber`, que **falla** si el
    /// destino ya existe: si hay otro registro en el mismo segundo, prueba `-2`, `-3`...
    async fn save(&self, record: &HistoryRecord) -> Result<PathBuf, RepoError> {
        let folder = self.folder(&record.subject, record.project.as_ref());
        let storage = |e: io::Error| RepoError::Storage(format!("{}: {e}", folder.display()));
        fs::create_dir_all(&folder).map_err(storage)?;

        let stamp = record.saved_at.format(STAMP).to_string();
        let mut file = NamedTempFile::new_in(&folder).map_err(storage)?;
        file.write_all(record.content.as_bytes()).map_err(storage)?;
        file.as_file().sync_all().map_err(storage)?;
        for attempt in 1.. {
            let name = if attempt == 1 {
                format!("{stamp}.md")
            } else {
                format!("{stamp}-{attempt}.md")
            };
            let path = folder.join(name);
            match file.persist_noclobber(&path) {
                Ok(_) => return Ok(path),
                Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => file = e.file,
                Err(e) => return Err(storage(e.error)),
            }
        }
        unreachable!("`1..` never ends")
    }

    async fn list(&self, filter: &HistoryFilter) -> Result<Vec<HistoryEntry>, RepoError> {
        let storage = |e: io::Error| RepoError::Storage(format!("{}: {e}", self.dir.display()));
        // Los nombres de carpeta de las tareas pasan por `safe_name`: el filtro también.
        let filter = match &filter.subject {
            SubjectFilter::Task(id) => HistoryFilter {
                subject: SubjectFilter::Task(
                    TaskId::new(safe_name(id.as_str())).expect("never blank"),
                ),
                ..filter.clone()
            },
            _ => filter.clone(),
        };

        let mut entries = Vec::new();
        for project_dir in subdirs(&self.dir).map_err(storage)? {
            let project = match file_name(&project_dir) {
                Some(GENERAL) => None,
                // Una carpeta que no es un slug no es de Taskflow: se ignora.
                Some(name) => match name.parse::<ProjectSlug>() {
                    Ok(slug) => Some(slug),
                    Err(_) => continue,
                },
                None => continue,
            };
            Self::entries_in(&project_dir, project.as_ref(), &filter, &mut entries)
                .map_err(storage)?;
        }
        entries.sort_by(|a, b| b.saved_at.cmp(&a.saved_at).then(b.path.cmp(&a.path)));
        Ok(entries)
    }

    async fn read(&self, entry: &HistoryEntry) -> Result<String, RepoError> {
        fs::read_to_string(&entry.path)
            .map_err(|e| RepoError::Storage(format!("{}: {e}", entry.path.display())))
    }
}

/// Un id de tarea como nombre de carpeta: todo lo que no sea letra, dígito, `-` o `_` se
/// cambia por `_`. Los ids de `ClickUp` no cambian; un id raro como `../x` no se sale de
/// la carpeta.
pub(crate) fn safe_name(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn file_name(path: &Path) -> Option<&str> {
    path.file_name().and_then(|name| name.to_str())
}

/// Las subcarpetas de `dir`. Si `dir` no existe, ninguna.
fn subdirs(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    match fs::read_dir(dir) {
        Ok(read) => {
            for item in read {
                let path = item?.path();
                if path.is_dir() {
                    dirs.push(path);
                }
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    Ok(dirs)
}

/// Los `.md` de `dir` cuyo nombre empieza con una fecha válida, con esa fecha.
fn documents(dir: &Path) -> io::Result<Vec<(NaiveDateTime, PathBuf)>> {
    let mut docs = Vec::new();
    let read = match fs::read_dir(dir) {
        Ok(read) => read,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(docs),
        Err(e) => return Err(e),
    };
    for item in read {
        let path = item?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let stamp = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.get(..STAMP_LEN));
        if let Some(saved_at) = stamp.and_then(|s| NaiveDateTime::parse_from_str(s, STAMP).ok()) {
            docs.push((saved_at, path));
        }
    }
    Ok(docs)
}

/// Arma la entrada leyendo el título del documento.
fn entry(
    subject: RecordSubject,
    project: Option<&ProjectSlug>,
    saved_at: NaiveDateTime,
    path: PathBuf,
) -> io::Result<HistoryEntry> {
    let content = fs::read_to_string(&path)?;
    let title = markdown_title(&content)
        .map(str::to_owned)
        .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default();
    Ok(HistoryEntry {
        subject,
        project: project.cloned(),
        saved_at,
        title,
        path,
    })
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    fn at(day: u32, hour: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, day)
            .unwrap()
            .and_hms_opt(hour, 30, 12)
            .unwrap()
    }

    fn analysis(id: &str) -> RecordSubject {
        RecordSubject::Analysis(TaskId::new(id).unwrap())
    }

    fn record(
        subject: RecordSubject,
        project: Option<&str>,
        saved_at: NaiveDateTime,
    ) -> HistoryRecord {
        HistoryRecord {
            subject,
            project: project.map(|p| p.parse().unwrap()),
            saved_at,
            content: format!("# Registro {saved_at}\n\ncuerpo"),
        }
    }

    #[tokio::test]
    async fn saves_by_project_task_and_date() {
        let dir = tempfile::tempdir().unwrap();
        let repo = FsHistoryRepository::new(dir.path());

        let analysis = repo
            .save(&record(analysis("86b1"), Some("idk-base"), at(14, 15)))
            .await
            .unwrap();
        let plan = repo
            .save(&record(RecordSubject::Plan, None, at(14, 9)))
            .await
            .unwrap();

        assert_eq!(
            analysis,
            dir.path()
                .join("idk-base/analyses/86b1/2026-10-14_153012.md")
        );
        assert_eq!(plan, dir.path().join("_general/plans/2026-10-14_093012.md"));
        assert!(
            fs::read_to_string(&analysis)
                .unwrap()
                .starts_with("# Registro")
        );
    }

    #[tokio::test]
    async fn never_overwrites_a_record_from_the_same_second() {
        let dir = tempfile::tempdir().unwrap();
        let repo = FsHistoryRepository::new(dir.path());
        let same = record(RecordSubject::Plan, None, at(14, 9));

        let first = repo.save(&same).await.unwrap();
        let second = repo.save(&same).await.unwrap();
        let third = repo.save(&same).await.unwrap();

        assert_eq!(file_name(&first), Some("2026-10-14_093012.md"));
        assert_eq!(file_name(&second), Some("2026-10-14_093012-2.md"));
        assert_eq!(file_name(&third), Some("2026-10-14_093012-3.md"));
        let files = fs::read_dir(first.parent().unwrap()).unwrap().count();
        assert_eq!(files, 3, "no temporary files left behind");
    }

    #[tokio::test]
    async fn odd_task_ids_stay_inside_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let repo = FsHistoryRepository::new(dir.path());

        let path = repo
            .save(&record(analysis("../../x"), None, at(14, 9)))
            .await
            .unwrap();

        assert!(path.starts_with(dir.path().join("_general/analyses")));
        let filter = HistoryFilter {
            subject: SubjectFilter::Task(TaskId::new("../../x").unwrap()),
            ..HistoryFilter::default()
        };
        assert_eq!(repo.list(&filter).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn lists_newest_first_with_titles_and_filters() {
        let dir = tempfile::tempdir().unwrap();
        let repo = FsHistoryRepository::new(dir.path());
        for rec in [
            record(analysis("1"), Some("acme"), at(13, 10)),
            record(analysis("2"), Some("acme"), at(15, 10)),
            record(RecordSubject::Plan, Some("acme"), at(14, 10)),
            record(RecordSubject::Plan, None, at(16, 10)),
        ] {
            repo.save(&rec).await.unwrap();
        }
        // Ruido que se ignora: otro archivo, una fecha inválida y una carpeta que no es slug.
        fs::write(dir.path().join("acme/plans/notas.txt"), "x").unwrap();
        fs::write(dir.path().join("acme/plans/hoy.md"), "# x").unwrap();
        fs::create_dir_all(dir.path().join("No Es Slug/plans")).unwrap();
        fs::write(
            dir.path().join("No Es Slug/plans/2026-10-20_100000.md"),
            "# x",
        )
        .unwrap();

        let all = repo.list(&HistoryFilter::default()).await.unwrap();
        let acme_plans = repo
            .list(&HistoryFilter {
                project: Some("acme".parse().unwrap()),
                subject: SubjectFilter::Plans,
            })
            .await
            .unwrap();
        let task_one = repo
            .list(&HistoryFilter {
                subject: SubjectFilter::Task(TaskId::new("1").unwrap()),
                ..HistoryFilter::default()
            })
            .await
            .unwrap();

        let days: Vec<u32> = all
            .iter()
            .map(|e| chrono::Datelike::day(&e.saved_at))
            .collect();
        assert_eq!(days, [16, 15, 14, 13]);
        assert_eq!(all[0].project, None);
        assert_eq!(all[0].title, "Registro 2026-10-16 10:30:12");
        assert_eq!(acme_plans.len(), 1);
        assert_eq!(acme_plans[0].saved_at, at(14, 10));
        assert_eq!(task_one.len(), 1);
        assert_eq!(task_one[0].subject, analysis("1"));
        assert_eq!(
            repo.read(&task_one[0]).await.unwrap(),
            "# Registro 2026-10-13 10:30:12\n\ncuerpo"
        );
    }

    #[tokio::test]
    async fn a_missing_folder_is_an_empty_history() {
        let dir = tempfile::tempdir().unwrap();
        let repo = FsHistoryRepository::new(dir.path().join("todavia-no-existe"));

        assert!(
            repo.list(&HistoryFilter::default())
                .await
                .unwrap()
                .is_empty()
        );
    }
}
