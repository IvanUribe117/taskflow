use std::path::Path;

/// Preguntas sobre carpetas del disco que necesita la sesión para ubicar el worktree: si
/// existen y si dos rutas son la misma. Así la aplicación no toca el sistema de archivos
/// directamente. Lo implementa `adapter-fs` (`FsFolders`).
pub trait Folders: Send + Sync {
    /// Si `path` existe y es una carpeta.
    fn exists(&self, path: &Path) -> bool;

    /// Si `a` y `b` son la misma carpeta, aunque una pase por un enlace
    /// (`/tmp` → `/private/tmp` en macOS).
    fn same(&self, a: &Path, b: &Path) -> bool;

    /// Si `path` está dentro de `dir` (o es `dir`), con la misma tolerancia a enlaces.
    fn is_inside(&self, path: &Path, dir: &Path) -> bool;
}
