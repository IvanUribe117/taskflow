//! Markdown → formato de comentario de `ClickUp`.
//!
//! `comment_text` se publica tal cual: los `##` y `**` se ven como texto. Para que se vea
//! con formato hay que mandar `comment`, una lista de tramos (verificado contra la API):
//!
//! - Formato de **texto** en el tramo: `{"text": "negrita", "attributes": {"bold": true}}`
//!   (`bold`, `italic`, `code`, `link`).
//! - Formato de **línea** en el `"\n"` que la termina: `{"text": "\n", "attributes":
//!   {"header": 2}}` (`header`, `list`, `code-block`, `blockquote`).
//!
//! Se convierte línea por línea: lo que entiende `ClickUp` y escribe Taskflow (títulos,
//! listas, casillas, bloques de código, citas, negrita, cursiva, código y enlaces). El
//! resto queda como texto, que es lo que pasaba antes: nunca se pierde nada.

use serde_json::{Map, Value, json};

/// Los tramos de `comment` para `markdown`.
pub(crate) fn to_comment(markdown: &str) -> Vec<Value> {
    let mut segments = Vec::new();
    let mut in_code = false;

    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            push_text(&mut segments, line, Map::new());
            segments.push(newline(json!({"code-block": {"code-block": "plain"}})));
            continue;
        }
        let (content, block) = block_format(line);
        inline(&mut segments, content);
        segments.push(newline(block));
    }
    segments
}

/// El formato de la línea y lo que queda después de su marca (`## `, `- `, `1. `...).
fn block_format(line: &str) -> (&str, Value) {
    let trimmed = line.trim_start();
    let indent = (line.len() - trimmed.len()) / 2;
    let with_indent = |mut format: Value| {
        if indent > 0 {
            format["indent"] = json!(indent);
        }
        format
    };

    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
        // `ClickUp` muestra tres niveles de título.
        return (
            trimmed[hashes + 1..].trim(),
            json!({"header": hashes.min(3)}),
        );
    }
    if matches!(trimmed, "---" | "***" | "___") {
        return ("", json!({}));
    }
    if let Some(rest) = trimmed.strip_prefix("> ") {
        return (rest, json!({"blockquote": true}));
    }
    for (prefix, list) in [
        ("- [ ] ", "unchecked"),
        ("- [x] ", "checked"),
        ("- [X] ", "checked"),
        ("[ ] ", "unchecked"),
        ("[x] ", "checked"),
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return (rest, with_indent(json!({"list": {"list": list}})));
        }
    }
    for prefix in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return (rest, with_indent(json!({"list": {"list": "bullet"}})));
        }
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits > 0
        && let Some(rest) = trimmed[digits..].strip_prefix(". ")
    {
        return (rest, with_indent(json!({"list": {"list": "ordered"}})));
    }
    (line, json!({}))
}

/// Negrita (`**x**`), cursiva (`*x*`), código (`` `x` ``) y enlaces (`[x](url)`). Una marca
/// sin cierre queda como texto.
fn inline(segments: &mut Vec<Value>, text: &str) {
    let mut plain = String::new();
    let mut rest = text;

    while let Some(c) = rest.chars().next() {
        let found = match c {
            '`' => delimited(rest, "`", "`")
                .map(|(inner, len)| (inner, attr("code", json!(true)), len)),
            '*' if rest.starts_with("**") => delimited(rest, "**", "**")
                .map(|(inner, len)| (inner, attr("bold", json!(true)), len)),
            '*' => delimited(rest, "*", "*")
                .map(|(inner, len)| (inner, attr("italic", json!(true)), len)),
            '[' => link(rest).map(|(label, url, len)| (label, attr("link", json!(url)), len)),
            _ => None,
        };
        match found {
            Some((inner, attributes, len)) if !inner.is_empty() => {
                push_text(segments, &std::mem::take(&mut plain), Map::new());
                push_text(segments, inner, attributes);
                rest = &rest[len..];
            }
            _ => {
                plain.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    push_text(segments, &plain, Map::new());
}

/// `**x**` al principio de `text` → `("x", largo total)`.
fn delimited<'a>(text: &'a str, open: &str, close: &str) -> Option<(&'a str, usize)> {
    let after = text.strip_prefix(open)?;
    let end = after.find(close)?;
    Some((&after[..end], open.len() + end + close.len()))
}

/// `[texto](url)` al principio de `text` → `("texto", "url", largo total)`.
fn link(text: &str) -> Option<(&str, &str, usize)> {
    let (label, label_len) = delimited(text, "[", "]")?;
    let (url, url_len) = delimited(&text[label_len..], "(", ")")?;
    Some((label, url, label_len + url_len))
}

fn attr(name: &str, value: Value) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert(name.to_owned(), value);
    map
}

fn push_text(segments: &mut Vec<Value>, text: &str, attributes: Map<String, Value>) {
    if !text.is_empty() {
        segments.push(segment(Value::from(text), Value::Object(attributes)));
    }
}

fn newline(attributes: Value) -> Value {
    segment(Value::from("\n"), attributes)
}

/// `{"text": ..., "attributes": ...}`, moviendo los valores (`json!` los copiaría).
fn segment(text: Value, attributes: Value) -> Value {
    let mut map = Map::new();
    map.insert("text".to_owned(), text);
    map.insert("attributes".to_owned(), attributes);
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segs(markdown: &str) -> Vec<String> {
        to_comment(markdown)
            .iter()
            .map(|s| format!("{}|{}", s["text"].as_str().unwrap(), s["attributes"]))
            .collect()
    }

    #[test]
    fn headers_lists_and_plain_lines() {
        assert_eq!(
            segs("## Título\ntexto\n- uno\n1. paso\n- [x] hecho\n\n#### Profundo"),
            [
                "Título|{}",
                "\n|{\"header\":2}",
                "texto|{}",
                "\n|{}",
                "uno|{}",
                "\n|{\"list\":{\"list\":\"bullet\"}}",
                "paso|{}",
                "\n|{\"list\":{\"list\":\"ordered\"}}",
                "hecho|{}",
                "\n|{\"list\":{\"list\":\"checked\"}}",
                "\n|{}",
                "Profundo|{}",
                "\n|{\"header\":3}",
            ]
        );
    }

    #[test]
    fn inline_bold_italic_code_and_links() {
        assert_eq!(
            segs("**Rama:** `CU-1` y *ver* [PR](https://x.test/1)"),
            [
                "Rama:|{\"bold\":true}",
                " |{}",
                "CU-1|{\"code\":true}",
                " y |{}",
                "ver|{\"italic\":true}",
                " |{}",
                "PR|{\"link\":\"https://x.test/1\"}",
                "\n|{}",
            ]
        );
    }

    #[test]
    fn code_blocks_keep_their_text_without_inline_parsing() {
        assert_eq!(
            segs("```rust\nlet x = **y**;\n```"),
            [
                "let x = **y**;|{}",
                "\n|{\"code-block\":{\"code-block\":\"plain\"}}",
            ]
        );
    }

    #[test]
    fn unclosed_marks_and_snake_case_stay_as_text() {
        assert_eq!(
            segs("2 * 3 y mi_variable_x y `sin cierre"),
            ["2 * 3 y mi_variable_x y `sin cierre|{}", "\n|{}"]
        );
    }

    #[test]
    fn nested_bullets_are_indented() {
        assert_eq!(
            segs("- padre\n  - hijo"),
            [
                "padre|{}",
                "\n|{\"list\":{\"list\":\"bullet\"}}",
                "hijo|{}",
                "\n|{\"indent\":1,\"list\":{\"list\":\"bullet\"}}",
            ]
        );
    }

    #[test]
    fn accents_do_not_break_the_scanner() {
        assert_eq!(
            segs("año **niño** ñ"),
            ["año |{}", "niño|{\"bold\":true}", " ñ|{}", "\n|{}"]
        );
    }
}
