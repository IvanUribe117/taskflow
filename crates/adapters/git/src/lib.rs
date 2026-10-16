//! `GitRepoInspector`, sobre el programa `git`:
//!
//! - `RepoInspector`: lee la rama de trabajo (`rev-parse`, `merge-base`, `log`, `diff`,
//!   `status`). Nunca cambia nada.
//! - `RepoCommitter`: `git add` y `git commit` **solo** de los archivos que elegiste, para
//!   `taskflow commit`, y `git push` para `taskflow pr`.
//! - `Worktrees`: una carpeta y una rama por tarea (`git worktree`).
//!
//! Y [`GhPullRequests`] (módulo `github`): los PR de GitHub con `gh`.
#![warn(clippy::pedantic)]
#![warn(missing_docs)]

mod github;
mod run;

pub use github::GhPullRequests;

use std::path::{Path, PathBuf};

use application::ports::{
    ChangedFile, RepoChanges, RepoCommitter, RepoError, RepoInspector, SyncOutcome, Worktree,
    Worktrees,
};
use async_trait::async_trait;

/// Máximo de commits que se listan.
const MAX_COMMITS: usize = 50;

/// Los puertos de git (`RepoInspector`, `RepoCommitter`, `Worktrees`) con el programa `git`.
#[derive(Debug, Clone, Default)]
pub struct GitRepoInspector;

/// `git -C <repo> <args>` ([`run::git_command`]). `Ok(None)` si `git` termina con error
/// (no es un repo, no existe la referencia…); `Err` solo si no se puede ejecutar o se pasa
/// del tiempo.
async fn git(repo: &Path, args: &[&str]) -> Result<Option<String>, RepoError> {
    let mut command = run::git_command(repo);
    command.args(args);
    let output = run::run(command, None, limit(args), "git").await?;
    Ok(output.status.success().then(|| run::stdout(&output)))
}

/// Como [`git`], pero si falla el error trae lo que dijo git (`what` dice qué se intentaba).
async fn git_checked(repo: &Path, args: &[&str], what: &str) -> Result<String, RepoError> {
    let mut command = run::git_command(repo);
    command.args(args);
    let output = run::run(command, None, limit(args), what).await?;
    if output.status.success() {
        Ok(run::stdout(&output))
    } else {
        Err(run::failure(what, &output))
    }
}

/// `fetch` y `push` van por la red: más tiempo. El resto es local.
fn limit(args: &[&str]) -> std::time::Duration {
    match args.first() {
        Some(&("fetch" | "push")) => run::NETWORK,
        _ => run::LOCAL,
    }
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
        let stat = git(repo, &["diff", "--no-ext-diff", "--stat", &against])
            .await?
            .unwrap_or_default();
        let diff = git(repo, &["diff", "--no-ext-diff", &against])
            .await?
            .unwrap_or_default();
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
        let mut diff = git(repo, &["diff", "--no-ext-diff", "HEAD"])
            .await?
            .unwrap_or_default();
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
        git_checked(repo, &add, "git add").await?;

        let mut command = run::git_command(repo);
        command.args(["commit", "-F", "-", "--"]).args(files);
        let output = run::run(command, Some(message), run::LOCAL, "git commit").await?;
        if !output.status.success() {
            return Err(run::failure("git commit", &output));
        }
        git(repo, &["rev-parse", "--short", "HEAD"])
            .await?
            .ok_or_else(|| RepoError::Invalid("git rev-parse failed".into()))
    }

    /// `git push -u origin HEAD`. Sin terminal para preguntar credenciales: si hacen falta,
    /// falla con el mensaje de git en vez de quedarse esperando.
    async fn push(&self, repo: &Path) -> Result<(), RepoError> {
        git_checked(repo, &["push", "-u", "origin", "HEAD"], "git push").await?;
        Ok(())
    }
}

/// `git worktree list --porcelain`: bloques separados por una línea vacía, con
/// `worktree <ruta>` y `branch refs/heads/<rama>` (o `detached`).
fn parse_worktrees(raw: &str) -> Vec<Worktree> {
    let mut list = Vec::new();
    for block in raw.split("\n\n") {
        let mut path = None;
        let mut branch = None;
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("worktree ") {
                path = Some(PathBuf::from(rest));
            } else if let Some(rest) = line.strip_prefix("branch ") {
                branch = Some(rest.trim_start_matches("refs/heads/").to_owned());
            }
        }
        if let Some(path) = path {
            list.push(Worktree { path, branch });
        }
    }
    list
}

async fn has_ref(repo: &Path, name: &str) -> Result<bool, RepoError> {
    Ok(git(repo, &["rev-parse", "--verify", "--quiet", name])
        .await?
        .is_some())
}

#[async_trait]
impl Worktrees for GitRepoInspector {
    async fn list(&self, repo: &Path) -> Result<Vec<Worktree>, RepoError> {
        let raw = git(repo, &["worktree", "list", "--porcelain"])
            .await?
            .unwrap_or_default();
        Ok(parse_worktrees(&raw))
    }

    /// 1. `git fetch origin`, para partir de la base al día. Si falla (sin red, sin
    ///    `origin`), se sigue con lo que hay.
    /// 2. `git worktree add`: con la rama local si existe; si solo está en `origin`, desde
    ///    esa; si no, una rama nueva desde la base, sin seguirla (`--no-track`: su upstream
    ///    será `origin/<rama>` al hacer push, no la base).
    async fn add(&self, repo: &Path, path: &Path, branch: &str) -> Result<(), RepoError> {
        let _ = git_checked(repo, &["fetch", "--quiet", "origin"], "git fetch").await;
        let target = path.to_string_lossy();
        let remote = format!("origin/{branch}");
        let args: Vec<String> = if has_ref(repo, &format!("refs/heads/{branch}")).await? {
            vec![target.into_owned(), branch.to_owned()]
        } else if has_ref(repo, &format!("refs/remotes/{remote}")).await? {
            vec!["-b".into(), branch.to_owned(), target.into_owned(), remote]
        } else {
            let mut args = vec!["--no-track".into(), "-b".into(), branch.to_owned()];
            args.push(target.into_owned());
            args.extend(base_branch(repo).await?);
            args
        };
        let mut full = vec!["worktree", "add", "--quiet"];
        full.extend(args.iter().map(String::as_str));
        git_checked(repo, &full, "git worktree add").await?;
        Ok(())
    }

    async fn remove(&self, repo: &Path, path: &Path) -> Result<(), RepoError> {
        let target = path.to_string_lossy();
        git_checked(
            repo,
            &["worktree", "remove", &target],
            "git worktree remove",
        )
        .await?;
        Ok(())
    }

    /// 1. `git fetch origin` (si falla, con lo que hay).
    /// 2. Sin base, o en la base misma → `NoBase`.
    /// 3. Si `@{u}` es la base, `git branch --unset-upstream`.
    /// 4. Con cambios en archivos que git conoce → `SkippedDirty` (los nuevos sin `git add`
    ///    no estorban: si chocaran, git se niega y es un `Conflict`).
    /// 5. `git rev-list --count HEAD..<base>`: 0 → `UpToDate`.
    /// 6. `git merge --no-edit <base>`; si falla, `git merge --abort` → `Conflict`.
    async fn sync_with_base(&self, path: &Path) -> Result<SyncOutcome, RepoError> {
        let _ = git_checked(path, &["fetch", "--quiet", "origin"], "git fetch").await;
        let (Some(base), Some(branch)) = (
            base_branch(path).await?,
            git(path, &["rev-parse", "--abbrev-ref", "HEAD"]).await?,
        ) else {
            return Ok(SyncOutcome::NoBase);
        };
        if branch == "HEAD" || base.trim_start_matches("origin/") == branch {
            return Ok(SyncOutcome::NoBase);
        }
        if git(path, &["rev-parse", "--abbrev-ref", "@{u}"])
            .await?
            .as_deref()
            == Some(&base)
        {
            let _ = git(path, &["branch", "--unset-upstream"]).await?;
        }
        let tracked_changes = git(path, &["status", "--porcelain", "--untracked-files=no"])
            .await?
            .unwrap_or_default();
        if !tracked_changes.trim().is_empty() {
            return Ok(SyncOutcome::SkippedDirty);
        }
        let range = format!("HEAD..{base}");
        let behind: usize = git(path, &["rev-list", "--count", &range])
            .await?
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or(0);
        if behind == 0 {
            return Ok(SyncOutcome::UpToDate);
        }
        if git_checked(path, &["merge", "--no-edit", &base], "git merge")
            .await
            .is_err()
        {
            let _ = git(path, &["merge", "--abort"]).await?;
            return Ok(SyncOutcome::Conflict { base });
        }
        Ok(SyncOutcome::Updated {
            base,
            commits: behind,
        })
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
    fn parses_the_worktree_list() {
        let raw = "worktree /code/idk\nHEAD abc\nbranch refs/heads/master\n\n\
                   worktree /code/idk-worktrees/CU-1\nHEAD def\nbranch refs/heads/CU-1\n\n\
                   worktree /tmp/x\nHEAD 123\ndetached\n";

        let list = parse_worktrees(raw);

        assert_eq!(list.len(), 3);
        assert_eq!(list[0].path, PathBuf::from("/code/idk"));
        assert_eq!(list[1].branch.as_deref(), Some("CU-1"));
        assert_eq!(list[2].branch, None);
    }

    #[tokio::test]
    async fn adds_a_worktree_from_the_base_and_reuses_a_branch() {
        let dir = repo().await; // En `CU-1`, con `main` como base.
        let root = tempfile::tempdir().unwrap();
        let new = root.path().join("wt/CU-2");
        let again = root.path().join("wt/CU-3");

        GitRepoInspector
            .add(dir.path(), &new, "CU-2")
            .await
            .unwrap();
        run(dir.path(), &["branch", "CU-3", "CU-1"]).await;
        GitRepoInspector
            .add(dir.path(), &again, "CU-3")
            .await
            .unwrap();

        let list = GitRepoInspector.list(dir.path()).await.unwrap();
        let branches: Vec<_> = list.iter().map(|w| w.branch.as_deref()).collect();
        assert_eq!(branches, [Some("CU-1"), Some("CU-2"), Some("CU-3")]);
        // CU-2 sale de `main` (sin el commit de CU-1) y no sigue a ninguna rama.
        let log = git(&new, &["log", "--oneline"]).await.unwrap().unwrap();
        assert_eq!(log.lines().count(), 1, "{log}");
        assert!(git(&new, &["rev-parse", "@{u}"]).await.unwrap().is_none());
        // CU-3 ya existía: el worktree la usa tal cual.
        let log = git(&again, &["log", "--oneline", "-1"])
            .await
            .unwrap()
            .unwrap();
        assert!(log.ends_with("fondo del login"), "{log}");
    }

    #[tokio::test]
    async fn a_dirty_worktree_is_not_removed() {
        let dir = repo().await;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("CU-2");
        GitRepoInspector
            .add(dir.path(), &path, "CU-2")
            .await
            .unwrap();
        fs::write(path.join("login.html"), "cambio").unwrap();

        let refused = GitRepoInspector
            .remove(dir.path(), &path)
            .await
            .unwrap_err();
        run(&path, &["checkout", "--", "login.html"]).await;
        GitRepoInspector.remove(dir.path(), &path).await.unwrap();

        assert!(
            refused.to_string().contains("git worktree remove failed"),
            "{refused}"
        );
        assert!(!path.exists());
        assert!(
            has_ref(dir.path(), "refs/heads/CU-2").await.unwrap(),
            "the branch stays"
        );
    }

    #[tokio::test]
    async fn a_branch_checked_out_elsewhere_brings_git_output() {
        let dir = repo().await; // `CU-1` ya está en el repo principal.
        let root = tempfile::tempdir().unwrap();

        let error = GitRepoInspector
            .add(dir.path(), &root.path().join("CU-1"), "CU-1")
            .await
            .unwrap_err();

        assert!(
            error.to_string().contains("git worktree add failed"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn syncs_the_task_branch_with_the_base() {
        let dir = repo().await; // `CU-1` con 1 commit sobre `main`, y `site.css` preparado.
        let path = dir.path();
        run(path, &["commit", "-q", "-m", "css"]).await;
        run(path, &["checkout", "-q", "main"]).await;
        fs::write(path.join("readme.md"), "base").unwrap();
        run(path, &["add", "readme.md"]).await;
        run(path, &["commit", "-q", "-m", "otro en main"]).await;
        run(path, &["checkout", "-q", "CU-1"]).await;
        // CU-1 sigue a main, como la rama que se subió a master sin querer.
        run(path, &["branch", "--set-upstream-to", "main"]).await;

        let first = GitRepoInspector.sync_with_base(path).await.unwrap();
        let again = GitRepoInspector.sync_with_base(path).await.unwrap();

        assert_eq!(
            first,
            SyncOutcome::Updated {
                base: "main".into(),
                commits: 1
            }
        );
        assert_eq!(again, SyncOutcome::UpToDate);
        assert!(path.join("readme.md").exists());
        assert!(
            git(path, &["rev-parse", "--abbrev-ref", "@{u}"])
                .await
                .unwrap()
                .is_none(),
            "no longer tracks the base"
        );
    }

    #[tokio::test]
    async fn a_dirty_or_conflicting_branch_is_left_as_it_was() {
        let dir = repo().await;
        let path = dir.path();
        run(path, &["commit", "-q", "-m", "css"]).await;
        run(path, &["checkout", "-q", "main"]).await;
        fs::write(path.join("login.html"), "<body class=\"otro\">\n").unwrap();
        run(path, &["commit", "-q", "-am", "choca"]).await;
        run(path, &["checkout", "-q", "CU-1"]).await;
        let head = git(path, &["rev-parse", "HEAD"]).await.unwrap();

        fs::write(path.join("site.css"), "sin commit").unwrap();
        let dirty = GitRepoInspector.sync_with_base(path).await.unwrap();
        run(path, &["checkout", "--", "site.css"]).await;
        let conflict = GitRepoInspector.sync_with_base(path).await.unwrap();

        assert_eq!(dirty, SyncOutcome::SkippedDirty);
        assert_eq!(
            conflict,
            SyncOutcome::Conflict {
                base: "main".into()
            }
        );
        assert_eq!(git(path, &["rev-parse", "HEAD"]).await.unwrap(), head);
        let status = git(path, &["status", "--porcelain"])
            .await
            .unwrap()
            .unwrap();
        assert!(status.is_empty(), "the merge was aborted: {status}");
    }

    #[test]
    fn long_diffs_are_cut_by_characters() {
        assert_eq!(truncate("ñandú", 10), "ñandú");
        assert_eq!(truncate("ñandú", 2), "ña\n… (diff recortado)");
    }
}
