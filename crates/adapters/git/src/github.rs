//! `GhPullRequests`: los PR de GitHub con la CLI oficial, `gh`. Usa tu sesión de `gh auth
//! login`; Taskflow no guarda ningún token de GitHub.

use std::path::Path;

use application::ports::{PullRequest, PullRequestDraft, PullRequests, RepoError};
use async_trait::async_trait;
use tokio::process::Command;

use crate::run;

/// Dónde se busca la plantilla de PR, en el orden en que la busca GitHub.
const TEMPLATES: &[&str] = &[
    ".github/pull_request_template.md",
    ".github/PULL_REQUEST_TEMPLATE.md",
    "pull_request_template.md",
    "PULL_REQUEST_TEMPLATE.md",
    "docs/pull_request_template.md",
    "docs/PULL_REQUEST_TEMPLATE.md",
];

/// Los pull requests de GitHub (`impl PullRequests`), con la CLI `gh`.
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
    /// Con `program` en lugar de `gh` (los tests usan un `gh` falso).
    #[must_use]
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    /// `gh <args>` dentro de `repo` (así `gh` sabe de qué repositorio de GitHub se trata),
    /// con `stdin` si hay y el tiempo límite de la red. Devuelve la salida; si falla, el
    /// error trae lo que dijo `gh` (ver [`crate::run::run`]).
    async fn gh(
        &self,
        repo: &Path,
        args: &[&str],
        stdin: Option<&str>,
    ) -> Result<String, RepoError> {
        let mut command = Command::new(&self.program);
        command
            .args(args)
            .current_dir(repo)
            .env("GH_PROMPT_DISABLED", "1");
        let what = format!("gh {}", args[..2.min(args.len())].join(" "));
        let output = run::run(command, stdin, run::NETWORK, &what)
            .await
            .map_err(|error| match error {
                RepoError::Storage(_) => RepoError::Storage(format!(
                    "could not run `{}` ({error}): install the GitHub CLI (https://cli.github.com) and run `gh auth login`",
                    self.program
                )),
                other => other,
            })?;
        if output.status.success() {
            Ok(run::stdout(&output).trim().to_owned())
        } else {
            Err(run::failure(&what, &output))
        }
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
    async fn a_gh_that_exits_without_reading_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("gh");
        fs::write(
            &script,
            "#!/bin/sh\necho 'To get started with GitHub CLI, please run: gh auth login' >&2\nexit 4\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let gh = GhPullRequests::new(script.to_string_lossy());
        let draft = PullRequestDraft {
            base: "master".into(),
            head: "CU-1".into(),
            title: "CU-1 - x".into(),
            // Más de lo que cabe en el buffer del pipe: escribirlo daría "Broken pipe".
            body: "x".repeat(1 << 20),
            draft: false,
        };

        let error = gh.create(dir.path(), &draft).await.unwrap_err().to_string();

        assert!(error.contains("gh pr create failed"), "{error}");
        assert!(error.contains("gh auth login"), "{error}");
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
