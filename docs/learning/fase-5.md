# Fase 5: proyectos y agrupación

Objetivo del roadmap (§24): *"`today --by-project`; `project add` elige listas de ClickUp"*.

Al terminar podrás organizar tus tareas por cliente o proyecto:

```sh
taskflow project add            # asistente: eliges espacios, carpetas o listas de tu ClickUp
taskflow project list           # tus proyectos y sus reglas
taskflow project show acme      # detalle de un proyecto
taskflow today --by-project     # tareas agrupadas: "ACME Corp (cliente)", "Interno", "Sin proyecto"
taskflow today --project acme   # solo las de ACME
```

Gran parte del trabajo ya existe: en la fase 1 escribiste `ProjectResolver` (qué proyecto le corresponde a cada tarea) y en la fase 2 `GetTodayTasks` ya sabía agrupar y filtrar por proyecto. Faltaba **de dónde salen los proyectos**: ahora se leen de archivos `project.toml` y se crean eligiendo ubicaciones reales de tu ClickUp.

Hay **55 tests nuevos**; ahora mismo fallan 51. Además, **3 tests que ya pasaban** fallan hasta que completes su paso, porque el código que prueban ahora llama a funciones nuevas:

| Test                                                                  | Se arregla en              |
| --------------------------------------------------------------------- | -------------------------- |
| `build::tests::builds_the_context_from_settings`                    | Paso 11 (`projects_dir`) |
| `presenter::tests::today_lists_every_group_with_continuous_numbers` | Paso 13 (`group_title`)  |
| `presenter::tests::today_cuts_long_titles_but_show_does_not`        | Paso 13 (`group_title`)  |

Los comprobé todos con una implementación de referencia que después borré. La sección **"Especificación por paso"** dice exactamente qué hacer en cada paso: condiciones, orden y errores.

---

## Mapa de cambios

```text
domain           LocationLevel: Display + FromStr · ProjectKind: FromStr · MatchRule: Display
                 flatten(árbol) → Vec<FlatNode>
application      TaskProvider::locations · ProjectRepository::get / create
                 [projects] en la configuración · ProjectDetail · 4 casos de uso nuevos
adapter-clickup  locations(): espacios → carpetas → listas
adapter-fs       project.toml ↔ Project · FsProjectRepository
bootstrap        projects_dir: dónde están los proyectos
cli              today --by-project --project · project list | show | add
```

| Archivo                                     | Qué hay                                                                  |
| ------------------------------------------- | ------------------------------------------------------------------------- |
| `crates/domain/src/task/location.rs`      | `Display`/`FromStr` de `LocationLevel`, `FlatNode`, `flatten`   |
| `crates/domain/src/project/mod.rs`        | `FromStr` de `ProjectKind`, `Display` de `MatchRule`              |
| `crates/application/src/projects/`        | `ListProjects`, `ShowProject`, `BrowseLocations`, `CreateProject` |
| `crates/application/src/dto.rs`           | `ProjectDetail`                                                         |
| `crates/adapters/clickup/src/provider.rs` | `locations`, `space_node`, `folder_node`                            |
| `crates/adapters/fs/src/project_file.rs`  | El formato de `project.toml` y sus conversiones                         |
| `crates/adapters/fs/src/projects.rs`      | `FsProjectRepository`                                                   |
| `crates/bootstrap/src/build.rs`           | `projects_dir`                                                          |
| `apps/cli/src/commands/project.rs`        | `project list                                                             |

**Lo que ya hice yo** (pegamento y tipos, sin lógica para implementar):

- Nuevos métodos en los puertos y sus fakes (`FakeTaskProvider::with_locations`, `FakeProjectRepository` con `get` y `create`).
- La sección `[projects]` de la configuración (`dir`, `default`) y sus claves en `ConfigKey::KNOWN`. Agregué `projects: ProjectsSettings::default()` a tu `AppSettings::new`.
- `bootstrap::build` recibe la ruta de `config.toml` y usa `FsProjectRepository`. `NoProjects` desapareció: ya no hace falta el objeto nulo.
- Los argumentos nuevos del CLI y el despacho en `commands/mod.rs`.
- `SettingsChange::Replace` ahora guarda un `Box<AppSettings>` (ver "Conceptos").

---

## Pasos

| #           | Archivo                                        | Qué implementar                                                                      | Verificar con                                       | Conceptos                                         |
| ----------- | ---------------------------------------------- | ------------------------------------------------------------------------------------- | --------------------------------------------------- | ------------------------------------------------- |
| **A** |                                                | **Dominio**                                                                     |                                                     |                                                   |
| 1           | `task/location.rs`                           | `Display` y `FromStr` de `LocationLevel`                                        | `cargo test -p domain location`                   | `Display` y `FromStr` como inversos           |
| 2           | `task/location.rs`                           | `flatten`                                                                           | (mismo comando)                                     | Recursión, lifetimes explícitos, préstamos     |
| 3           | `project/mod.rs`                             | `FromStr` de `ProjectKind`, `Display` de `MatchRule`                          | `cargo test -p domain project::tests`             | `write!`                                        |
| **B** |                                                | **Aplicación**                                                                 |                                                     |                                                   |
| 4           | `settings/mod.rs`                            | Regla 5 de `validate` (busca `TODO(fase 5)`)                                      | `cargo test -p application --lib settings::tests` | *let chains*                                    |
| 5           | `dto.rs`                                     | `From<&Project> for ProjectDetail`                                                  | `cargo test -p application --lib dto`             | `ToString`, `Path::display`                   |
| 6           | `projects/*.rs`                              | Los 4 casos de uso                                                                    | `cargo test -p application --lib projects`        | Validar antes de guardar                          |
| **C** |                                                | **Adaptadores**                                                                 |                                                     |                                                   |
| 7           | `clickup/src/mapper.rs`                      | `location_node`                                                                     | `cargo test -p adapter-clickup --lib mapper`      | Mover campos                                      |
| 8           | `clickup/src/provider.rs`                    | `locations`, `space_node`, `folder_node`                                        | `cargo test -p adapter-clickup --test provider`   | Concurrencia anidada, temporales y futures        |
| 9           | `fs/src/project_file.rs`                     | Los dos `TryFrom`, el `From` y `expand_home`                                    | `cargo test -p adapter-fs --lib project_file`     | `serde(untagged)`, `collect` sobre `Result` |
| 10          | `fs/src/projects.rs`                         | `read`, `list`, `get`, `create`                                               | `cargo test -p adapter-fs --lib projects::`       | `read_dir`, rutas                               |
| **D** |                                                | **Raíz de composición**                                                       |                                                     |                                                   |
| 11          | `bootstrap/src/build.rs`                     | `projects_dir`                                                                      | `cargo test -p bootstrap`                         | `Path::join` con rutas absolutas                |
| **E** |                                                | **CLI**                                                                         |                                                     |                                                   |
| 12          | `cli/src/commands/today.rs`                  | `query`                                                                             | `cargo test -p taskflow today`                    | Flags contra configuración                       |
| 13          | `cli/src/presenter.rs`                       | `group_title`, `kind_label`, `projects`, `project_detail`, `location_label` | `cargo test -p taskflow presenter`                | Guardas en `match`                              |
| 14          | `cli/src/commands/project.rs`, `wizard.rs` | `run`, `default_slug`, `project_add`                                            | A mano (ver abajo)                                  | `MultiSelect`, `validate_with`                |
| 15          | —                                             | Probar contra tu ClickUp real                                                         | `cargo run -p taskflow -- project add`            | —                                                |

Al terminar, **borra el bloque `#![allow(...)]` marcado con `TODO(fase 5)`** en estos seis archivos:

- `crates/domain/src/lib.rs`
- `crates/application/src/lib.rs`
- `crates/adapters/clickup/src/lib.rs`
- `crates/adapters/fs/src/lib.rs`
- `crates/bootstrap/src/lib.rs`
- `apps/cli/src/main.rs`

```sh
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

---

## Antes de empezar: conceptos nuevos de esta fase

### `Display` y `FromStr` como inversos

```rust
assert_eq!(level.to_string().parse::<LocationLevel>(), Ok(level));
```

Si un tipo se escribe en un archivo (`level = "list"`) y se vuelve a leer, sus dos conversiones deben ser **inversas**: lo que `Display` escribe, `FromStr` lo entiende. El test `known_levels_round_trip` lo comprueba para cada variante.

La excepción es `LocationLevel::Other(String)`: se puede **mostrar**, pero no **parsear**. Si `FromStr` aceptara cualquier texto como `Other`, un error de tipeo en `project.toml` (`level = "lsit"`) se convertiría en un nivel válido que nunca coincide con nada, y el proyecto quedaría vacío sin ninguna explicación. Mejor un error claro.

### Tipos recursivos y recursión con lifetimes

```rust
pub struct LocationNode {
    pub segment: LocationSegment,
    pub children: Vec<LocationNode>,   // un nodo contiene nodos
}
```

Un struct no puede contenerse a sí mismo directamente: su tamaño sería infinito. `Vec<LocationNode>` sí funciona porque un `Vec` es siempre un puntero, una longitud y una capacidad (24 bytes); los hijos viven en el heap. Por eso no hace falta `Box`.

Para recorrer un árbol, lo natural es una función **recursiva** que se llama para los hijos. `flatten` necesita una auxiliar que acumule en un `Vec` compartido:

```rust
fn walk<'a>(nodes: &'a [LocationNode], depth: usize, out: &mut Vec<FlatNode<'a>>)
```

Aquí los lifetimes **no se pueden omitir**: hay dos referencias de entrada (`nodes` y `out`), y Rust no puede adivinar de cuál salen las referencias que se guardan en `out`. `'a` dice: "los `FlatNode` que guardo en `out` toman prestado de `nodes`".

### `FlatNode<'a>`: aplanar sin copiar

```rust
pub struct FlatNode<'a> {
    pub depth: usize,
    pub segment: &'a LocationSegment,   // préstamo, no copia
}
```

El asistente necesita una lista para el selector, pero el árbol ya existe. `FlatNode` solo guarda una referencia a cada segmento: aplanar un árbol de 200 listas no copia ni un `String`. El test `flatten_borrows_the_segments` lo comprueba comparando direcciones de memoria con `std::ptr::eq`.

### `#[serde(untagged)]`: el archivo no dice qué variante es

```toml
[[match]]          # ← ¿es una ubicación o una etiqueta? Depende de los campos
level = "list"
id = "901100200300"

[[match]]
tag = "acme"
```

```rust
#[serde(untagged)]
enum RuleFile {
    Location { level: String, id: String },
    Tag { tag: String },
}
```

`untagged` prueba las variantes **en orden** y se queda con la primera cuyos campos encajan. Es cómodo para el usuario, con un costo: si ninguna encaja (`level` sin `id`), el error es genérico ("data did not match any variant"), sin decir qué faltó. Con etiqueta (`#[serde(tag = "type")]`, como `ProviderKind` en la fase 4), los errores son más claros pero el archivo exige un campo extra.

### Más atributos de `serde`

```rust
#[serde(rename = "match", default)]    // `match` es palabra reservada en Rust
pub rules: Vec<RuleFile>,

#[serde(default, skip_serializing_if = "is_zero")]   // 0 no se escribe en el archivo
pub priority_boost: i64,
```

`skip_serializing_if` recibe el **nombre de una función** que toma una referencia al campo y devuelve `bool`. Así el `project.toml` que genera `create` queda limpio: sin `priority_boost = 0` ni `repo_path` vacío.

### `collect` sobre un iterador de `Result`

```rust
let rules: Vec<MatchRule> = file
    .rules
    .into_iter()
    .map(MatchRule::try_from)            // Iterator<Item = Result<MatchRule, DomainError>>
    .collect::<Result<_, _>>()?;          // Result<Vec<MatchRule>, DomainError>
```

Lo usaste en la fase 3 con los comentarios. `collect` puede construir un `Result<Vec<T>, E>` a partir de un iterador de `Result<T, E>`: se detiene en el primer `Err` y lo devuelve.

### Futures que toman prestados temporales

```rust
// ❌ error[E0716]: temporary value dropped while borrowed
futures::try_join!(
    self.client.get::<ApiFoldersPage>(&format!("/space/{}/folder", id), &params),
    ...
)
```

`format!(...)` crea un `String` **temporal** que se destruye al terminar la expresión en la que aparece. Pero `get(&path, ...)` devuelve un `Future` que **guarda la referencia** `&path` hasta que se ejecuta, y `try_join!` lo ejecuta después. La referencia viviría más que el texto.

Sin `async` esto no pasa: una función normal usa `&path` y termina en la misma línea. Con `async`, la función "termina" cuando se hace `.await`, y para entonces el temporal ya no existe. La solución es darle un nombre:

```rust
let folders_path = format!("/space/{}/folder", id);   // vive hasta el final del bloque
futures::try_join!(self.client.get::<ApiFoldersPage>(&folders_path, &params), ...)
```

### Concurrencia anidada

```text
locations:     GET /team/{id}/space
                 │
                 ├─ space_node(S1) ─┬─ GET /space/S1/folder ┐ try_join!
                 │                  └─ GET /space/S1/list   ┘
                 │                     └─ folder_node(F1) ── GET /folder/F1/list   ┐ try_join_all
                 │                     └─ folder_node(F2) ── GET /folder/F2/list   ┘
                 └─ space_node(S2) ─ ...                                   } try_join_all
```

Cada nivel lanza en paralelo lo que puede: todos los espacios a la vez, y dentro de cada espacio, sus dos listas a la vez y todas sus carpetas a la vez. Con 5 espacios y 20 carpetas son 31 peticiones; el semáforo de la fase 3 sigue limitando cuántas van al mismo tiempo, sin que este código lo sepa.

### `Box` para una variante grande: `large_enum_variant`

```rust
pub enum SettingsChange {
    Set { key: ConfigKey, value: String },   // ~40 bytes
    Replace(Box<AppSettings>),               // 8 bytes (antes: ~250)
}
```

Un enum ocupa lo que su **variante más grande**. Al agregar `[projects]`, `AppSettings` creció y clippy avisó: cada `SettingsChange::Set` cargaba con 200 bytes vacíos. Con `Box`, la configuración vive en el heap y la variante guarda solo un puntero. Por eso el asistente ahora hace `SettingsChange::Replace(Box::new(settings))`.

### *Let chains* (Rust 2024)

```rust
if let Some(default) = &self.projects.default
    && let Err(error) = default.parse::<ProjectSlug>()
{
    // ...
}
```

Desde la edición 2024 se pueden encadenar `let` con `&&` en un mismo `if`. Antes había que anidar dos `if let`, y clippy pedía juntarlos (`collapsible_if`).

### `Path::join` con rutas absolutas

```rust
Path::new("/home/ivan/.config/taskflow").join("projects")       // → /home/ivan/.config/taskflow/projects
Path::new("/home/ivan/.config/taskflow").join("/srv/projects")  // → /srv/projects
```

Si lo que se agrega es una ruta absoluta, `join` la **reemplaza** en lugar de concatenarla. Por eso `projects.dir` puede ser relativo o absoluto sin un `if`.

---

## Especificación por paso

Cada paso dice **qué recibe, qué devuelve, en qué orden y con qué error falla**. Si sigues la especificación al pie de la letra, los tests pasan. Los fragmentos de código son las piezas clave; el resto es tuyo.

### Paso 1: `Display` y `FromStr` de `LocationLevel` (`domain/src/task/location.rs`)

**`Display`:** un `match self`, un brazo por variante, cada uno con `f.write_str(...)`:

| Variante        | Texto             |
| --------------- | ----------------- |
| `Workspace`   | `"workspace"`   |
| `Space`       | `"space"`       |
| `Folder`      | `"folder"`      |
| `List`        | `"list"`        |
| `Repository`  | `"repository"`  |
| `Other(name)` | `name` tal cual |

**`FromStr`:** un `match s` con los cinco textos de la tabla → `Ok(variante)`. Cualquier otro texto, incluidos `""` y `"List"` con mayúscula:

```rust
other => Err(DomainError::InvalidLevel(other.to_owned())),
```

Nunca devuelvas `Ok(Self::Other(...))`: `Other` se puede mostrar pero no parsear.

### Paso 2: `flatten` (`domain/src/task/location.rs`)

- **Entrada:** `&[LocationNode]` (el árbol). **Salida:** `Vec<FlatNode<'_>>`, una fila por nodo.
- **Orden:** preorden. Cada nodo, e **inmediatamente después** todos sus descendientes, antes de pasar a su hermano.
- **Profundidad:** 0 para los nodos de la lista recibida, +1 por cada nivel hacia abajo.
- **`segment`:** una referencia al segmento del árbol (`&node.segment`), nunca un `clone()`.
- **Árbol vacío:** `Vec` vacío.

Estructura: una función interna `walk` que recorre y escribe en un `Vec` compartido.

```rust
fn walk<'a>(nodes: &'a [LocationNode], depth: usize, out: &mut Vec<FlatNode<'a>>) {
    for node in nodes {
        // 1. out.push(FlatNode con depth y &node.segment)
        // 2. walk(&node.children, depth + 1, out)
    }
}
// flatten: crear `out`, llamar walk(nodes, 0, &mut out), devolver `out`
```

### Paso 3: `ProjectKind` y `MatchRule` (`domain/src/project/mod.rs`)

**`FromStr` de `ProjectKind`:** `"client"` → `Client`, `"internal"` → `Internal`. Cualquier otro (incluido `"cliente"`) → `Err(DomainError::InvalidKind(texto.to_owned()))`.

**`Display` de `MatchRule`:**

| Regla                      | Texto                   | Cómo                                                        |
| -------------------------- | ----------------------- | ------------------------------------------------------------ |
| `Location { level, id }` | `"list 901100200300"` | `write!(f, "{level} {id}")`: usa el `Display` del paso 1 |
| `Tag(tag)`               | `"tag acme"`          | `write!(f, "tag {tag}")`                                   |

### Paso 4: regla 5 de `validate` (`application/src/settings/mod.rs`)

**Dónde:** en lugar del comentario `// TODO(fase 5): regla 5.`, después de la regla 4 y antes de `if issues.is_empty()`.

**Condición exacta:** se agrega un problema **solo si** se cumplen las dos:

1. `self.projects.default` es `Some(texto)`, y
2. `texto.parse::<ProjectSlug>()` devuelve `Err(error)`.

| `projects.default`  | Resultado           |
| --------------------- | ------------------- |
| `None`              | Nada (es opcional)  |
| `Some("acme")`      | Nada (slug válido) |
| `Some("ACME Corp")` | Un problema         |

**El problema:**

| Clave                  | Mensaje                                                                                                                      |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `"projects.default"` | `error.to_string()`, el mensaje del `DomainError`: *invalid project slug `ACME Corp`: use 1-40 lowercase letters...* |

**Forma:** un *let chain* (ver "Conceptos"):

```rust
if let Some(default) = &self.projects.default
    && let Err(error) = default.parse::<ProjectSlug>()
{
    issues.push(SettingsIssue::new("projects.default", error.to_string()));
}
```

Necesitas `use domain::project::ProjectSlug;` arriba del archivo.

### Paso 5: `From<&Project> for ProjectDetail` (`application/src/dto.rs`)

Campo por campo:

| Campo de `ProjectDetail` | Valor                                                                        | Ejemplo                                                      |
| -------------------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------ |
| `slug`                   | `project.slug.to_string()`                                                 | `"acme"`                                                   |
| `name`                   | `project.name.clone()`                                                     | `"ACME Corp"`                                              |
| `kind`                   | `project.kind.to_string()`                                                 | `"client"` (en inglés: la traducción es del presentador) |
| `rules`                  | Cada regla con su `Display`, **en el mismo orden**                   | `["list L1", "tag acme"]`                                  |
| `repo_path`              | `None` → `None`; `Some(ruta)` → `Some(ruta.display().to_string())` | `Some("/code/acme")`                                       |
| `priority_boost`         | `project.priority_boost` (es `i64`, `Copy`)                            | `20`                                                       |

Las dos expresiones que no son obvias:

```rust
rules: project.rules.iter().map(ToString::to_string).collect(),
repo_path: project.repo_path.as_ref().map(|path| path.display().to_string()),
```

- `ToString::to_string` como función: `ToString` viene gratis con `Display` (paso 3).
- `PathBuf` no implementa `Display`, porque una ruta puede no ser UTF-8 válido. `display()` devuelve algo que sí lo implementa.
- `as_ref()` convierte `&Option<PathBuf>` en `Option<&PathBuf>`: así `map` no intenta mover la ruta fuera de un préstamo.

### Paso 6: casos de uso (`application/src/projects/`)

**`ListProjects::execute`** (`list_projects.rs`):

1. `self.ctx.projects.list().await?`. Un `RepoError` se convierte solo en `AppError::Repo`.
2. Convertir cada `Project` con `ProjectDetail::from` y devolver el `Vec`.
3. **No reordenes:** el repositorio ya los entrega ordenados por slug.

**`ShowProject::execute(slug)`** (mismo archivo):

1. `self.ctx.projects.get(slug).await?`. Si no existe, el `RepoError::NotFound` llega como `AppError::Repo(RepoError::NotFound(..))`, que es lo que espera el test.
2. `Ok(ProjectDetail::from(&project))`.

**`BrowseLocations::execute`** (`browse_locations.rs`): una línea, `Ok(self.ctx.tasks.locations().await?)`. El `?` convierte `ProviderError` en `AppError::Provider`.

**`CreateProject::execute(draft)`** (`create_project.rs`), **en este orden exacto**, porque si una validación falla no debe guardarse nada:

| # | Qué                                                              | Si falla                                                                           |
| - | ----------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| 1 | `let name = draft.name.trim();` y comprobar `name.is_empty()` | `return Err(AppError::InvalidInput("the project name cannot be empty".into()))`  |
| 2 | Comprobar `draft.rules.is_empty()`                              | `return Err(AppError::InvalidInput("a project needs at least one rule".into()))` |
| 3 | Construir el `Project` (tabla de abajo)                         | —                                                                                 |
| 4 | `self.ctx.projects.create(&project).await?`                     | `AlreadyExists` llega como `AppError::Repo(..)`                                |
| 5 | `Ok(ProjectDetail::from(&project))`                             | —                                                                                 |

El `Project` del punto 3:

| Campo                              | Valor                                                                                      |
| ---------------------------------- | ------------------------------------------------------------------------------------------ |
| `slug`                           | `draft.slug` (se mueve)                                                                  |
| `name`                           | `name.to_owned()`: el nombre **recortado**, `"  ACME Corp  "` → `"ACME Corp"` |
| `kind`, `rules`, `repo_path` | Los de `draft` (se mueven)                                                               |
| `priority_boost`                 | `0` (se cambia a mano en `project.toml`)                                               |

Detalle de préstamos: `name` toma prestado `draft.name` y después mueves `draft.slug`, `draft.rules`... Rust lo permite porque son **campos distintos** de un valor que te pertenece (*movimientos parciales*, fase 2).

### Paso 7: `location_node` (`clickup/src/mapper.rs`)

Un struct literal, moviendo los `String` de `item` (llega por valor):

```rust
LocationNode {
    segment: LocationSegment { level, id: item.id, name: item.name },
    children,
}
```

### Paso 8: árbol de ubicaciones (`clickup/src/provider.rs`)

Las tres peticiones llevan los parámetros de `archived_false()` (ya hecha: `?archived=false`).

**`locations()`:**

| # | Qué                                                                                                                                                       |
| - | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1 | `let page: ApiSpacesPage = self.client.get(&format!("/team/{}/space", self.team_id), &archived_false()).await?;`                                         |
| 2 | Construir **todos los espacios en paralelo**, en el mismo orden: `try_join_all(page.spaces.into_iter().map(\|space\| self.space_node(space))).await` |

**`space_node(space)`:**

| # | Qué                                                                                                                      | Detalle                                                                                                                                                           |
| - | ------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1 | `let params = archived_false();`                                                                                        |                                                                                                                                                                   |
| 2 | `let folders_path = format!("/space/{}/folder", space.id);` y `let lists_path = format!("/space/{}/list", space.id);` | **En variables**, no dentro de `try_join!` (error E0716, ver "Conceptos")                                                                                 |
| 3 | Pedir las dos **a la vez**                                                                                          | `let (folders, lists) = futures::try_join!(self.client.get::<ApiFoldersPage>(&folders_path, &params), self.client.get::<ApiListsPage>(&lists_path, &params))?;` |
| 4 | Hijos = las carpetas **con `hidden == false`**, cada una con `self.folder_node(folder)`, **en paralelo**  | `let mut children = try_join_all(folders.folders.into_iter().filter(\|f\| !f.hidden).map(\|f\| self.folder_node(f))).await?;`                                       |
| 5 | Agregar **después** las listas sin carpeta, como hojas                                                             | `children.extend(lists.lists.into_iter().map(\|list\| location_node(LocationLevel::List, list, Vec::new())));`                                                    |
| 6 | Devolver el espacio con esos hijos                                                                                        | `Ok(location_node(LocationLevel::Space, space, children))`                                                                                                      |

Orden resultante de los hijos de un espacio: primero las carpetas (en el orden de la API), después las listas sin carpeta. `space` se mueve en el paso 6, por eso los `format!` que leen `space.id` van antes.

**`folder_node(folder)`:**

| # | Qué                                                                                                                                                                                    |
| - | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1 | `let page: ApiListsPage = self.client.get(&format!("/folder/{}/list", folder.id), &archived_false()).await?;` (aquí sí puede ir dentro: el `.await` está en la misma expresión) |
| 2 | Cada lista → `location_node(LocationLevel::List, list, Vec::new())`, en un `Vec`                                                                                                   |
| 3 | `location_node` pide un `ApiNamedRef` y tienes un `ApiFolder`: crea `ApiNamedRef { id: folder.id, name: folder.name }`                                                          |
| 4 | `Ok(location_node(LocationLevel::Folder, esa_ref, listas))`                                                                                                                           |

Los tests montan respuestas falsas con `.expect(1)` por ruta: si pides algo dos veces, o pides las listas de la carpeta oculta, fallan.

### Paso 9: `project.toml` ↔ `Project` (`fs/src/project_file.rs`)

**`TryFrom<RuleFile> for MatchRule`:**

| `RuleFile`               | `MatchRule`                                         | Error                                                   |
| -------------------------- | ----------------------------------------------------- | ------------------------------------------------------- |
| `Location { level, id }` | `MatchRule::Location { level: level.parse()?, id }` | `InvalidLevel` del paso 1 si `level` es desconocido |
| `Tag { tag }`            | `MatchRule::Tag(tag)`                               | —                                                      |

**`TryFrom<ProjectFile> for Project`:** escribe los campos **en este orden** (en un struct literal, los `?` se evalúan en el orden en que los escribes):

| Campo              | Valor                                                                          | Error                            |
| ------------------ | ------------------------------------------------------------------------------ | -------------------------------- |
| `slug`           | `file.slug.parse()?`                                                         | `InvalidSlug`                  |
| `name`           | `file.name`                                                                  | —                               |
| `kind`           | `file.kind.parse()?`                                                         | `InvalidKind`                  |
| `rules`          | `file.rules.into_iter().map(MatchRule::try_from).collect::<Result<_, _>>()?` | El de la primera regla inválida |
| `repo_path`      | `file.repo_path.map(PathBuf::from)`, **sin** expandir `~`            | —                               |
| `priority_boost` | `file.priority_boost`                                                        | —                               |

**`From<&Project> for ProjectFile`:** el camino inverso.

| Campo              | Valor                                                                                                                                                                                     |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `slug`, `kind` | `.to_string()`                                                                                                                                                                          |
| `name`           | `.clone()`                                                                                                                                                                              |
| `repo_path`      | `project.repo_path.as_ref().map(\|p\| p.display().to_string())`                                                                                                                           |
| `priority_boost` | copia                                                                                                                                                                                     |
| `rules`          | Cada `MatchRule::Location { level, id }` → `RuleFile::Location { level: level.to_string(), id: id.clone() }`; cada `MatchRule::Tag(tag)` → `RuleFile::Tag { tag: tag.clone() }` |

**`expand_home(path, home)`:**

| Caso                                        | Resultado                          |
| ------------------------------------------- | ---------------------------------- |
| `home` es `None`                        | `path.to_path_buf()` sin cambios |
| `path.strip_prefix("~")` da `Ok(resto)` | `home.join(resto)`               |
| `path.strip_prefix("~")` da `Err(_)`    | `path.to_path_buf()` sin cambios |

| Entrada (home `/Users/ivan`)                | Salida                    |
| --------------------------------------------- | ------------------------- |
| `~/code/acme`                               | `/Users/ivan/code/acme` |
| `~`                                         | `/Users/ivan`           |
| `/srv/acme`, `code/acme`, `~other/acme` | Sin cambios               |

`strip_prefix` compara **componentes** de la ruta, no texto: `~other` es un componente distinto de `~`.

### Paso 10: `FsProjectRepository` (`fs/src/projects.rs`)

Formato de los errores con ruta: `format!("{}: {motivo}", path.display())`. El test `invalid_files_name_the_file` comprueba que el mensaje contenga `acme/project.toml`.

**`read(path)`**, en este orden:

| # | Qué                                                                                      | Si falla                                                                           |
| - | ----------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| 1 | `fs::read_to_string(path)`                                                              | `RepoError::Storage("<ruta>: <error>")`                                          |
| 2 | `toml::from_str::<ProjectFile>(&text)`                                                  | `RepoError::Invalid("<ruta>: <error>")`                                          |
| 3 | `Project::try_from(file)`                                                               | `RepoError::Invalid("<ruta>: <error>")`                                          |
| 4 | El nombre de la carpeta que contiene el archivo debe ser igual al slug                    | `RepoError::Invalid("<ruta>: slug `<slug></slug>` does not match its folder")` |
| 5 | `project.repo_path = project.repo_path.map(\|p\| expand_home(&p, self.home.as_deref()));` | —                                                                                 |
| 6 | `Ok(project)`                                                                           | —                                                                                 |

Para el punto 4, el nombre de la carpeta es `path.parent().and_then(Path::file_name).and_then(|name| name.to_str())`, un `Option<&str>`. Compáralo con `Some(project.slug.as_str())`. Una closure `let invalid = |message: String| RepoError::Invalid(format!("{}: {message}", path.display()));` te ahorra repetir el formato en 2, 3 y 4.

**`list()`:**

| # | Qué                                                                                                                                                                |
| - | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1 | `fs::read_dir(&self.dir)`. Si el error es `std::io::ErrorKind::NotFound` → `Ok(Vec::new())` (todavía no hay proyectos). Cualquier otro error → `Storage` |
| 2 | Por cada entrada (`entry?`, error → `Storage`): `let file = entry.path().join(FILE_NAME);`                                                                   |
| 3 | Si `file.is_file()` → `projects.push(self.read(&file)?)`. Si no (archivo suelto o carpeta sin `project.toml`) → se ignora                                   |
| 4 | Ordenar por slug: `projects.sort_by(\|a, b\| a.slug.cmp(&b.slug))` (`read_dir` no garantiza ningún orden)                                                        |

**`get(slug)`:** `let file = self.file(slug);`. Si `!file.exists()` → `Err(RepoError::NotFound(slug.to_string()))`. Si existe → `self.read(&file)`.

**`create(project)`:**

| # | Qué                                                                    | Si falla                                            |
| - | ----------------------------------------------------------------------- | --------------------------------------------------- |
| 1 | Si `self.dir.join(project.slug.as_str()).exists()`                    | `Err(RepoError::AlreadyExists(slug.to_string()))` |
| 2 | `toml::to_string_pretty(&ProjectFile::from(project))`                 | `RepoError::Invalid(e.to_string())`               |
| 3 | `atomic::write(&self.file(&project.slug), &text)` (crea las carpetas) | `RepoError::Storage(e.to_string())`               |

### Paso 11: `projects_dir` (`bootstrap/src/build.rs`)

`config_path.parent()` da la carpeta de `config.toml` (`Option`: una ruta como `config.toml` sola no tiene padre → usa `Path::new(".")`). Después `.join(&settings.projects.dir)`. Si `projects.dir` es absoluto, `join` lo devuelve tal cual: no hace falta un `if`.

### Paso 12: `today::query` (`cli/src/commands/today.rs`)

| Campo de `TodayQuery` | Valor                                                                               |
| ----------------------- | ----------------------------------------------------------------------------------- |
| `limit`               | `limit` tal cual                                                                  |
| `project`             | `project` tal cual                                                                |
| `group_by`            | `GroupBy::Project` si `by_project` es `true`; si no, `settings.ui.group_by` |

El flag gana sobre la configuración. Un `if` es una expresión y puede ir directo como valor del campo.

### Paso 13: presentador (`cli/src/presenter.rs`)

**`kind_label(kind)`:** `"client"` → `"cliente"`, `"internal"` → `"interno"`, cualquier otro → el mismo texto que llegó.

**`group_title(group, among_others)`:** un `match &group.project`:

| `group.project` | `among_others` | Resultado                                                                              |
| ----------------- | ---------------- | -------------------------------------------------------------------------------------- |
| `Some(p)`       | cualquiera       | `Some(format!("{} ({})", p.name, kind_label(&p.kind)))` → `"ACME Corp (cliente)"` |
| `None`          | `true`         | `Some("Sin proyecto".to_owned())`                                                    |
| `None`          | `false`        | `None`                                                                               |

El segundo brazo lleva una guarda: `None if among_others => ...`.

**`projects(list)`:**

- Lista vacía → exactamente `"No hay proyectos. Crea uno con `taskflow project add`."`.
- Si no: `Table::new()`, `.load_preset(NOTHING)`, `.set_header(["Slug", "Name", "Kind", "Rules", "Repo"])` y una fila por proyecto:

| Columna | Valor                                              |
| ------- | -------------------------------------------------- |
| Slug    | `slug`                                           |
| Name    | `name`                                           |
| Kind    | `kind_label(&kind)`                              |
| Rules   | `rules.len().to_string()` (cuántas, no cuáles) |
| Repo    | `repo_path`, o `"—"` si es `None`           |

- Devuelve `table.to_string()`.

**`project_detail(project)`:** un `Vec` de líneas unidas con `"\n"`:

| Línea | Cuándo                            | Texto                                                                                          |
| ------ | ---------------------------------- | ---------------------------------------------------------------------------------------------- |
| 1      | Siempre                            | `format!("{} ({}) · {}", name, slug, kind_label(&kind))` → `ACME Corp (acme) · cliente` |
| 2      | Si `repo_path` es `Some(repo)` | `Repositorio: {repo}`                                                                        |
| 3      | Si `priority_boost != 0`         | `Bonificación de prioridad: {boost}`                                                        |
| 4      | Siempre                            | `Reglas:`                                                                                    |
| 5…    | Una por regla                      | `  - {regla}` (dos espacios, guion, espacio)                                                 |

**`location_label(node)`:**

- Nombre: `segment.name`, o `segment.id` si el nombre está vacío.
- `format!("{}{nombre} ({})", "  ".repeat(node.depth), segment.level)`: dos espacios por nivel y el nivel con el `Display` del paso 1.
- `depth 1`, `Backend`, `folder` → `"  Backend (folder)"`.

### Paso 14: comandos y asistente

**`project::run`** (`cli/src/commands/project.rs`), un `match command`:

| Comando                 | Qué hace                                                                                                                                                  |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `List { json }`       | `ListProjects::new(ctx).execute().await?`. Con `json`, `println!("{}", presenter::json(&lista)?)`; si no, `presenter::projects(&lista)`            |
| `Show { slug, json }` | El slug es el del argumento o, si es `None`, `default_slug(settings)?`. Después `ShowProject` y `presenter::json` o `presenter::project_detail` |
| `Add`                 | `wizard::project_add(ctx).await?`                                                                                                                        |

Al final, `Ok(())`.

**`default_slug(settings)`:**

| `settings.projects.default` | Resultado                                                                                                               |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `None`                      | `Err(anyhow::anyhow!("indica un proyecto o configura uno por defecto: taskflow config set projects.default <slug>"))` |
| `Some(texto)`               | `Ok(texto.parse()?)` (ya es válido: lo comprobó `validate` en el paso 4)                                          |

Con `as_deref()` conviertes `&Option<String>` en `Option<&str>`, y con `ok_or_else` el `None` en el error.

**`wizard::project_add`** (`cli/src/wizard.rs`), las preguntas en este orden, todas con `ColorfulTheme::default()`:

| # | Pregunta                                                                | `dialoguer`                                                                                                         | Qué se hace con la respuesta                                                                                                                                                                  |
| - | ----------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1 | —                                                                      | —                                                                                                                    | `let tree = BrowseLocations::new(ctx).execute().await?;` y `let nodes = flatten(&tree);`. Si está vacío: `anyhow::ensure!(!nodes.is_empty(), "el proveedor no devolvió ubicaciones")` |
| 2 | `"Ubicaciones del proyecto (espacio para marcar, enter para seguir)"` | `MultiSelect`, con `items(&labels)` y `labels` = `presenter::location_label` de cada nodo                     | Devuelve `Vec<usize>` con los índices marcados. Cada índice `i` → `MatchRule::Location { level: nodes[i].segment.level.clone(), id: nodes[i].segment.id.clone() }`                    |
| 3 | `"Etiquetas (separadas por coma, opcional)"`                          | `Input::<String>` con `.allow_empty(true)`                                                                        | `split(',')`, `trim()` a cada una, descartar las vacías y cada una → `MatchRule::Tag` agregada a las reglas                                                                            |
| 4 | `"Slug (minúsculas, dígitos y guiones)"`                            | `Input::<String>` con `.validate_with(\|raw: &String\| raw.parse::<ProjectSlug>().map(\|_\| ()))`                     | `dialoguer` vuelve a preguntar mientras sea inválido. Después `.parse::<ProjectSlug>()?`                                                                                                 |
| 5 | `"Nombre"`                                                            | `Input::<String>` con `.default(...)`: el nombre del **primer** nodo marcado, o `""` si no marcó ninguno | El texto                                                                                                                                                                                       |
| 6 | `"Tipo"`                                                              | `Select` con `.items(&["cliente", "interno"])` y `.default(0)`                                                  | Índice 0 → `ProjectKind::Client`, 1 → `ProjectKind::Internal`                                                                                                                           |
| 7 | `"Repositorio local (opcional)"`                                      | `Input::<String>` con `.allow_empty(true)`                                                                        | Vacío (después de `trim`) → `None`; si no, `Some(PathBuf::from(texto.trim()))`                                                                                                        |
| 8 | —                                                                      | —                                                                                                                    | `CreateProject::new(ctx).execute(ProjectDraft { ... }).await?`                                                                                                                               |
| 9 | —                                                                      | —                                                                                                                    | `println!("✔ Proyecto creado\n\n{}", presenter::project_detail(&detalle));`                                                                                                                 |

Si el usuario no marca ninguna ubicación ni escribe etiquetas, `CreateProject` responde con "a project needs at least one rule". No hace falta validarlo también en el asistente.

---

## Paso 14: probar a mano

Sin ClickUp, con proyectos escritos a mano:

```sh
mkdir -p /tmp/tf/projects/acme
cat > /tmp/tf/config.toml <<'EOF'
version = 1

[provider]
kind = "clickup"

[provider.clickup]
team_id = "9001"
EOF
cat > /tmp/tf/projects/acme/project.toml <<'EOF'
slug = "acme"
name = "ACME Corp"
kind = "client"
repo_path = "~/code/acme"

[[match]]
level = "list"
id = "901100200300"

[[match]]
tag = "acme"
EOF

export TASKFLOW_CLICKUP_TOKEN=pk_falso
cargo run -p taskflow -- project list --config /tmp/tf/config.toml
cargo run -p taskflow -- project show acme --config /tmp/tf/config.toml
cargo run -p taskflow -- project show --config /tmp/tf/config.toml          # pide projects.default
cargo run -p taskflow -- config set projects.default acme --config /tmp/tf/config.toml
cargo run -p taskflow -- project show --config /tmp/tf/config.toml
cargo run -p taskflow -- today --project "ACME Corp" --config /tmp/tf/config.toml   # slug inválido
unset TASKFLOW_CLICKUP_TOKEN
```

## Paso 15: con tu ClickUp real

```sh
cargo run -p taskflow -- project add
cargo run -p taskflow -- project list
cargo run -p taskflow -- today --by-project
cargo run -p taskflow -- config set ui.group_by project   # agrupar siempre, sin el flag
```

Los proyectos quedan en `projects/` junto a tu `config.toml` (`taskflow config path` dice dónde). Puedes editar los `project.toml` a mano: agregar reglas, `priority_boost` o el `repo_path`.

---

## Limitaciones conocidas (aceptadas para esta fase)

- **Subcarpetas:** ClickUp las devuelve en la misma lista que las carpetas; se muestran como hermanas, no anidadas.
- **Plantillas de contexto:** §14.1 dice que `project add` crea también `context/` con plantillas. Llega en la fase 6, con el contexto.
- **`[session_status]` y `[session_hooks]`** en `project.toml` son de la fase 8; hoy `deny_unknown_fields` los rechaza.
- **Proyecto por carpeta actual:** §7 prevé elegir el proyecto según el directorio donde estás (`repo_path`). Llegará con los Playbooks, que es donde más se usa.
- **`untagged`:** una regla mal escrita da un error genérico (ver "Conceptos").

---

## Qué observar mientras implementas

- **`GetTodayTasks` no cambia.** Desde la fase 2 sabía agrupar y filtrar; solo le faltaban proyectos de verdad.
- **El patrón de la fase 3 se repite.** `project_file.rs` es a `project.toml` lo que `api_types.rs` + `mapper.rs` eran al JSON de ClickUp: un tipo que refleja el formato externo y conversiones que validan al entrar al dominio.
- **El CLI sigue sin saber de ClickUp.** `project add` muestra "space", "folder" y "list" porque así se llaman los niveles del dominio, no porque el CLI conozca la API.
- **Los proyectos son texto plano.** Puedes versionar la carpeta `projects/` con git (NFR-16).

Cuando termines, avísame y reviso tu implementación. Las dudas que surjan las agregamos al final de este documento.

---

# Conceptos aplicados

La fase quedó completa. Esta sección reúne las dudas que surgieron mientras la implementabas y las decisiones de cada paso.

## Paso 1: `FromStr` debe rechazar lo desconocido

La primera versión convertía cualquier texto desconocido en `Other`:

```rust
other => Ok(Self::Other(other.to_string()))       // ❌ "lsit" sería un nivel válido
other => Err(DomainError::InvalidLevel(other.to_owned()))   // ✅
```

Con la primera, `level = "lsit"` en un `project.toml` se aceptaba en silencio, la regla nunca coincidía con ninguna tarea y el proyecto quedaba vacío sin explicación. Con la segunda, `project list` dice exactamente qué archivo y qué valor fallan:

```text
Error: invalid data: .../projects/interno/project.toml: unknown location level `lsit`: use workspace, space, folder, list or repository
```

**Regla general:** un `FromStr` es una puerta de entrada al dominio. Si deja pasar valores dudosos, el error aparece lejos, tarde y sin pistas.

## Paso 2: `flatten` y la recursión

```rust
pub fn flatten(nodes: &[LocationNode]) -> Vec<FlatNode<'_>> {
    fn walk<'a>(nodes: &'a [LocationNode], depth: usize, out: &mut Vec<FlatNode<'a>>) {
        for node in nodes {
            out.push(FlatNode { depth, segment: &node.segment });
            walk(&node.children, depth + 1, out);
        }
    }

    let mut out = Vec::new();
    walk(nodes, 0, &mut out);
    out
}
```

- **Una función dentro de otra.** `walk` solo existe para `flatten`. Hace falta porque la recursión necesita dos datos que `flatten` no recibe: la profundidad y el `Vec` compartido.
- **Preorden.** Primero el `push` del nodo y después la llamada para sus hijos: cada nodo queda seguido de sus descendientes.
- **El caso base no se escribe.** Un nodo sin hijos tiene `children` vacío, el `for` de esa llamada no da vueltas y la función vuelve sola.
- **`out` sin `&mut` en la llamada recursiva.** Dentro de `walk`, `out` **ya es** un `&mut Vec`; se pasa tal cual. En `flatten`, `out` es el `Vec` mismo, por eso ahí sí va `&mut out`.
- **El lifetime `'a`.** `walk` recibe dos referencias y guarda préstamos de una dentro de la otra; Rust no puede adivinar cuál es cuál. `'a` dice que las filas de `out` toman prestado de `nodes`. Los hijos viven dentro de `nodes`, así que comparten el `'a` y la llamada recursiva encaja.

Un experimento: quita los `'a` de `walk` y lee el error. El compilador te pide justamente esa relación.

## Paso 5: `as_ref` y `map` sobre un `Option`

### `as_ref`: trabajar con el contenido sin sacarlo

```rust
repo_path: project.repo_path.as_ref().map(|path| path.display().to_string()),
```

`map` (como `filter`, `ok_or`, `unwrap` y `?`) **consume** el `Option`. Si el `Option` está dentro de algo prestado (`&Project`), consumirlo significaría mover el `PathBuf` fuera de un préstamo: error E0507. `as_ref()` convierte `&Option<T>` en `Option<&T>`, que sí se puede consumir porque mover una referencia no mueve el dato.

| Ya lo usaste en... | Código |
| --- | --- |
| Fase 3, `map_location` | `task.folder.as_ref().filter(\|f\| !f.hidden)` |
| Fase 3, comentarios | `comment.assignee.as_ref().map(user_id)` |
| Fase 4, `CommentView` | `comment.assignee.as_ref() == Some(me)` |
| Fase 4, `clickup_config` | `settings.provider.clickup.as_ref().ok_or(...)` |

Su primo `as_deref()` va un paso más allá: `Option<String>` → `Option<&str>` (usado en `default_slug`).

No hay que confundirlo con el trait `AsRef` (`fn open(path: impl AsRef<Path>)`), que es una conversión genérica para aceptar varios tipos en una función.

### ¿Por qué no `Some(project.repo_path?.display().to_string())`?

| | `opt.as_ref().map(f)` | `Some(f(opt.as_ref()?))` |
| --- | --- | --- |
| Si es `None` | El **campo** queda `None`; la función sigue | La **función entera** devuelve `None` |
| Dónde se puede usar | En cualquier función | Solo en funciones que devuelven `Option` |

`From::from` devuelve `Self`, no un `Option`, así que `?` ni siquiera compila ahí. Y aunque compilara, un proyecto sin repositorio haría desaparecer todo el `ProjectDetail`. `?` brilla al encadenar varios pasos que pueden faltar: `path.parent()?.file_name()?.to_str()`.

### `map` en un `Option` no es un iterador

`Option::map` es un método propio de `Option`, equivalente a este `match`:

```rust
match opt {
    Some(x) => Some(f(x)),
    None => None,
}
```

Comparte nombre con `Iterator::map` y `Result::map` porque hace la misma **idea**: transformar lo de adentro y conservar el envoltorio. Una forma de verlo: un `Option` es una lista de 0 o 1 elementos. Por eso también tiene `filter`, `and_then` (el `flat_map` de los iteradores) y se puede recorrer con `for`.

## Paso 6: `collect` deduce su tipo del contexto

```rust
let projects = self.ctx.projects.list().await?.iter().map(ProjectDetail::from).collect();
projects   // ❌ E0277
```

`collect` puede construir un `Vec`, un `HashMap`, un `Result<Vec<_>, _>`... y elige según el tipo esperado. Aquí `projects` se devolvía como resultado de la función, así que Rust dedujo `Result<Vec<ProjectDetail>, AppError>`. Pero `collect` solo arma un `Result<Vec<T>, E>` a partir de elementos `Result<T, E>`, y los elementos eran `ProjectDetail` sueltos.

```rust
let projects = self.ctx.projects.list().await?;
Ok(projects.iter().map(ProjectDetail::from).collect())   // ✅ dentro de Ok, se espera un Vec
```

| Si los elementos son... | `collect()` construye... | Cuándo |
| --- | --- | --- |
| `T` | `Vec<T>` | La conversión no falla (`ProjectDetail::from`) |
| `Result<T, E>` | `Result<Vec<T>, E>` | Puede fallar: para en el primer error (`MatchRule::try_from`) |

Cuando `collect` da un error raro, casi siempre es porque Rust dedujo un tipo de destino distinto del que pensabas. Escribirlo (`let x: Vec<_> = ...`) lo aclara.

### Cláusulas de guarda

```rust
if name.is_empty() {
    return Err(...);
}
if draft.rules.is_empty() {
    return Err(...);
}
```

Después de un `return`, un `else` no aporta nada. Escribir cada validación como un `if` independiente se lee mejor y permite agregar otra sin pensar en el encadenamiento.

## Paso 8: un dato pedido y nunca usado

La primera versión de `folder_node` pedía las listas de la carpeta pero construía el nodo con `Vec::new()`:

```text
 left: [(0, "90110001"), (1, "90110002"),                                              (1, "901100200900"), (0, "90110009")]
right: [(0, "90110001"), (1, "90110002"), (2, "901100200300"), (2, "901100200301"), (1, "901100200900"), (0, "90110009")]
```

**Cómo leer el fallo:** comparar fila por fila. Faltaban justo las de profundidad 2, las listas dentro de una carpeta. Eso apuntaba al único lugar que produce hijos de una carpeta: `folder_node`.

```rust
let lists = page
    .lists
    .into_iter()
    .map(|list| location_node(LocationLevel::List, list, Vec::new()))
    .collect();
// ...
Ok(location_node(LocationLevel::Folder, folder, lists))   // lists, no Vec::new()
```

**Por qué el compilador no avisó:** normalmente habría dicho `warning: unused variable: page`, pero el bloque `#![allow(unused_variables, ...)]` de la fase lo apagaba. Es el mismo motivo por el que esos bloques se borran al terminar. Mientras trabajas, comentarlo un momento y compilar suele señalar el problema.

### Futures que toman prestados temporales (E0716)

```rust
// ❌ el String de format! muere al terminar la expresión, pero el Future guarda &path
futures::try_join!(self.client.get::<ApiFoldersPage>(&format!(...), &params), ...)

// ✅ con nombre, vive hasta el final del bloque
let folders_path = format!("/space/{}/folder", space.id);
futures::try_join!(self.client.get::<ApiFoldersPage>(&folders_path, &params), ...)
```

En `folder_node`, en cambio, `self.client.get(&format!(...), ...).await?` funciona: el `.await` está en la misma expresión, así que el temporal vive lo suficiente.

## Paso 9: los traits de conversión tienen una sola dirección

### Un enum se construye eligiendo una variante

```rust
let a = Self { ... };        // ❌ Self = MatchRule es un enum: ¿qué variante?
Self::Tag(tag)               // ✅
MatchRule::Location { level, id }   // ✅
```

Un struct tiene una sola forma; un enum tiene varias, así que siempre se nombra la variante. Para convertir de un enum a otro, un `match` sobre la entrada elige el brazo, y cada brazo construye la variante correspondiente de la salida:

```rust
match rule {
    RuleFile::Location { level, id } => Ok(MatchRule::Location { level: level.parse()?, id }),
    RuleFile::Tag { tag } => Ok(MatchRule::Tag(tag)),
}
```

El patrón `RuleFile::Location { level, id }` comprueba la variante **y** saca sus campos en variables (*desestructurar*). Como `rule` llegó por valor, los `String` se mueven sin copias. `level.parse()?` usa el `FromStr` del paso 1; el `?` no necesita conversión porque `Self::Error` ya es `DomainError`.

### `TryFrom` de ida no te da `From` de vuelta

```rust
rules: project.rules.into_iter().map(|rule| RuleFile::from(rule)).collect(),
// ❌ expected `RuleFile`, found `MatchRule`
```

| Escribiste | Te da | No te da |
| --- | --- | --- |
| `impl TryFrom<RuleFile> for MatchRule` | `MatchRule::try_from(rule_file)` | `RuleFile::from(match_rule)` |
| `impl From<A> for B` | `B::from(a)` y `a.into()` | `A::from(b)` |

Las conversiones no son simétricas: ir del archivo al dominio **valida** (y puede fallar); volver no valida nada. Son dos funciones distintas, y cada dirección se escribe. El error mencionaba `RuleFile` porque Rust encontró el único `From` que todo tipo tiene: `From<RuleFile> for RuleFile`.

La solución fue agregar el `impl` de vuelta:

```rust
impl From<&MatchRule> for RuleFile {
    fn from(rule: &MatchRule) -> Self {
        match rule {
            MatchRule::Location { level, id } => Self::Location { level: level.to_string(), id: id.clone() },
            MatchRule::Tag(tag) => Self::Tag { tag: tag.clone() },
        }
    }
}
```

Recibe `&MatchRule`, así que clona (`clone()`) en lugar de mover, y usa `to_string()` (el `Display` del paso 1) en lugar de `parse()`.

### `iter()` con algo prestado, `into_iter()` con algo propio

En `From<&Project> for ProjectFile`, `project` es un préstamo:

| Escrito | Resultado |
| --- | --- |
| `project.rules.into_iter()` | ❌ E0507: intenta sacar el `Vec` del préstamo |
| `project.rules.iter()` | ✅ cada regla es `&MatchRule`, justo lo que pide `RuleFile::from` |
| `project.repo_path.map(...)` | ❌ E0507, el mismo caso que el paso 5 |
| `project.repo_path.as_ref().map(...)` | ✅ |

En `TryFrom<ProjectFile> for Project`, en cambio, `file.rules.into_iter()` es correcto: `file` llegó por valor y ya no se usa.

### `strip_prefix` compara componentes, no texto

```rust
match path.strip_prefix("~") {
    Ok(rest) => home.join(rest),
    Err(_) => path.to_path_buf(),
}
```

`~/code/acme` empieza con el componente `~` → `/Users/ivan/code/acme`. `~other/acme` empieza con el componente `~other`, que no es `~` → sin cambios. Con `starts_with` sobre el texto, `~other` habría coincidido por error.

## Paso 10: errores que dicen dónde

```rust
let invalid = |message: String| RepoError::Invalid(format!("{}: {message}", path.display()));
```

Una closure que captura `path` evita repetir el formato en tres errores distintos (TOML inválido, valor inválido, slug que no coincide con la carpeta). Con varios proyectos, saber **qué archivo** falló es la mitad de la solución.

Otras decisiones:

- **`read_dir` no garantiza orden** (depende del sistema de archivos), por eso `list` ordena por slug al final.
- **Una carpeta inexistente no es un error:** antes del primer `project add` no hay `projects/`, y la respuesta correcta es "no hay proyectos".
- **El slug debe coincidir con la carpeta:** si alguien copia `acme/` a `beta/` y olvida cambiar el slug, `get("beta")` devolvería un proyecto llamado `acme`. Mejor detenerse con un error claro.

## Al cerrar la fase

- Se borraron los seis bloques `#![allow(...)]` con `TODO(fase 5)`. Clippy encontró tres imports que habían quedado sin usar en `project_file.rs` (`std::path::self`, `FromStr` y `LocationLevel`): exactamente lo que esos bloques ocultaban.
- `cargo test --workspace`: 211 tests. Clippy y `cargo fmt` sin avisos.

---

## Ajuste posterior: el límite de peticiones de ClickUp

Al usar `project add` con un workspace real, falló con `Error: rate limited by the provider`. ClickUp permite **100 peticiones por minuto** en los planes Free, Unlimited y Business, y el asistente pedía el árbol completo: `1 + 2·espacios + carpetas` peticiones antes de mostrar nada. Con 15 espacios y 80 carpetas son 111: se pasa del límite por sí solo.

Se corrigió con dos cambios.

### 1. Pedir por partes (*lazy loading*)

El puerto pasó de un método a dos:

```rust
async fn root_locations(&self) -> Result<Vec<LocationSegment>, ProviderError>;   // 1 petición
async fn location_children(&self, root: &LocationSegment) -> Result<Vec<LocationNode>, ProviderError>;
```

```text
Antes:  espacios ─► TODOS los espacios ─► TODAS sus carpetas ─► selector     (~100 peticiones)
Ahora:  espacios ─► selector de espacio ─► SOLO ese espacio ─► selector       (~2 + carpetas del espacio)
```

Es el mismo principio que una carpeta de archivos que se despliega al hacer clic: no se lee todo el disco para mostrar la raíz. El asistente es además más rápido, porque solo espera lo que va a mostrar.

`location_node(...).segment` reutiliza la conversión del mapper para crear un `LocationSegment` desde un `ApiNamedRef`, sin escribir otra función. Y el asistente vuelve a armar un árbol con el espacio como raíz (`LocationNode { segment: root, children }`) para seguir usando `flatten` y `location_label` sin cambios.

### 2. Esperar lo que indica ClickUp

Ante un 429, ClickUp envía el header `X-RateLimit-Reset` con el momento en que se libera el cupo, como *timestamp* Unix. Antes el cliente esperaba 2 segundos fijos y se rendía a los 4; ahora espera hasta ese momento:

```rust
fn retry_wait(reset: Option<&str>, now: SystemTime, fallback: Duration) -> Duration {
    let Some(value) = reset.and_then(|raw| raw.trim().parse::<u64>().ok()) else {
        return fallback;
    };
    let reset_at = if value >= 1_000_000_000_000 {
        UNIX_EPOCH + Duration::from_millis(value)
    } else {
        UNIX_EPOCH + Duration::from_secs(value)
    };
    match reset_at.duration_since(now) {
        Ok(remaining) => (remaining + RATE_LIMIT_MARGIN).min(MAX_RATE_LIMIT_WAIT),
        Err(_) => Duration::ZERO,
    }
}
```

| Concepto | Qué hace aquí |
| --- | --- |
| `SystemTime` y `UNIX_EPOCH` | `SystemTime` es un instante del reloj del sistema; `UNIX_EPOCH` es el 1 de enero de 1970. `UNIX_EPOCH + Duration` convierte un *timestamp* en un instante |
| `duration_since` devuelve `Result` | Si `reset_at` ya pasó, no hay duración positiva: es un `Err`. El reloj del sistema puede incluso retroceder (ajustes de hora), y Rust obliga a manejarlo |
| `Duration` con `+` y `.min(...)` | `Duration` se suma y se compara como un número. `.min(MAX)` acota la espera: un header raro no deja el programa colgado |
| Segundos o milisegundos | La documentación de ClickUp no lo dice. Un *timestamp* en segundos tiene hoy 10 dígitos y uno en milisegundos 13, así que el umbral `1_000_000_000_000` los distingue |
| Función pura con `now` como parámetro | El reloj entra como argumento (como `Clock` en la fase 2): los tests usan un instante fijo y comprueban la espera exacta sin dormir |
| `const` con `Duration` | `Duration::from_mins(1)` y `from_millis(500)` se pueden usar en un `const` porque son funciones `const fn` |

El margen de medio segundo (`RATE_LIMIT_MARGIN`) existe porque el *timestamp* viene en segundos enteros: llegar justo en el borde puede chocar otra vez con el límite.

### Avisos con `tracing` en lugar de `println!`

Cuando hay que esperar al menos un segundo, el usuario ve:

```text
 WARN límite de peticiones de ClickUp alcanzado; esperando 23 s
```

El adaptador **no imprime**: registra un evento con `tracing::warn!`. Quien decide si se muestra, dónde y con qué formato es la interfaz, que configura un *subscriber* al arrancar (`apps/cli/src/main.rs`):

```rust
tracing_subscriber::fmt()
    .with_writer(std::io::stderr)          // salida de errores, no la normal
    .with_max_level(tracing::Level::WARN)  // solo avisos y errores
    .without_time()
    .with_target(false)
    .init();
```

- **Por qué no `println!` en el adaptador:** la app de escritorio no tiene terminal, y un servidor web querría el aviso en sus logs. El adaptador emite el evento; cada interfaz lo muestra a su manera. Es el mismo principio que el `presenter`.
- **Por qué `stderr`:** `taskflow today --json | jq` lee la salida normal (`stdout`). Si el aviso fuera ahí, rompería el JSON. Los avisos van a la salida de errores, que la tubería no toca.
- **Lo que viene:** §19 prevé `-v` para ver también los mensajes de nivel `debug` (duración de cada petición, por ejemplo).

### Validar donde se pregunta, no al final

La primera versión del asistente dejaba avanzar sin marcar ninguna ubicación ni escribir etiquetas, preguntaba slug, nombre, tipo y repositorio, y recién al final `CreateProject` respondía `a project needs at least one rule`. Además el texto decía "espacio para marcar", que se confunde con un espacio de ClickUp.

Ahora las ubicaciones y las etiquetas van dentro de un `loop` que solo termina con al menos una regla:

```rust
let (chosen, rules) = loop {
    let chosen = MultiSelect::with_theme(&theme).with_prompt("... (barra espaciadora para marcar, Enter para continuar)")...;
    let mut rules = /* ubicaciones + etiquetas */;
    if !rules.is_empty() {
        break (chosen, rules);      // el loop "devuelve" una tupla con los dos valores
    }
    println!("Marca al menos una ubicación con la barra espaciadora, o escribe una etiqueta.");
};
```

- **`break` con una tupla:** el `loop` como expresión (fase 3) puede devolver varios valores a la vez. Se necesitan los dos: `rules` para el proyecto y `chosen` para sugerir el nombre.
- **La regla sigue en `CreateProject`.** El asistente valida para dar una mejor experiencia; el caso de uso valida para que **ninguna** interfaz pueda crear un proyecto sin reglas. Las dos validaciones tienen motivos distintos, y no sobra ninguna.
