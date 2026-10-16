//! Escritura atómica (NFR-14).

use std::io::{self, Write};
use std::path::Path;

use tempfile::NamedTempFile;

/// Escribe `contents` en `path` de forma que el archivo siempre quede **completo**: con el
/// contenido anterior o con el nuevo, nunca a medias (por un corte de luz, un error de
/// disco o un Ctrl+C a mitad de la escritura).
///
/// 0. Si `path` es un enlace simbólico (un `config.toml` de tus dotfiles), se escribe en
///    el archivo al que apunta: renombrar sobre el enlace lo reemplazaría por un archivo.
/// 1. Crear el directorio padre si no existe.
/// 2. Crear un archivo temporal **en el mismo directorio** (`NamedTempFile::new_in`).
/// 3. Escribir todo el contenido y llamar a `sync_all` (que llegue al disco, no solo al
///    buffer del sistema operativo).
/// 4. `persist(path)`: renombra el temporal sobre el destino. Renombrar dentro de un mismo
///    sistema de archivos es atómico.
///
/// # Errors
///
/// Cualquier error de E/S. Si falla antes del paso 4, el temporal se borra solo (`Drop`).
///
/// El archivo nuevo queda con los permisos del temporal (`0600`, solo tú): la
/// configuración y las sesiones no son para otros usuarios de la máquina.
pub(crate) fn write(path: &Path, contents: &str) -> io::Result<()> {
    let target = if path.is_symlink() {
        std::fs::canonicalize(path)?
    } else {
        path.to_path_buf()
    };
    let path = target.as_path();
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;

    let mut file = NamedTempFile::new_in(dir)?;
    file.write_all(contents.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn creates_the_file_and_its_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deeper/config.toml");

        write(&path, "version = 1\n").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "version = 1\n");
    }

    #[test]
    fn replaces_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "old content that is longer than the new one").unwrap();

        write(&path, "new").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn leaves_no_temporary_files_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        write(&path, "a").unwrap();
        write(&path, "b").unwrap();

        let files: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn a_symlink_keeps_pointing_to_the_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("dotfiles/config.toml");
        fs::create_dir_all(real.parent().unwrap()).unwrap();
        fs::write(&real, "old").unwrap();
        let link = dir.path().join("config.toml");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        write(&link, "new").unwrap();

        assert!(link.is_symlink(), "still a link");
        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
    }
}
