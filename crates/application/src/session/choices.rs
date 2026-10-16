//! Las decisiones del flujo de la sesión que la interfaz solo muestra: qué opciones hay al
//! salir del agente y qué subir después de un commit. Son funciones puras: cualquier
//! interfaz (la terminal, una app) ofrece lo mismo, en el mismo orden.

use crate::dto::BranchStatus;
use crate::settings::AfterCommit;

/// Una opción del menú al salir del agente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AfterAgentChoice {
    /// Que el agente revise sus cambios (`review_request`).
    Review,
    /// Hacer commit de `files` archivos con cambios.
    Commit {
        /// Cuántos archivos tienen cambios sin commit.
        files: usize,
    },
    /// Subir la rama y abrir el PR: todo está commiteado y la rama aún no tiene PR.
    PullRequest {
        /// La rama de trabajo (`CU-86b1`).
        branch: String,
        /// Cuántos commits tiene respecto de la base.
        commits: usize,
    },
    /// Publicar el reporte de lo que se hizo y terminar la sesión.
    PublishAndFinish,
    /// Terminar la sesión sin publicar.
    Finish,
    /// Volver a abrir el agente, retomando su conversación.
    BackToAgent,
    /// Salir dejando la sesión abierta, para seguir después.
    Leave,
}

/// Las opciones al salir del agente, en orden:
///
/// 1. "Revisar los cambios", si hay algo que revisar (archivos sin commit o una rama con
///    commits) y todavía no se revisó.
/// 2. "Hacer commit" si hay archivos con cambios; si no, "Crear el PR" si la rama tiene
///    commits y no tiene PR.
/// 3. "Revisar los cambios" otra vez, si ya se revisó: detrás del commit, para no repetir
///    la revisión por inercia.
/// 4. Publicar y terminar, terminar, volver al agente, dejar la sesión abierta.
///
/// `branch` es el estado de la rama de trabajo, si se pudo leer (no lo hay en la rama
/// base o sin commits).
#[must_use]
pub fn after_agent_choices(
    pending_files: usize,
    branch: Option<&BranchStatus>,
    reviewed: bool,
) -> Vec<AfterAgentChoice> {
    use AfterAgentChoice as C;
    let mut choices = Vec::new();
    let can_review = pending_files > 0 || branch.is_some();
    if can_review && !reviewed {
        choices.push(C::Review);
    }
    if pending_files > 0 {
        choices.push(C::Commit {
            files: pending_files,
        });
    } else if let Some(status) = branch.filter(|status| status.pr.is_none()) {
        choices.push(C::PullRequest {
            branch: status.branch.clone(),
            commits: status.commits.len(),
        });
    }
    if can_review && reviewed {
        choices.push(C::Review);
    }
    choices.extend([C::PublishAndFinish, C::Finish, C::BackToAgent, C::Leave]);
    choices
}

/// Qué subir después de un commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upload {
    /// `git push` y abrir el PR (con la descripción que propone la IA).
    PushAndPr,
    /// Solo `git push` (actualiza el PR si ya hay uno).
    Push,
    /// No subir nada por ahora.
    Nothing,
}

/// Lo que dice `session.after_commit` para este commit: hacer algo ya, o preguntar entre
/// estas opciones (la primera es la sugerida).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadDecision {
    /// Hacerlo sin preguntar.
    Do(Upload),
    /// Preguntar entre estas opciones; la primera es la sugerida.
    Ask(Vec<Upload>),
}

/// Según el modo, si la rama ya tiene PR y si se puede preguntar (`can_ask`: hay terminal
/// y no se pasó `--yes`):
///
/// | modo   | con PR        | sin PR                              |
/// | ------ | ------------- | ----------------------------------- |
/// | `none` | nada          | nada                                |
/// | `push` | push          | push                                |
/// | `pr`   | push          | push y PR                           |
/// | `ask`  | push o nada   | push y PR, solo push o nada         |
///
/// `ask` sin poder preguntar no sube nada: subir es hacia afuera y lo decides tú.
///
/// ```
/// use application::session::{Upload, UploadDecision, upload_after_commit};
/// use application::settings::AfterCommit;
///
/// assert_eq!(upload_after_commit(AfterCommit::Pr, false, false), UploadDecision::Do(Upload::PushAndPr));
/// assert_eq!(upload_after_commit(AfterCommit::Ask, false, false), UploadDecision::Do(Upload::Nothing));
/// ```
#[must_use]
pub fn upload_after_commit(mode: AfterCommit, has_pr: bool, can_ask: bool) -> UploadDecision {
    use UploadDecision::{Ask, Do};
    match mode {
        AfterCommit::None => Do(Upload::Nothing),
        AfterCommit::Push => Do(Upload::Push),
        AfterCommit::Pr if has_pr => Do(Upload::Push),
        AfterCommit::Pr => Do(Upload::PushAndPr),
        AfterCommit::Ask if !can_ask => Do(Upload::Nothing),
        AfterCommit::Ask if has_pr => Ask(vec![Upload::Push, Upload::Nothing]),
        AfterCommit::Ask => Ask(vec![Upload::PushAndPr, Upload::Push, Upload::Nothing]),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::ports::PullRequest;

    fn branch(pr: bool) -> BranchStatus {
        BranchStatus {
            repo: PathBuf::from("/code/idk"),
            branch: "CU-1".into(),
            base: "master".into(),
            commits: vec!["abc CU-1 - feat: x".into()],
            pr: pr.then(|| PullRequest {
                number: 7,
                url: "https://x/pull/7".into(),
            }),
            uncommitted: false,
        }
    }

    #[test]
    fn review_comes_first_until_it_is_done() {
        use AfterAgentChoice as C;
        let tail = [C::PublishAndFinish, C::Finish, C::BackToAgent, C::Leave];

        let before = after_agent_choices(2, None, false);
        let after = after_agent_choices(2, None, true);

        assert_eq!(before[..2], [C::Review, C::Commit { files: 2 }]);
        assert_eq!(after[..2], [C::Commit { files: 2 }, C::Review]);
        assert_eq!(before[2..], tail);
    }

    #[test]
    fn with_everything_committed_the_pr_is_offered_only_without_one() {
        use AfterAgentChoice as C;

        let without = after_agent_choices(0, Some(&branch(false)), true);
        let with = after_agent_choices(0, Some(&branch(true)), true);
        let nothing = after_agent_choices(0, None, false);

        assert_eq!(
            without[0],
            C::PullRequest {
                branch: "CU-1".into(),
                commits: 1
            }
        );
        assert_eq!(with[0], C::Review, "no PR option: it already has one");
        assert_eq!(
            nothing[0],
            C::PublishAndFinish,
            "nothing to review or upload"
        );
    }

    #[test]
    fn upload_follows_the_mode() {
        use Upload::{Nothing, Push, PushAndPr};
        use UploadDecision::{Ask, Do};

        assert_eq!(
            upload_after_commit(AfterCommit::None, false, true),
            Do(Nothing)
        );
        assert_eq!(
            upload_after_commit(AfterCommit::Push, false, true),
            Do(Push)
        );
        assert_eq!(upload_after_commit(AfterCommit::Pr, true, true), Do(Push));
        assert_eq!(
            upload_after_commit(AfterCommit::Ask, false, true),
            Ask(vec![PushAndPr, Push, Nothing])
        );
        assert_eq!(
            upload_after_commit(AfterCommit::Ask, true, true),
            Ask(vec![Push, Nothing])
        );
    }
}
