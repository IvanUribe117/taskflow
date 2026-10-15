//! `GhPullRequests`: los PR de GitHub con la CLI oficial, `gh`. Usa tu sesión de `gh auth
//! login`; Taskflow no guarda ningún token de GitHub.

use std::path::Path;
use std::process::Stdio;

use application::ports::{PullRequest, PullRequestDraft, PullRequests, RepoError};
use async_trait::async_trait;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Dónde se busca la plantilla de PR, en el orden en que la busca GitHub.
const TEMPLATES: &[&str] = &[
    ".github/pull_request_template.md",
    ".github/PULL_REQUEST_TEMPLATE.md",
    "pull_request_template.md",
    "PULL_REQUEST_TEMPLATE.md",
    "docs/pull_request_template.md",
    "docs/PULL_REQUEST_TEMPLATE.md",
];

#[derive(Debug, Clone)]
pub struct GhPullRequests {
    /// El programa: `gh`, o un sustituto en los tests.
    program: String,
}

impl Default for GhPullRequests {
    fn default() -> Self {
        Self::new("gh")
    }
}

impl GhPullRequests {
    #[must_use]
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    /// `gh <args>` dentro de `repo` (así `gh` sabe de qué repositorio de GitHub se trata),
    /// con `stdin` si hay. Devuelve la salida; si falla, el error trae lo que dijo `gh`.
    async fn gh(
        &self,
        repo: &Path,
        args: &[&str],
        stdin: Option<&str>,
    ) -> Result<String, RepoError> {
        let mut child = Command::new(&self.program)
            .args(args)
            .current_dir(repo)
            .env("GH_PROMPT_DISABLED", "1")
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                RepoError::Storage(format!(
                    "could not run `{}` ({e}): install the GitHub CLI (https://cli.github.com) and run `gh auth login`",
                    self.program
                ))
            })?;
        if let (Some(text), Some(mut input)) = (stdin, child.stdin.take()) {
            input
                .write_all(text.as_bytes())
                .await
                .map_err(|e| RepoError::Storage(format!("gh: {e}")))?;
        }
        let output = child
            .wait_with_output()
            .await
            .map_err(|e| RepoError::Storage(format!("gh: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let said = if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            };
            return Err(RepoError::Invalid(format!(
                "gh {} failed ({}): {said}",
                args[..2.min(args.len())].join(" "),
                output.status
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

/// `"42 https://github.com/o/r/pull/42"` → el PR.
fn parse_line(line: &str) -> Option<PullRequest> {
    let (number, url) = line.trim().split_once(' ')?;
    Some(PullRequest {
        number: number.parse().ok()?,
        url: url.trim().to_owned(),
    })
}

/// La URL que imprime `gh pr create` (la última línea) → el PR; el número sale de la URL.
fn parse_created(stdout: &str) -> Option<PullRequest> {
    let url = stdout.lines().rev().find(|l| l.starts_with("http"))?.trim();
    let number = url.rsplit('/').next()?.parse().ok()?;
    Some(PullRequest {
        number,
        url: url.to_owned(),
    })
}

#[async_trait]
impl PullRequests for GhPullRequests {
    /// `gh pr list --head <rama> --state open`: el primero, como `número url`.
    async fn find(&self, repo: &Path, branch: &str) -> Result<Option<PullRequest>, RepoError> {
        let out = self
            .gh(
                repo,
                &[
                    "pr",
                    "list",
                    "--head",
                    branch,
                    "--state",
                    "open",
                    "--limit",
                    "1",
                    "--json",
                    "number,url",
                    "--jq",
                    r#".[] | "\(.number) \(.url)""#,
                ],
                None,
            )
            .await?;
        Ok(out.lines().next().and_then(parse_line))
    }

    async fn template(&self, repo: &Path) -> Result<Option<String>, RepoError> {
        for candidate in TEMPLATES {
            if let Ok(text) = tokio::fs::read_to_string(repo.join(candidate)).await
                && !text.trim().is_empty()
            {
                return Ok(Some(text));
            }
        }
        Ok(None)
    }

    /// `gh pr create --base --head --title --body-file -` (la descripción por la entrada
    /// estándar: puede ser larga y tener cualquier carácter).
    async fn create(
        &self,
        repo: &Path,
        draft: &PullRequestDraft,
    ) -> Result<PullRequest, RepoError> {
        let mut args = vec![
            "pr",
            "create",
            "--base",
            &draft.base,
            "--head",
            &draft.head,
            "--title",
            &draft.title,
            "--body-file",
            "-",
        ];
        if draft.draft {
            args.push("--draft");
        }
        let out = self.gh(repo, &args, Some(&draft.body)).await?;
        parse_created(&out).ok_or_else(|| {
            RepoError::Invalid(format!("unexpected output from gh pr create: {out}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// Un `gh` falso: guarda sus argumentos y su entrada en la carpeta y responde `reply`.
    fn fake_gh(dir: &Path, reply: &str, exit: i32) -> String {
        let script = dir.join("gh");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{0}/args\"\ncat > \"{0}/stdin\"\nprintf '%s\\n' '{reply}'\nexit {exit}\n",
                dir.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        script.to_string_lossy().into_owned()
    }

    #[tokio::test]
    async fn creates_the_pr_with_the_body_on_stdin() {
        let dir = tempfile::tempdir().unwrap();
        let gh = GhPullRequests::new(fake_gh(
            dir.path(),
            "https://github.com/acme/idk/pull/4033",
            0,
        ));

        let pr = gh
            .create(
                dir.path(),
                &PullRequestDraft {
                    base: "master".into(),
                    head: "CU-1".into(),
                    title: "CU-1 - fix: x".into(),
                    body: "## What changed\n\n`a` & 'b'".into(),
                    draft: true,
                },
            )
            .await
            .unwrap();

        assert_eq!(pr.number, 4033);
        assert_eq!(pr.url, "https://github.com/acme/idk/pull/4033");
        let args = fs::read_to_string(dir.path().join("args")).unwrap();
        assert_eq!(
            args,
            "pr\ncreate\n--base\nmaster\n--head\nCU-1\n--title\nCU-1 - fix: x\n--body-file\n-\n--draft\n"
        );
        let stdin = fs::read_to_string(dir.path().join("stdin")).unwrap();
        assert_eq!(stdin, "## What changed\n\n`a` & 'b'");
    }

    #[tokio::test]
    async fn finds_the_open_pr_of_a_branch() {
        let dir = tempfile::tempdir().unwrap();
        let found = GhPullRequests::new(fake_gh(
            dir.path(),
            "7 https://github.com/acme/idk/pull/7",
            0,
        ));

        let pr = found.find(dir.path(), "CU-1").await.unwrap();

        assert_eq!(pr.map(|p| p.number), Some(7));
        let args = fs::read_to_string(dir.path().join("args")).unwrap();
        assert!(args.contains("--head\nCU-1\n--state\nopen\n"), "{args}");
    }

    #[tokio::test]
    async fn gh_errors_and_a_missing_gh_are_explained() {
        let dir = tempfile::tempdir().unwrap();
        let failing = GhPullRequests::new(fake_gh(dir.path(), "", 1));
        let missing = GhPullRequests::new(dir.path().join("nope").to_string_lossy());

        let failed = failing.find(dir.path(), "CU-1").await.unwrap_err();
        let absent = missing.find(dir.path(), "CU-1").await.unwrap_err();

        assert!(failed.to_string().contains("gh pr list failed"), "{failed}");
        assert!(absent.to_string().contains("gh auth login"), "{absent}");
    }

    #[tokio::test]
    async fn reads_the_repo_template() {
        let dir = tempfile::tempdir().unwrap();
        let gh = GhPullRequests::default();
        let none = gh.template(dir.path()).await.unwrap();
        fs::create_dir(dir.path().join(".github")).unwrap();
        fs::write(
            dir.path().join(".github/pull_request_template.md"),
            "## What changed\n",
        )
        .unwrap();

        assert!(none.is_none());
        assert_eq!(
            gh.template(dir.path()).await.unwrap().as_deref(),
            Some("## What changed\n")
        );
    }

    #[test]
    fn parses_gh_output() {
        assert_eq!(
            parse_line("12 https://x/pull/12").map(|p| p.number),
            Some(12)
        );
        assert!(parse_line("").is_none());
        assert_eq!(
            parse_created("Creating pull request…\nhttps://github.com/a/b/pull/3\n")
                .map(|p| p.number),
            Some(3)
        );
    }
}
