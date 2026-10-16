//! La terminal: saber si se puede preguntar, confirmar, medir el ancho de los menús y
//! abrir el editor.
//!
//! Los comandos no hablan con `dialoguer` ni con `$EDITOR` para estas cosas: pasan por
//! aquí, para que todos se porten igual (por ejemplo, sin terminal ninguno se queda
//! esperando una respuesta que nunca llegará).

use std::fs::OpenOptions;
use std::io::{IsTerminal, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context as _;

use crate::presenter;

/// Si se puede preguntar: hay una terminal de verdad (no un script ni una tubería).
pub(crate) fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Antes de preguntar algo: sin terminal, un error que dice cómo evitar la pregunta
/// (`hint`, por ejemplo "usa --yes"), en vez del error de E/S de `dialoguer`.
///
/// # Errors
///
/// Si no hay terminal.
pub(crate) fn need_terminal(hint: &str) -> anyhow::Result<()> {
    if interactive() {
        Ok(())
    } else {
        anyhow::bail!("no hay una terminal para preguntar: {hint}")
    }
}

/// Confirmar una acción con `question` (`default` es la respuesta preseleccionada). Sin
/// terminal, un error que pide `--yes`: quien confirma sin mirar debe decirlo.
///
/// # Errors
///
/// Sin terminal, o si falla la entrada.
pub(crate) fn confirm(default: bool, question: &str) -> anyhow::Result<bool> {
    need_terminal("usa --yes para confirmar sin preguntar")?;
    Ok(
        dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
            .with_prompt(question)
            .default(default)
            .interact()?,
    )
}

/// Cuántos caracteres caben en una opción de un `Select`: el ancho de la terminal menos
/// el margen del cursor (`❯ `).
///
/// Por qué importa: una opción más larga ocupa dos líneas, y `dialoguer` solo borra una
/// por opción al redibujar el menú, así que la pantalla se llena de copias. Por eso las
/// opciones largas pasan por `presenter::truncate(texto, fit_width())`.
pub(crate) fn fit_width() -> usize {
    let (_, columns) = dialoguer::console::Term::stdout().size();
    usize::from(columns).saturating_sub(4).max(20)
}

/// Abre `path` en `$VISUAL`, `$EDITOR` o `vi`, y espera a que el editor termine.
///
/// Se ejecuta como `sh -c '<editor> "$1"' sh <path>`: así un editor con argumentos
/// (`code --wait`) funciona, y la ruta llega como argumento aparte, sin que el shell la
/// interprete (una ruta con espacios o comillas no rompe nada).
///
/// # Errors
///
/// No se pudo ejecutar el editor, o terminó con error.
pub(crate) fn edit_file(path: &Path) -> anyhow::Result<()> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_owned());
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(path)
        .status()
        .with_context(|| format!("no se pudo abrir el editor `{editor}`"))?;
    if !status.success() {
        anyhow::bail!("el editor terminó con {status}");
    }
    Ok(())
}

/// Edita `initial` en tu editor y devuelve el texto final, sin espacios al principio ni
/// al final. Sirve para textos que no viven en un archivo (el mensaje del commit, la
/// descripción del PR).
///
/// El texto pasa por un archivo temporal con extensión `extension` (`md`, `txt`), para
/// que el editor elija el resaltado. El nombre lleva el proceso y la hora en
/// nanosegundos, y se crea con `create_new`: nunca se escribe sobre un archivo ajeno. El
/// archivo se borra siempre al salir, también si el editor falla (ver [`TempFile`]).
///
/// # Errors
///
/// No se pudo escribir o leer el archivo temporal, o falla el editor.
pub(crate) fn edit_text(initial: &str, extension: &str) -> anyhow::Result<String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let path = std::env::temp_dir().join(format!(
        "taskflow-{}-{nanos}.{extension}",
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .with_context(|| format!("no se pudo crear {}", presenter::path(&path)))?;
    // Desde aquí el archivo es nuestro: el guardián lo borra pase lo que pase.
    let temp = TempFile(path);
    file.write_all(initial.as_bytes())
        .with_context(|| format!("no se pudo escribir {}", presenter::path(&temp.0)))?;
    drop(file);
    edit_file(&temp.0)?;
    let edited = std::fs::read_to_string(&temp.0)
        .with_context(|| format!("no se pudo leer {}", presenter::path(&temp.0)))?;
    Ok(edited.trim().to_owned())
}

/// Un archivo temporal que se borra al salir de su alcance (`Drop`), también con `?` o
/// un panic. Borrarlo es limpieza: si falla, no hay nada más que hacer.
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_files_are_removed_on_drop() {
        let path = std::env::temp_dir().join(format!("taskflow-test-{}.txt", std::process::id()));
        std::fs::write(&path, "x").unwrap();

        drop(TempFile(path.clone()));

        assert!(!path.exists());
    }

    #[test]
    fn menus_are_never_narrower_than_twenty() {
        assert!(fit_width() >= 20);
    }
}
