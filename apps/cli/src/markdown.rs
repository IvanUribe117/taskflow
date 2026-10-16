//! Markdown → texto con estilo para la terminal (reportes, planes). Línea por línea, como
//! el conversor de `ClickUp`: títulos en negrita y color, viñetas `•`, `código` resaltado,
//! **negrita**, *cursiva* y enlaces. Sin colores (por ejemplo, con la salida a un archivo),
//! queda texto limpio, sin `#` ni `**`.

use dialoguer::console::{Style, colors_enabled};

/// `markdown` listo para imprimir. Los colores siguen a la terminal (`colors_enabled`).
#[must_use]
pub(crate) fn render(markdown: &str) -> String {
    render_with(markdown, colors_enabled())
}

/// Igual, eligiendo si hay colores. Función pura: así se prueba.
#[must_use]
pub(crate) fn render_with(markdown: &str, colors: bool) -> String {
    let paint = |text: &str, style: Style| {
        if colors {
            // `force_styling`: si se pidieron colores, se ponen aunque `console` crea que
            // la salida no es una terminal.
            style.force_styling(true).apply_to(text).to_string()
        } else {
            text.to_owned()
        }
    };
    let mut out = Vec::new();
    let mut in_code = false;

    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            out.push(paint(&format!("    {line}"), Style::new().dim()));
            continue;
        }
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            let title = inline(trimmed[hashes + 1..].trim(), colors);
            let style = if hashes <= 2 {
                Style::new().bold().cyan().underlined()
            } else {
                Style::new().bold().cyan()
            };
            out.push(paint(&strip_marks(&title, colors), style));
        } else if matches!(trimmed, "---" | "***" | "___") {
            out.push(paint(&"─".repeat(40), Style::new().dim()));
        } else if let Some(rest) = trimmed.strip_prefix("> ") {
            out.push(paint(
                &format!("{indent}│ {}", inline(rest, colors)),
                Style::new().dim(),
            ));
        } else if let Some(rest) = ["- ", "* ", "+ "]
            .iter()
            .find_map(|prefix| trimmed.strip_prefix(prefix))
        {
            out.push(format!("{indent}  • {}", inline(rest, colors)));
        } else {
            out.push(format!("{indent}{}", inline(trimmed, colors)));
        }
    }
    out.join("\n")
}

/// Dentro de un título ya en negrita, los estilos internos sobran.
fn strip_marks(text: &str, colors: bool) -> String {
    if colors {
        dialoguer::console::strip_ansi_codes(text).into_owned()
    } else {
        text.to_owned()
    }
}

/// `**negrita**`, `*cursiva*`, `` `código` `` y `[texto](url)`. Una marca sin cierre queda
/// como texto.
fn inline(text: &str, colors: bool) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let found = match c {
            '`' => delimited(rest, "`", "`")
                .map(|(inner, len)| (styled(inner, &Style::new().yellow(), colors), len)),
            '*' if rest.starts_with("**") => delimited(rest, "**", "**")
                .map(|(inner, len)| (styled(inner, &Style::new().bold(), colors), len)),
            '*' => delimited(rest, "*", "*")
                .map(|(inner, len)| (styled(inner, &Style::new().italic(), colors), len)),
            '[' => link(rest).map(|(label, url, len)| {
                (
                    format!(
                        "{} ({url})",
                        styled(label, &Style::new().underlined(), colors)
                    ),
                    len,
                )
            }),
            _ => None,
        };
        match found {
            Some((piece, len)) if len > 2 => {
                out.push_str(&piece);
                rest = &rest[len..];
            }
            _ => {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    out
}

fn styled(text: &str, style: &Style, colors: bool) -> String {
    if colors {
        style.clone().force_styling(true).apply_to(text).to_string()
    } else {
        text.to_owned()
    }
}

fn delimited<'a>(text: &'a str, open: &str, close: &str) -> Option<(&'a str, usize)> {
    let after = text.strip_prefix(open)?;
    let end = after.find(close)?;
    (end > 0).then(|| (&after[..end], open.len() + end + close.len()))
}

fn link(text: &str) -> Option<(&str, &str, usize)> {
    let (label, label_len) = delimited(text, "[", "]")?;
    let (url, url_len) = delimited(&text[label_len..], "(", ")")?;
    Some((label, url, label_len + url_len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_colors_the_marks_disappear() {
        let text = "## Plan\n- usa `cargo test` y **revisa**\n  - anidado\n1. paso\n> nota\n---\n[PR](https://x.test)";

        assert_eq!(
            render_with(text, false),
            "Plan\n  • usa cargo test y revisa\n    • anidado\n1. paso\n│ nota\n────────────────────────────────────────\nPR (https://x.test)"
        );
    }

    #[test]
    fn code_blocks_are_indented_and_not_parsed() {
        assert_eq!(
            render_with("```\nlet **x** = 1;\n```", false),
            "    let **x** = 1;"
        );
    }

    #[test]
    fn with_colors_there_are_ansi_codes_but_the_same_text() {
        let colored = render_with("### Título\ntexto `x`", true);

        assert!(colored.contains("\u{1b}["), "{colored:?}");
        assert_eq!(
            dialoguer::console::strip_ansi_codes(&colored),
            "Título\ntexto x"
        );
    }

    #[test]
    fn snake_case_and_unclosed_marks_stay() {
        assert_eq!(
            render_with("mi_var * 2 y `abierto", false),
            "mi_var * 2 y `abierto"
        );
    }
}
