//! Cambios sobre el documento TOML conservando comentarios, orden y formato (FR-21).
//!
//! `toml` convierte texto ↔ structs, pero al volver a escribir pierde los comentarios.
//! `toml_edit` trabaja sobre el documento tal cual está escrito: se usa para editar, y
//! `toml` para validar el resultado.

use application::settings::{ConfigError, ConfigKey};
use toml_edit::{DocumentMut, Item, Table, Value};

/// El elemento de `key` dentro del documento, o `None` si no aparece.
///
/// El resultado toma prestado `doc`. No hace falta escribir lifetimes: con una sola
/// referencia de entrada, Rust asume que la salida vive lo mismo que ella (*elision*).
/// Escrito a mano sería `fn lookup<'a>(doc: &'a DocumentMut, ...) -> Option<&'a Item>`.
pub(crate) fn lookup(doc: &DocumentMut, key: ConfigKey) -> Option<&Item> {
    let mut item = doc.as_item();
    for segment in key.segments() {
        item = item.get(segment)?;
    }
    Some(item)
}

/// El valor de `key` como texto TOML (`9001`, `"9001"`, `["qa"]`), sin espacios ni
/// comentarios alrededor. `None` si no aparece o si no es un valor (es una tabla).
pub(crate) fn get(doc: &DocumentMut, key: ConfigKey) -> Option<String> {
    let mut value = lookup(doc, key)?.as_value()?.clone();
    value.decor_mut().clear();
    Some(value.to_string())
}

/// Interpreta lo que escribió el usuario en `config set`.
///
/// - Si el valor actual (`current`) es un texto, `raw` se guarda como texto sin
///   interpretarlo: `config set provider.clickup.team_id 9001` no lo convierte en número.
/// - Si no, `raw` se interpreta como un valor TOML (`10`, `true`, `["a", "b"]`).
/// - Si no es TOML válido (`project`, `in progress`), se guarda como texto.
pub(crate) fn parse_value(raw: &str, current: Option<&Item>) -> Value {
    if current.and_then(Item::as_str).is_some() {
        return Value::from(raw);
    }
    raw.parse().unwrap_or_else(|_| Value::from(raw))
}

/// Escribe `value` en `key`, creando las tablas intermedias que falten (`[ui]`).
///
/// Si la clave ya existía, se reemplaza **solo el valor** y se le copia la decoración del
/// anterior. Así no se pierde ningún comentario:
///
/// ```text
/// # Peso de la prioridad          ← decoración de la CLAVE: se conserva si no tocas la clave
/// priority_weight = 10  # mío     ← "  # mío" es decoración del VALOR: hay que copiarla
/// ```
///
/// Ojo: `table.insert(last, ...)` sobre una clave existente crea una clave nueva y pierde
/// el comentario de arriba. Usa `table.get_mut(last)` para cambiar el valor en su lugar.
///
/// # Errors
///
/// [`ConfigError::Syntax`] si un tramo intermedio de la clave existe pero no es una tabla
/// (por ejemplo `provider = "clickup"` y se intenta escribir `provider.kind`).
pub(crate) fn set(doc: &mut DocumentMut, key: ConfigKey, value: Value) -> Result<(), ConfigError> {
    let mut segments: Vec<&str> = key.segments().collect();
    let last = segments.pop().expect("a key has at least one segment");

    let mut table = doc.as_table_mut();
    for segment in segments {
        table = table
            .entry(segment)
            .or_insert_with(|| Item::Table(Table::new()))
            .as_table_mut()
            .ok_or_else(|| ConfigError::Syntax(format!("`{segment}` is not a table")))?;
    }

    match table.get_mut(last).and_then(Item::as_value_mut) {
        Some(existing) => {
            let decor = existing.decor().clone();
            *existing = value;
            *existing.decor_mut() = decor;
        }
        None => {
            table.insert(last, Item::Value(value));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> DocumentMut {
        text.parse().unwrap()
    }

    fn key(raw: &str) -> ConfigKey {
        raw.parse().unwrap()
    }

    const SAMPLE: &str = r#"# Taskflow
version = 1

[provider]
kind = "clickup"  # proveedor de tareas

[provider.clickup]
team_id = "9001"

[policy]
# Peso de la prioridad
priority_weight = 10  # mi comentario
"#;

    #[test]
    fn get_returns_values_as_toml_text() {
        let doc = doc(SAMPLE);

        assert_eq!(
            get(&doc, key("provider.kind")).as_deref(),
            Some(r#""clickup""#)
        );
        assert_eq!(
            get(&doc, key("policy.priority_weight")).as_deref(),
            Some("10")
        );
        assert_eq!(get(&doc, key("ui.group_by")), None);
    }

    #[test]
    fn strings_stay_strings() {
        let doc = doc(SAMPLE);
        let current = lookup(&doc, key("provider.clickup.team_id"));

        assert_eq!(parse_value("0042", current).as_str(), Some("0042"));
    }

    #[test]
    fn other_values_are_parsed_as_toml() {
        assert_eq!(parse_value("25", None).as_integer(), Some(25));
        assert_eq!(parse_value("true", None).as_bool(), Some(true));
        let list = parse_value(r#"["qa", "review"]"#, None);
        assert_eq!(list.as_array().map(toml_edit::Array::len), Some(2));
    }

    #[test]
    fn invalid_toml_becomes_text() {
        assert_eq!(parse_value("project", None).as_str(), Some("project"));
        assert_eq!(
            parse_value("in progress", None).as_str(),
            Some("in progress")
        );
    }

    #[test]
    fn set_keeps_comments_and_the_rest_of_the_file() {
        let mut doc = doc(SAMPLE);

        set(&mut doc, key("policy.priority_weight"), Value::from(25)).unwrap();

        let text = doc.to_string();
        assert!(
            text.contains("priority_weight = 25  # mi comentario"),
            "{text}"
        );
        assert!(text.contains("# Peso de la prioridad"), "{text}");
        assert!(
            text.contains(r#"kind = "clickup"  # proveedor de tareas"#),
            "{text}"
        );
    }

    #[test]
    fn set_creates_missing_tables() {
        let mut doc = doc(SAMPLE);

        set(&mut doc, key("ui.group_by"), Value::from("project")).unwrap();

        assert_eq!(
            get(&doc, key("ui.group_by")).as_deref(),
            Some(r#""project""#)
        );
        assert!(doc.to_string().contains("[ui]"));
    }

    #[test]
    fn set_fails_when_a_parent_is_not_a_table() {
        let mut doc = doc("provider = \"clickup\"\n");

        let result = set(&mut doc, key("provider.kind"), Value::from("clickup"));

        assert!(matches!(result, Err(ConfigError::Syntax(_))));
    }
}
