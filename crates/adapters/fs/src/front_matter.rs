//! Secciones de contexto (§14.2): Markdown con *front matter* TOML entre líneas `+++`,
//! como en Hugo o Zola.
//!
//! ```markdown
//! +++
//! title = "Modelo de negocio"
//! role = "knowledge"
//! usage = ["summary", "plan"]
//! priority = 10
//! +++
//!
//! ACME vende software de facturación...
//! ```

use domain::DomainError;
use domain::context::{ContextSection, SectionKey, SectionRole, Usage};
use enumset::EnumSet;
use serde::Deserialize;

/// Prioridad de una sección que no la declara (§14.2).
const DEFAULT_PRIORITY: u8 = 100;

/// El *front matter* tal cual. Todos los campos son opcionales.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SectionFile {
    title: Option<String>,
    role: Option<String>,
    usage: Option<Vec<String>>,
    priority: Option<u8>,
    #[serde(default)]
    private: bool,
}

/// Separa el *front matter* del cuerpo. Los dos resultados son **trozos de `text`**
/// (`&str` prestados), no copias.
///
/// | `text` | Resultado |
/// | --- | --- |
/// | Empieza con `+++` + salto de línea, y hay otra línea `+++` | `(Some(lo de en medio), cuerpo)` |
/// | No empieza con `+++` | `(None, text completo)` |
/// | Empieza con `+++` pero no hay otra línea `+++` | error `"front matter is not closed with +++"` |
///
/// El cuerpo se devuelve **sin espacios ni saltos de línea al principio y al final** (`trim`).
///
/// # Errors
///
/// El de la última fila de la tabla.
pub(crate) fn split(text: &str) -> Result<(Option<&str>, &str), String> {
    let Some(rest) = text.strip_prefix("+++") else {
        return Ok((None, text.trim()));
    };
    // Archivos con saltos de línea de Windows (`\r\n`).
    let rest = rest.trim_start_matches('\r');
    let Some(rest) = rest.strip_prefix('\n') else {
        return Ok((None, text.trim()));
    };
    let end = rest
        .find("\n+++")
        .ok_or_else(|| "front matter is not closed with +++".to_owned())?;
    let front = &rest[..end];
    let body = &rest[end + "\n+++".len()..];
    Ok((Some(front), body.trim()))
}

/// Una sección a partir del nombre de su archivo y su texto:
///
/// | Campo | Valor | Si falta en el *front matter* |
/// | --- | --- | --- |
/// | `key` | `SectionKey::from_file_name(file_name)` | — |
/// | `title` | `title` | La clave (`negocio`) |
/// | `role` | `role` con `SectionRole::from_str` | `Knowledge` |
/// | `usage` | Cada texto con `Usage::from_str`, reunidos en un `EnumSet` | Todos los usos (`EnumSet::all()`) |
/// | `priority` | `priority` | [`DEFAULT_PRIORITY`] |
/// | `private` | `private` | `false` |
/// | `body` | El cuerpo de [`split`] | — |
///
/// Sin *front matter*, todos los campos toman su valor por defecto.
///
/// # Errors
///
/// Un texto con el motivo: nombre de archivo inválido, *front matter* sin cerrar, TOML
/// inválido o con campos desconocidos, o un `role`/`usage` desconocido.
pub(crate) fn parse_section(file_name: &str, text: &str) -> Result<ContextSection, String> {
    let key = SectionKey::from_file_name(file_name).map_err(|e| e.to_string())?;
    let (front, body) = split(text)?;

    let file: SectionFile = match front {
        Some(raw) => toml::from_str(raw).map_err(|e| e.to_string())?,
        None => SectionFile::default(),
    };
    let role = match file.role {
        Some(raw) => raw.parse().map_err(|e: DomainError| e.to_string())?,
        None => SectionRole::Knowledge,
    };
    let usage = match file.usage {
        Some(list) => list
            .iter()
            .map(|raw| raw.parse::<Usage>())
            .collect::<Result<EnumSet<Usage>, _>>()
            .map_err(|e| e.to_string())?,
        None => EnumSet::all(),
    };

    Ok(ContextSection {
        title: file.title.unwrap_or_else(|| key.to_string()),
        key,
        role,
        usage,
        priority: file.priority.unwrap_or(DEFAULT_PRIORITY),
        private: file.private,
        body: body.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use enumset::enum_set;

    use super::*;

    const FULL: &str = "+++
title = \"Modelo de negocio\"
role = \"instruction\"
usage = [\"summary\", \"agent_prompt\"]
priority = 10
private = true
+++

ACME vende software.
- Pagan por factura.
";

    #[test]
    fn split_separates_front_matter_and_trims_the_body() {
        let (front, body) = split(FULL).unwrap();

        assert!(front.unwrap().contains("title = \"Modelo de negocio\""));
        assert_eq!(body, "ACME vende software.\n- Pagan por factura.");
    }

    #[test]
    fn split_without_front_matter_is_all_body() {
        assert_eq!(split("\nSolo texto.\n").unwrap(), (None, "Solo texto."));
    }

    #[test]
    fn split_borrows_from_the_text() {
        let (_, body) = split(FULL).unwrap();

        let start = FULL.as_ptr() as usize;
        let body_start = body.as_ptr() as usize;
        assert!(body_start > start && body_start < start + FULL.len());
    }

    #[test]
    fn unclosed_front_matter_is_an_error() {
        assert!(split("+++\ntitle = \"x\"\nsin cierre").is_err());
    }

    #[test]
    fn parses_every_field() {
        let section = parse_section("10-negocio.md", FULL).unwrap();

        assert_eq!(section.key.as_str(), "negocio");
        assert_eq!(section.title, "Modelo de negocio");
        assert_eq!(section.role, SectionRole::Instruction);
        assert_eq!(
            section.usage,
            enum_set!(Usage::Summary | Usage::AgentPrompt)
        );
        assert_eq!(section.priority, 10);
        assert!(section.private);
        assert!(section.body.starts_with("ACME"));
    }

    #[test]
    fn missing_fields_take_their_defaults() {
        let section = parse_section("codigo.md", "Rust y axum.").unwrap();

        assert_eq!(section.title, "codigo");
        assert_eq!(section.role, SectionRole::Knowledge);
        assert_eq!(section.usage, EnumSet::all());
        assert_eq!(section.priority, DEFAULT_PRIORITY);
        assert!(!section.private);
        assert_eq!(section.body, "Rust y axum.");
    }

    #[test]
    fn invalid_sections_explain_why() {
        let cases = [
            ("Negocio.md", "texto", "Negocio.md"),
            (
                "negocio.md",
                "+++\nusage = [\"resumen\"]\n+++\nx",
                "resumen",
            ),
            ("negocio.md", "+++\nrole = \"rol\"\n+++\nx", "rol"),
            ("negocio.md", "+++\ntitel = \"x\"\n+++\nx", "titel"),
        ];
        for (file, text, mentioned) in cases {
            let error = parse_section(file, text).unwrap_err();
            assert!(error.contains(mentioned), "`{mentioned}` not in `{error}`");
        }
    }
}
