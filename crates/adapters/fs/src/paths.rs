//! Nombres de archivo y carpeta a partir de datos del proveedor.

/// Un id de tarea como nombre de carpeta: todo lo que no sea letra, dígito, `-` o `_` se
/// cambia por `_`. Los ids de `ClickUp` no cambian; un id raro como `../x` no se sale de
/// la carpeta.
pub(crate) fn safe_name(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
