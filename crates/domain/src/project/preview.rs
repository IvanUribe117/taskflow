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
    /// Dónde se ve: una plantilla como `http://localhost:{port}`.
    pub url: String,
    /// El primer puerto: cada tarea usa uno de `port..port + 100`.
    pub port: u16,
}

/// Los valores para completar las plantillas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewVars<'a> {
    /// El repo principal del proyecto.
    pub repo: &'a str,
    /// El worktree de la tarea.
    pub worktree: &'a str,
    /// La rama (`CU-86b1`).
    pub branch: &'a str,
    /// El id de la tarea: de él sale el puerto.
    pub task_id: &'a str,
}

/// La vista previa de una tarea, con los comandos completos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPreview {
    /// El comando para levantarla.
    pub start: String,
    /// El comando para detenerla, si hay.
    pub stop: Option<String>,
    /// Dónde se ve.
    pub url: String,
    /// El puerto de esta tarea.
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
        let port_text = port.to_string();
        let value = |key: &str| match key {
            "repo" => Some(vars.repo),
            "worktree" => Some(vars.worktree),
            "branch" => Some(vars.branch),
            "name" => Some(name.as_str()),
            "port" => Some(port_text.as_str()),
            _ => None,
        };
        let fill = |template: &str| fill_placeholders(template, value);
        RenderedPreview {
            start: fill(&self.start),
            stop: self.stop.as_deref().map(fill),
            url: fill(&self.url),
            port,
        }
    }
}

/// Cambia cada `{clave}` de `template` por `value(clave)`, en **una sola pasada**: lo que
/// se inserta no se vuelve a revisar (una ruta con `{name}` adentro queda tal cual). Una
/// clave desconocida, o una llave sin cerrar, queda como estaba.
fn fill_placeholders<'v>(template: &str, value: impl Fn(&str) -> Option<&'v str>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        if let Some((close, text)) = after
            .find('}')
            .and_then(|close| Some((close, value(&after[..close])?)))
        {
            out.push_str(text);
            rest = &after[close + 1..];
        } else {
            out.push('{');
            rest = after;
        }
    }
    out.push_str(rest);
    out
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

    #[test]
    fn placeholders_are_filled_once_and_unknown_ones_stay() {
        let preview = Preview {
            start: "run '{repo}' '{worktree}' {branch} {name} {otro} {".into(),
            stop: None,
            url: "http://localhost:{port}".into(),
            port: 8100,
        };
        let vars = PreviewVars {
            repo: "/code/idk",
            worktree: "/wt/{name}",
            branch: "CU-86b1",
            task_id: "86b1",
        };

        let rendered = preview.render(&vars);

        assert_eq!(
            rendered.start,
            "run '/code/idk' '/wt/{name}' CU-86b1 cu-86b1 {otro} {"
        );
        assert_eq!(rendered.url, format!("http://localhost:{}", rendered.port));
    }
}
