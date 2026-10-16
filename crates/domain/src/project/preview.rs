//! `[preview]` de `project.toml`: cómo levantar la app de un worktree para probarla en el
//! navegador. Son plantillas de comandos; aquí solo se completan (sin I/O).

/// ```toml
/// [preview]
/// start = "bash ~/previews/idk.sh start {repo} {worktree} {name} {port}"
/// stop = "docker rm -f idk-preview-{name}"
/// url = "http://localhost:{port}"
/// port = 8100
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// Levanta la app del worktree (si ya corre, no debería hacer nada).
    pub start: String,
    /// La detiene. Taskflow la corre al terminar la sesión.
    pub stop: Option<String>,
    pub url: String,
    /// El primer puerto: cada tarea usa uno de `port..port + 100`.
    pub port: u16,
}

/// Los valores para completar las plantillas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewVars<'a> {
    /// El repo principal del proyecto.
    pub repo: &'a str,
    /// El worktree de la tarea.
    pub worktree: &'a str,
    /// La rama (`CU-86b1`).
    pub branch: &'a str,
    pub task_id: &'a str,
}

/// La vista previa de una tarea, con los comandos completos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPreview {
    pub start: String,
    pub stop: Option<String>,
    pub url: String,
    pub port: u16,
}

/// Cuántos puertos se reparten a partir de `Preview::port`.
const PORT_RANGE: u16 = 100;

impl Preview {
    /// El puerto de la tarea: siempre el mismo para el mismo id (así no hace falta
    /// guardarlo), dentro de `port..port + 100`.
    #[must_use]
    pub fn port_for(&self, task_id: &str) -> u16 {
        let sum = task_id.bytes().fold(0u32, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(u32::from(b))
        });
        // `sum % 100` siempre cabe en un `u16`.
        let offset = u16::try_from(sum % u32::from(PORT_RANGE)).unwrap_or(0);
        self.port.saturating_add(offset)
    }

    /// Completa `{repo}`, `{worktree}`, `{branch}`, `{name}` (la rama en minúsculas, válida
    /// como nombre de contenedor) y `{port}` en los tres textos. Las rutas van tal cual: si
    /// pueden tener espacios, la plantilla debe ponerlas entre comillas.
    ///
    /// ```
    /// use domain::project::{Preview, PreviewVars};
    ///
    /// let preview = Preview {
    ///     start: "run {worktree} {name} {port}".into(),
    ///     stop: Some("rm idk-{name}".into()),
    ///     url: "http://localhost:{port}".into(),
    ///     port: 8100,
    /// };
    /// let vars = PreviewVars { repo: "/r", worktree: "/w", branch: "CU-86B1", task_id: "86B1" };
    ///
    /// let rendered = preview.render(&vars);
    ///
    /// let port = preview.port_for("86B1");
    /// assert_eq!(rendered.start, format!("run /w cu-86b1 {port}"));
    /// assert_eq!(rendered.stop.as_deref(), Some("rm idk-cu-86b1"));
    /// assert_eq!(rendered.url, format!("http://localhost:{port}"));
    /// ```
    #[must_use]
    pub fn render(&self, vars: &PreviewVars<'_>) -> RenderedPreview {
        let port = self.port_for(vars.task_id);
        let name: String = vars
            .branch
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let fill = |template: &str| {
            template
                .replace("{repo}", vars.repo)
                .replace("{worktree}", vars.worktree)
                .replace("{branch}", vars.branch)
                .replace("{name}", &name)
                .replace("{port}", &port.to_string())
        };
        RenderedPreview {
            start: fill(&self.start),
            stop: self.stop.as_deref().map(fill),
            url: fill(&self.url),
            port,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_task_keeps_its_port_inside_the_range() {
        let preview = Preview {
            start: String::new(),
            stop: None,
            url: String::new(),
            port: 8100,
        };

        let a = preview.port_for("86bcjdn6v");

        assert_eq!(a, preview.port_for("86bcjdn6v"), "stable");
        assert!((8100..8200).contains(&a));
        assert_ne!(a, preview.port_for("86baqt7cp"));
    }
}
