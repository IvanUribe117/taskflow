//! `GitRepoInspector`, sobre el programa `git`:
//!
//! - `RepoInspector`: lee la rama de trabajo (`rev-parse`, `merge-base`, `log`, `diff`,
//!   `status`). Nunca cambia nada.
//! - `RepoCommitter`: `git add` y `git commit` **solo** de los archivos que elegiste, para
//!   `taskflow commit`, y `git push` para `taskflow pr`.
//!
//! Y `GhPullRequests` (módulo [`github`]): los PR de GitHub con `gh`.
#![warn(clippy::pedantic)]

mod github;

pub use github::GhPullRequests;

use std::path::Path;

use std::process::Stdio;

use application::ports::{ChangedFile, RepoChanges, RepoCommitter, RepoError, RepoInspector};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Máximo de commits que se listan.
const MAX_COMMITS: usize = 50;

#[derive(Debug, Clone, Default)]
pub struct GitRepoInspector;

/// `git -C <repo> <args>`. `Ok(None)` si `git` termina con error (no es un repo, no existe
/// la referencia…); `Err` solo si no se puede ejecutar `git`.
async fn git(repo: &Path, args: &[&str]) -> Result<Option<String>, RepoError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .await
        .map_err(|e| RepoError::Storage(format!("git: {e}")))?;
    Ok(output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_owned()
    }))
}

/// La rama base: la que apunta `origin/HEAD` (`origin/master`), o la primera que exista de
/// `origin/main`, `origin/master`, `main`, `master`.
async fn base_branch(repo: &Path) -> Result<Option<String>, RepoError> {
    if let Some(head) = git(repo, &["rev-parse", "--abbrev-ref", "origin/HEAD"]).await?
        && head != "origin/HEAD"
    {
        return Ok(Some(head));
    }
    for candidate in ["origin/main", "origin/master", "main", "master"] {
        if git(repo, &["rev-parse", "--verify", "--quiet", candidate])
            .await?
            .is_some()
        {
            return Ok(Some(candidate.to_owned()));
        }
    }
    Ok(None)
}

/// Los primeros `max` caracteres (no bytes: no corta una letra por la mitad).
fn truncate(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((cut, _)) => format!("{}\n… (diff recortado)", &text[..cut]),
        None => text.to_owned(),
    }
}

#[async_trait]
impl RepoInspector for GitRepoInspector {
    /// - Sin base: solo los cambios sin commit (`git diff HEAD`).
    /// - Con base: commits desde `merge-base HEAD <base>` y el diff contra ese punto, que
    ///   incluye lo que no tiene commit. Los archivos nuevos sin `git add` no aparecen en el
    ///   diff, pero sí cuentan como "cambios sin commit".
    async fn changes(
        &self,
        repo: &Path,
        max_diff_chars: usize,
    ) -> Result<Option<RepoChanges>, RepoError> {
        let Some(branch) = git(repo, &["rev-parse", "--abbrev-ref", "HEAD"]).await? else {
            return Ok(None); // No es un repositorio de git.
        };
        let base = base_branch(repo).await?;
        let fork_point = match &base {
            Some(base) => git(repo, &["merge-base", "HEAD", base]).await?,
            None => None,
        };
        let against = fork_point.clone().unwrap_or_else(|| "HEAD".to_owned());

        let commits = match &fork_point {
            Some(point) => {
                let range = format!("{point}..HEAD");
                git(repo, &["log", "--oneline", "--no-decorate", &range])
                    .await?
                    .unwrap_or_default()
                    .lines()
                    .take(MAX_COMMITS)
                    .map(str::to_owned)
                    .collect()
            }
            None => Vec::new(),
        };
        let stat = git(repo, &["diff", "--stat", &against])
            .await?
            .unwrap_or_default();
        let diff = git(repo, &["diff", &against]).await?.unwrap_or_default();
        let status = git(repo, &["status", "--porcelain"])
            .await?
            .unwrap_or_default();

        Ok(Some(RepoChanges {
            branch,
            base,
            commits,
            stat,
            diff: truncate(&diff, max_diff_chars),
            uncommitted: !status.trim().is_empty(),
        }))
    }
}

/// `git status --porcelain=v1 -z -uall`: entradas `XY ruta` separadas por NUL. Un
/// renombre (`R`) o copia (`C`) trae además la ruta original en el campo siguiente.
/// `-z` evita que git ponga comillas en rutas con espacios o tildes.
fn parse_status(raw: &str) -> Vec<ChangedFile> {
    let mut files = Vec::new();
    let mut fields = raw.split('\0').filter(|f| !f.is_empty());
    while let Some(entry) = fields.next() {
        let Some((status, path)) = entry.split_at_checked(3) else {
            continue;
        };
        let status = status[..2].to_owned();
        if status.starts_with('R') || status.starts_with('C') {
            fields.next(); // La ruta original.
        }
        files.push(ChangedFile {
            path: path.to_owned(),
            untracked: status == "??",
            status,
        });
    }
    files
}

#[async_trait]
impl RepoCommitter for GitRepoInspector {
    async fn changed_files(&self, repo: &Path) -> Result<Vec<ChangedFile>, RepoError> {
        let raw = git(repo, &["status", "--porcelain=v1", "-z", "-uall"])
            .await?
            .unwrap_or_default();
        Ok(parse_status(&raw))
    }

    /// El diff contra `HEAD` y, al final, los archivos nuevos (su contenido no sale en el
    /// diff hasta hacer `git add`).
    async fn pending_diff(&self, repo: &Path, max_chars: usize) -> Result<String, RepoError> {
        let mut diff = git(repo, &["diff", "HEAD"]).await?.unwrap_or_default();
        let untracked: Vec<String> = self
            .changed_files(repo)
            .await?
            .into_iter()
            .filter(|f| f.untracked)
            .map(|f| f.path)
            .collect();
        if !untracked.is_empty() {
            diff.push_str("\n\nArchivos nuevos (sin git add):\n");
            diff.push_str(&untracked.join("\n"));
        }
        Ok(truncate(&diff, max_chars))
    }

    async fn recent_commits(&self, repo: &Path, n: usize) -> Result<Vec<String>, RepoError> {
        let count = n.to_string();
        let log = git(repo, &["log", "--oneline", "--no-decorate", "-n", &count])
            .await?
            .unwrap_or_default();
        Ok(log.lines().map(str::to_owned).collect())
    }

    /// 1. `git add -A -- <archivos>` (también registra los borrados).
    /// 2. `git commit -F - -- <archivos>`: el mensaje por la entrada estándar, y **solo**
    ///    esos archivos, aunque hubiera otros preparados con `git add`.
    /// 3. `git rev-parse --short HEAD`.
    ///
    /// Los hooks del repo (pre-commit) se ejecutan como siempre; si uno falla, el error
    /// trae su salida.
    async fn commit(
        &self,
        repo: &Path,
        files: &[String],
        message: &str,
    ) -> Result<String, RepoError> {
        let mut add: Vec<&str> = vec!["add", "-A", "--"];
        add.extend(files.iter().map(String::as_str));
        if git(repo, &add).await?.is_none() {
            return Err(RepoError::Invalid("git add failed".into()));
        }

        let mut child = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["commit", "-F", "-", "--"])
            .args(files)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RepoError::Storage(format!("git: {e}")))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(message.as_bytes())
                .await
                .map_err(|e| RepoError::Storage(format!("git: {e}")))?;
        }
        let output = child
            .wait_with_output()
            .await
            .map_err(|e| RepoError::Storage(format!("git: {e}")))?;
        if !output.status.success() {
            let text = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
            return Err(RepoError::Invalid(format!(
                "git commit failed:\n{}",
                text.trim()
            )));
        }
        git(repo, &["rev-parse", "--short", "HEAD"])
            .await?
            .ok_or_else(|| RepoError::Invalid("git rev-parse failed".into()))
    }

    /// `git push -u origin HEAD`. Sin terminal para preguntar credenciales: si hacen falta,
    /// falla con el mensaje de git en vez de quedarse esperando.
    async fn push(&self, repo: &Path) -> Result<(), RepoError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["push", "-u", "origin", "HEAD"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| RepoError::Storage(format!("git: {e}")))?;
        if output.status.success() {
            return Ok(());
        }
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Err(RepoError::Invalid(format!(
            "git push failed:\n{}",
            text.trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    async fn run(repo: &Path, args: &[&str]) {
        assert!(
            git(repo, args).await.unwrap().is_some(),
            "git {args:?} failed"
        );
    }

    /// Un repo con `main` (1 commit) y una rama `CU-1` con 1 commit más y un cambio sin
    /// commit.
    async fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        run(path, &["init", "-q", "-b", "main"]).await;
        run(path, &["config", "user.email", "t@example.test"]).await;
        run(path, &["config", "user.name", "Test"]).await;
        fs::write(path.join("login.html"), "<body>\n").unwrap();
        run(path, &["add", "."]).await;
        run(path, &["commit", "-q", "-m", "inicio"]).await;
        run(path, &["checkout", "-q", "-b", "CU-1"]).await;
        fs::write(path.join("login.html"), "<body class=\"bg\">\n").unwrap();
        run(path, &["commit", "-q", "-am", "fondo del login"]).await;
        fs::write(path.join("site.css"), "x").unwrap();
        run(path, &["add", "site.css"]).await;
        dir
    }

    #[tokio::test]
    async fn reads_branch_commits_and_diff_against_the_base() {
        let dir = repo().await;

        let changes = GitRepoInspector
            .changes(dir.path(), 10_000)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(changes.branch, "CU-1");
        assert_eq!(changes.base.as_deref(), Some("main"));
        assert_eq!(changes.commits.len(), 1);
        assert!(changes.commits[0].ends_with("fondo del login"));
        assert!(changes.stat.contains("login.html"), "{}", changes.stat);
        assert!(changes.stat.contains("site.css"), "staged changes count");
        assert!(
            changes.diff.contains("+<body class=\"bg\">"),
            "{}",
            changes.diff
        );
        assert!(changes.uncommitted);
    }

    #[tokio::test]
    async fn a_folder_that_is_not_a_repo_has_no_changes() {
        let dir = tempfile::tempdir().unwrap();

        assert!(
            GitRepoInspector
                .changes(dir.path(), 100)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn status_entries_with_renames_and_spaces() {
        let raw = " M a b.html\0R  nuevo.css\0viejo.css\0?? docs/ñ.md\0";

        let files = parse_status(raw);

        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["a b.html", "nuevo.css", "docs/ñ.md"]);
        assert_eq!(files[1].status, "R ");
        assert!(files[2].untracked);
        assert!(!files[0].untracked);
    }

    #[tokio::test]
    async fn commits_only_the_chosen_files() {
        let dir = repo().await; // login.html con commit en CU-1; site.css preparado.
        let path = dir.path();
        fs::write(path.join("login.html"), "<body class=\"bg2\">\n").unwrap();
        fs::write(path.join("notas.md"), "mías").unwrap();

        let files = GitRepoInspector.changed_files(path).await.unwrap();
        let hash = GitRepoInspector
            .commit(
                path,
                &["login.html".into()],
                "CU-1 - feat: otro fondo\n\nCuerpo.",
            )
            .await
            .unwrap();

        let names: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(names, ["login.html", "site.css", "notas.md"]);
        assert!(!hash.is_empty());
        let last = git(path, &["log", "-1", "--format=%B"])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(last, "CU-1 - feat: otro fondo\n\nCuerpo.");
        let left: Vec<String> = GitRepoInspector
            .changed_files(path)
            .await
            .unwrap()
            .into_iter()
            .map(|f| f.path)
            .collect();
        assert_eq!(left, ["site.css", "notas.md"], "the rest stays as it was");
        let recent = GitRepoInspector.recent_commits(path, 5).await.unwrap();
        assert_eq!(recent.len(), 3);
        assert!(recent[0].ends_with("CU-1 - feat: otro fondo"));
    }

    #[tokio::test]
    async fn push_sets_the_upstream_of_the_branch() {
        let dir = repo().await;
        let remote = tempfile::tempdir().unwrap();
        run(remote.path(), &["init", "-q", "--bare"]).await;
        let url = remote.path().to_str().unwrap();
        run(dir.path(), &["remote", "add", "origin", url]).await;

        GitRepoInspector.push(dir.path()).await.unwrap();

        let upstream = git(dir.path(), &["rev-parse", "--abbrev-ref", "@{u}"])
            .await
            .unwrap();
        assert_eq!(upstream.as_deref(), Some("origin/CU-1"));
        let pushed = git(remote.path(), &["log", "--oneline", "-1", "CU-1"])
            .await
            .unwrap()
            .unwrap();
        assert!(pushed.ends_with("fondo del login"), "{pushed}");
    }

    #[tokio::test]
    async fn a_failed_push_brings_git_output() {
        let dir = repo().await; // Sin `origin`.

        let error = GitRepoInspector.push(dir.path()).await.unwrap_err();

        assert!(error.to_string().contains("git push failed"), "{error}");
    }

    #[tokio::test]
    async fn pending_diff_lists_new_files() {
        let dir = repo().await;
        fs::write(dir.path().join("notas.md"), "x").unwrap();

        let diff = GitRepoInspector
            .pending_diff(dir.path(), 10_000)
            .await
            .unwrap();

        assert!(diff.contains("site.css"), "{diff}");
        assert!(
            diff.ends_with("Archivos nuevos (sin git add):\nnotas.md"),
            "{diff}"
        );
    }

    #[test]
    fn long_diffs_are_cut_by_characters() {
        assert_eq!(truncate("ñandú", 10), "ñandú");
        assert_eq!(truncate("ñandú", 2), "ña\n… (diff recortado)");
    }
}
