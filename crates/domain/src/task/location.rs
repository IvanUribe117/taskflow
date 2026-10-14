use std::fmt;
use std::str::FromStr;

use crate::DomainError;

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

/// En minúsculas, como se escribe en `project.toml`: `space`, `folder`, `list`...
/// `Other(name)` muestra `name` tal cual.
impl fmt::Display for LocationLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Workspace => f.write_str("workspace"),
            Self::Space => f.write_str("space"),
            Self::Folder => f.write_str("folder"),
            Self::List => f.write_str("list"),
            Self::Repository => f.write_str("repository"),
            Self::Other(name) => f.write_str(name),
        }
    }
}

/// El inverso de `Display` para los cinco niveles conocidos.
///
/// `Other` **no** se puede parsear: si se aceptara cualquier texto, un error de tipeo
/// (`"lsit"`) se convertiría en un nivel válido en silencio.
impl FromStr for LocationLevel {
    type Err = DomainError;

    /// # Errors
    ///
    /// [`DomainError::InvalidLevel`] si `s` no es uno de los cinco nombres conocidos.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "workspace" => Ok(Self::Workspace),
            "space" => Ok(Self::Space),
            "folder" => Ok(Self::Folder),
            "list" => Ok(Self::List),
            "repository" => Ok(Self::Repository),
            other => Err(DomainError::InvalidLevel(other.to_owned())),
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationNode {
    pub segment: LocationSegment,
    pub children: Vec<LocationNode>,
}

impl LocationNode {
    /// Un nodo sin hijos.
    #[must_use]
    pub fn leaf(level: LocationLevel, id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            segment: LocationSegment {
                level,
                id: id.into(),
                name: name.into(),
            },
            children: Vec::new(),
        }
    }
}

/// Un nodo del árbol visto como una fila de una lista: su profundidad y su segmento.
///
/// Toma prestado el segmento del árbol (`&'a`): aplanar no copia nada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatNode<'a> {
    /// 0 para las raíces (espacios), 1 para sus hijos, etc.
    pub depth: usize,
    pub segment: &'a LocationSegment,
}

/// El árbol como lista, en **preorden**: cada nodo y, justo debajo, sus hijos.
///
/// ```text
/// Clientes            depth 0
///   Backend           depth 1
///     Sprint          depth 2
///   Soporte           depth 1
/// Interno             depth 0
/// ```
///
/// Así el asistente puede mostrar el árbol en un selector, que solo sabe de listas.
#[must_use]
pub fn flatten(nodes: &[LocationNode]) -> Vec<FlatNode<'_>> {
    /// Agrega a `out` cada nodo de `nodes` y, justo debajo, sus hijos (recursivamente).
    fn walk<'a>(nodes: &'a [LocationNode], depth: usize, out: &mut Vec<FlatNode<'a>>) {
        for node in nodes {
            out.push(FlatNode {
                depth,
                segment: &node.segment,
            });
            walk(&node.children, depth + 1, out);
        }
    }

    let mut out = Vec::new();
    walk(nodes, 0, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_display_in_lowercase() {
        assert_eq!(LocationLevel::Space.to_string(), "space");
        assert_eq!(LocationLevel::Folder.to_string(), "folder");
        assert_eq!(LocationLevel::List.to_string(), "list");
        assert_eq!(LocationLevel::Other("board".into()).to_string(), "board");
    }

    #[test]
    fn known_levels_round_trip() {
        for level in [
            LocationLevel::Workspace,
            LocationLevel::Space,
            LocationLevel::Folder,
            LocationLevel::List,
            LocationLevel::Repository,
        ] {
            assert_eq!(level.to_string().parse::<LocationLevel>(), Ok(level));
        }
    }

    #[test]
    fn unknown_levels_are_rejected() {
        for raw in ["", "lsit", "List", "board"] {
            assert_eq!(
                raw.parse::<LocationLevel>(),
                Err(DomainError::InvalidLevel(raw.to_owned())),
                "`{raw}` should be rejected"
            );
        }
    }

    fn tree() -> Vec<LocationNode> {
        let mut clients = LocationNode::leaf(LocationLevel::Space, "S1", "Clientes");
        let mut backend = LocationNode::leaf(LocationLevel::Folder, "F1", "Backend");
        backend
            .children
            .push(LocationNode::leaf(LocationLevel::List, "L1", "Sprint"));
        clients.children.push(backend);
        clients
            .children
            .push(LocationNode::leaf(LocationLevel::List, "L2", "Soporte"));
        vec![
            clients,
            LocationNode::leaf(LocationLevel::Space, "S2", "Interno"),
        ]
    }

    #[test]
    fn flatten_lists_every_node_in_preorder() {
        let tree = tree();

        let rows: Vec<(usize, &str)> = flatten(&tree)
            .iter()
            .map(|node| (node.depth, node.segment.name.as_str()))
            .collect();

        assert_eq!(
            rows,
            [
                (0, "Clientes"),
                (1, "Backend"),
                (2, "Sprint"),
                (1, "Soporte"),
                (0, "Interno"),
            ]
        );
    }

    #[test]
    fn flatten_borrows_the_segments() {
        let tree = tree();

        let flat = flatten(&tree);

        // Misma dirección de memoria: es una referencia al segmento del árbol, no una copia.
        assert!(std::ptr::eq(
            flat[0].segment,
            std::ptr::from_ref(&tree[0].segment)
        ));
    }

    #[test]
    fn empty_tree_is_empty() {
        assert!(flatten(&[]).is_empty());
    }
}
