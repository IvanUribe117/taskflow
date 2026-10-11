/// Nivel jerárquico genérico. `ClickUp`: Space > Folder > List. `GitHub`: Owner > Repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LocationLevel {
    Workspace,
    Space,
    Folder,
    List,
    Repository,
    Other(String),
}

impl LocationLevel {
    /// Mayor profundidad = más específico. Lo usa la resolución de proyectos.
    #[must_use]
    pub fn depth(&self) -> u8 {
        match self {
            Self::Workspace => 0,
            Self::Space | Self::Other(_) => 1,
            Self::Folder => 2,
            Self::List | Self::Repository => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationSegment {
    pub level: LocationLevel,
    pub id: String,
    pub name: String,
}

/// Ruta desde la raíz hasta el contenedor de la tarea.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskLocation(Vec<LocationSegment>);

impl TaskLocation {
    #[must_use]
    pub fn new(segments: Vec<LocationSegment>) -> Self {
        Self(segments)
    }

    #[must_use]
    pub fn segments(&self) -> &[LocationSegment] {
        &self.0
    }
}

/// Árbol de ubicaciones para el asistente de proyectos. `Vec` ya vive en el heap,
/// por eso el tipo recursivo no necesita `Box`.
#[derive(Debug, Clone)]
pub struct LocationNode {
    pub segment: LocationSegment,
    pub children: Vec<LocationNode>,
}
