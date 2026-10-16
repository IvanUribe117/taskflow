//! `FsFolders`: el puerto `Folders` con el sistema de archivos real.

use std::path::{Path, PathBuf};

use application::ports::Folders;

/// Carpetas del disco, resolviendo enlaces con `canonicalize`.
#[derive(Debug, Clone, Copy, Default)]
pub struct FsFolders;

/// La ruta real (sin enlaces); si no existe, tal cual.
fn real(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

impl Folders for FsFolders {
    fn exists(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn same(&self, a: &Path, b: &Path) -> bool {
        real(a) == real(b)
    }

    fn is_inside(&self, path: &Path, dir: &Path) -> bool {
        path.starts_with(dir) || real(path).starts_with(real(dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_is_the_same_folder() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("link");
        std::fs::create_dir_all(dir.path().join("real/x")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("real"), &link).unwrap();

        assert!(FsFolders.exists(&link));
        assert!(FsFolders.same(&link, &dir.path().join("real")));
        assert!(FsFolders.is_inside(&link.join("x"), &dir.path().join("real")));
        assert!(!FsFolders.exists(&dir.path().join("nope")));
    }
}
