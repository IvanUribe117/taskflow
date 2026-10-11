# Fase 1 explicada: qué se hizo y por qué

Este documento acompaña las fases 0 y 1 del roadmap (`docs/ARCHITECTURE.md`, §24). No repite el diseño: explica **las decisiones de código** y **los conceptos de Rust** que aparecen, en el orden en que conviene entenderlos.

Cada sección termina con un **"Para comprobar"**: una pregunta o un experimento corto. Si puedes responderla, entendiste el concepto.

---

## 1. Qué hay en el repositorio

```text
taskflow/
├── Cargo.toml              ← define el workspace (no es un crate)
├── Cargo.lock              ← versiones exactas de todas las dependencias
├── rust-toolchain.toml     ← versión de Rust del proyecto
├── .github/workflows/ci.yml
├── crates/
│   └── domain/
│       ├── Cargo.toml      ← define el crate `domain`
│       └── src/
│           ├── lib.rs      ← raíz del crate
│           ├── error.rs
│           ├── policy.rs
│           ├── testing.rs  ← constructores para tests
│           ├── task/       ← mod.rs, status.rs, location.rs
│           └── project/    ← mod.rs, resolver.rs
└── docs/
```

Comandos que vas a usar siempre:

```sh
cargo test --workspace                  # todos los tests de todos los crates
cargo test -p domain                    # solo un crate
cargo test -p domain policy             # solo los tests cuyo nombre contiene "policy"
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo doc -p domain --open              # documentación generada desde los comentarios ///
```

---

## 2. Workspace de Cargo

### 2.1 Tres palabras que se confunden

| Término | Qué es | En Taskflow |
| --- | --- | --- |
| **Crate** | La unidad que compila `rustc`: una librería (`lib.rs`) o un ejecutable (`main.rs`). | `domain` es un crate de librería |
| **Package** | Un `Cargo.toml` con sección `[package]`. Contiene uno o más crates. | `crates/domain/Cargo.toml` |
| **Workspace** | Un conjunto de packages que se compilan juntos. | El `Cargo.toml` de la raíz |

El `Cargo.toml` de la raíz **no tiene `[package]`**, solo `[workspace]`. A eso se le llama *workspace virtual*: no es un crate, solo agrupa a los demás.

### 2.2 Qué comparten los miembros del workspace

```toml
[workspace]
resolver = "3"
members = ["crates/domain"]
```

- **Un solo `Cargo.lock`** en la raíz: todos los crates usan exactamente la misma versión de `chrono`, `thiserror`, etc. No puede pasar que `domain` use chrono 0.4.38 y `application` la 0.4.41.
- **Un solo directorio `target/`**: una dependencia se compila una vez y la reutilizan todos los crates. Por eso el segundo `cargo build` es mucho más rápido.
- **Comandos sobre todo el conjunto**: `cargo test --workspace` prueba todos los crates de una vez.

### 2.3 Por qué Taskflow usa varios crates y no uno solo con módulos

Esta es la razón más importante del workspace en este proyecto. La arquitectura hexagonal dice que **el dominio no puede depender de nada externo** (§8.1). Con módulos dentro de un mismo crate, esa regla es solo una convención: nada impide escribir `use reqwest::Client;` en `policy.rs`.

Con crates separados, **la regla la hace cumplir el compilador**. `domain/Cargo.toml` solo declara `chrono`, `indexmap` y `thiserror`; si alguien intenta usar `reqwest` en el dominio, el código no compila:

```text
error[E0432]: unresolved import `reqwest`
```

Lo mismo con las capas: cuando exista `adapter-clickup`, dependerá de `application`, pero `application` nunca podrá depender de `adapter-clickup`. Cargo prohíbe los ciclos entre crates, así que las flechas solo pueden apuntar hacia adentro.

### 2.4 Herencia: `[workspace.package]`, `[workspace.dependencies]` y `[workspace.lints]`

En la raíz:

```toml
[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.95"
publish = false

[workspace.dependencies]
chrono = { version = "0.4", default-features = false, features = ["std"] }
indexmap = "2"
thiserror = "2"
```

En `crates/domain/Cargo.toml`:

```toml
[package]
name = "domain"
version.workspace = true      # ← "usa el valor del workspace"
edition.workspace = true

[dependencies]
chrono.workspace = true       # ← misma versión y features que la raíz
```

- **Por qué:** cuando haya 8 crates, actualizar `chrono` será cambiar **una** línea, no ocho.
- Un crate puede **agregar** features a una dependencia heredada, pero no quitarlas. En la fase 2, `application` usa `chrono = { workspace = true, features = ["clock", "serde"] }`.

### 2.5 `resolver = "3"` y `edition = "2024"`

- **Edición:** Rust publica una edición cada tres años (2015, 2018, 2021, 2024). Cada edición permite cambios de sintaxis o comportamiento sin romper el código antiguo, porque cada crate declara la suya. Taskflow usa la última, 2024.
- **Resolver:** es el algoritmo con el que Cargo elige versiones y features de las dependencias. La versión 3 (la predeterminada con la edición 2024) evita elegir versiones de dependencias que exigen un Rust más nuevo que el `rust-version` declarado.
- **Por qué hay que escribirlo:** en un workspace virtual no hay `[package]` del que deducir la edición, así que el resolver se declara a mano.

### 2.6 Las features se unifican (detalle importante)

`domain` declara `chrono` **sin** la feature `clock`. En chrono, `Utc::now()` solo existe con la feature `now`, que viene incluida en `clock`. Es intencional: el dominio no debe leer el reloj del sistema, debe recibir `now` como parámetro. Así cualquier regla que dependa de la hora se puede probar con una fecha fija.

Hay un matiz: cuando se compila el workspace completo, Cargo **unifica** las features. Si `application` activa `clock`, chrono se compila una sola vez con `clock`, y `domain` también podría ver `Utc::now()`. La garantía completa solo se cumple al compilar `domain` por separado:

```sh
cargo check -p domain     # aquí `Utc::now()` no existe
```

> **Para comprobar:** agrega `let _ = chrono::Utc::now();` en `policy.rs` y ejecuta `cargo check -p domain`. ¿Qué error da? Después bórralo.

---

## 3. `rust-toolchain.toml`

```toml
[toolchain]
channel = "1.95.0"
components = ["rustfmt", "clippy"]
```

`rustup` lee este archivo y usa exactamente Rust 1.95.0 dentro del proyecto, aunque tengas otra versión instalada globalmente. **Por qué:** clippy agrega lints nuevos en cada versión; sin fijarla, el mismo código podría pasar en tu máquina y fallar en CI.

---

## 4. Lints: clippy, `pedantic` y `-D warnings`

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
missing_debug_implementations = "warn"

[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
```

| Lint | Qué hace | Por qué |
| --- | --- | --- |
| `unsafe_code = "forbid"` | Prohíbe `unsafe` en todo el workspace; `forbid` ni siquiera deja reactivarlo con `#[allow]`. | Taskflow no necesita `unsafe`; así nadie lo introduce por accidente. |
| `missing_debug_implementations` | Avisa si un tipo público no implementa `Debug`. | Todo tipo público debe poder imprimirse en logs y en mensajes de tests. |
| `clippy::all` | Activa los grupos de lints recomendados de clippy. | Detecta errores comunes y código poco idiomático. |
| `priority = -1` | Aplica el grupo primero, para que un lint individual pueda sobrescribirlo después. | Sin esto, clippy avisa de la ambigüedad (lint `lint_groups_priority`). |

En `lib.rs` del dominio además:

```rust
#![warn(clippy::pedantic)]
```

`pedantic` es el grupo más estricto. Los avisos que viste en la fase 1 vinieron de ahí:

- **`doc_markdown`:** "ClickUp" en un comentario `///` parecía un identificador, por eso pidió backticks: `` `ClickUp` ``.
- **`similar_names`:** dos variables `resolver` y `resolved` en el mismo test, muy fáciles de confundir al leer.
- **`must_use_candidate`:** por eso hay `#[must_use]` en funciones como `as_str()`. Si alguien llama `task_id.as_str();` y descarta el resultado, el compilador avisa.
- **`missing_errors_doc`:** por eso los `pub fn` que devuelven `Result` tienen una sección `# Errors`.

**Por qué `pedantic` está en `lib.rs` y no en el `Cargo.toml` del crate:** cuando un crate usa `[lints] workspace = true`, no puede agregar lints propios en su `Cargo.toml`. El atributo `#![warn(...)]` al inicio de `lib.rs` sí puede.

**`-D warnings` en CI** convierte todos los avisos en errores. En tu máquina un aviso no bloquea; en CI, el código con avisos no pasa.

---

## 5. CI (`.github/workflows/ci.yml`)

Dos trabajos:

1. **`check`** (Linux): `fmt --check` → `clippy -D warnings` → `test`. Si el formato está mal, ni siquiera se ejecutan los tests.
2. **`build`** (Linux, Windows y macOS): compila en modo release. Valida NFR-08 (multiplataforma). Usa `needs: check` para no gastar tres máquinas si el primer trabajo ya falló.

`Swatinem/rust-cache` guarda `target/` entre ejecuciones; sin esa caché, cada ejecución compilaría todas las dependencias desde cero.

---

## 6. Módulos y visibilidad

### 6.1 Cómo encuentra Rust los archivos

```rust
// lib.rs
pub mod task;     // busca task.rs o task/mod.rs
```

```rust
// task/mod.rs
mod location;     // busca task/location.rs
mod status;

pub use location::{LocationLevel, LocationNode, LocationSegment, TaskLocation};
pub use status::{Priority, Status};
```

- **`mod status;` es privado:** desde fuera del crate no existe `domain::task::status`.
- **`pub use` re-exporta:** desde fuera se escribe `domain::task::Status` en lugar de `domain::task::status::Status`. Esa es la **API pública**.
- **Por qué:** la organización interna en archivos puede cambiar (por ejemplo, dividir `status.rs` en dos) sin romper el código que usa el crate.

### 6.2 Niveles de visibilidad

| Visibilidad | Quién puede usarlo |
| --- | --- |
| (nada) | El módulo actual y sus hijos |
| `pub(crate)` | Todo el crate, pero no otros crates |
| `pub` | Cualquiera que dependa del crate |

### 6.3 `#[cfg(test)]` y la feature `test-support`

```rust
#[cfg(any(test, feature = "test-support"))]
pub mod testing;
```

`cfg` es **compilación condicional**: el módulo `testing` (con `task("1")`, `user("me")`, etc.) solo se compila al ejecutar tests, o cuando otro crate activa la feature `test-support`. En el binario final no existe.

Es una incorporación de la fase 2: `application` necesita los mismos constructores para sus tests y los activa así en su `Cargo.toml`:

```toml
[dev-dependencies]
domain = { workspace = true, features = ["test-support"] }
```

`[dev-dependencies]` solo aplica a tests, ejemplos y benchmarks.

---

## 7. Newtypes y *parse, don't validate*

### 7.1 El problema

Si `Task.id` fuera un `String`, cualquier función que reciba un id tendría que preguntarse "¿estará vacío?". Y nada impediría pasar por error un id de usuario donde se espera uno de tarea: los dos serían `String`.

### 7.2 La solución

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(String);          // ← campo PRIVADO

impl TaskId {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let raw = raw.into();
        if raw.trim().is_empty() {
            return Err(DomainError::EmptyTaskId);
        }
        Ok(Self(raw))
    }
}
```

- **Newtype:** un struct de un solo campo que envuelve otro tipo. No cuesta nada en ejecución (ocupa lo mismo que el `String`), pero para el compilador `TaskId` y `UserId` son tipos distintos.
- **Campo privado:** desde fuera del crate **no se puede** escribir `TaskId(String::new())`. La única puerta de entrada es `TaskId::new`, que valida.
- **Consecuencia:** si una función recibe `&TaskId`, ya sabe que no está vacío. Se valida **una vez**, en la frontera, y el tipo "recuerda" que se validó. Eso significa *parse, don't validate*.

### 7.3 `impl Into<String>`

```rust
pub fn new(raw: impl Into<String>) -> ...
```

Acepta cualquier tipo que se pueda convertir en `String`: `TaskId::new("abc")` (un `&str`) y `TaskId::new(some_string)` (un `String`, sin copiarlo). Es azúcar sintáctica para un genérico: `fn new<S: Into<String>>(raw: S)`.

### 7.4 `FromStr` en `ProjectSlug`

```rust
impl FromStr for ProjectSlug {
    type Err = DomainError;
    fn from_str(s: &str) -> Result<Self, Self::Err> { /* valida */ }
}
```

Implementar `FromStr` habilita `.parse()`:

```rust
let slug: ProjectSlug = "acme".parse()?;
```

**Por qué `FromStr` y no `new`:** es el trait estándar de "texto → tipo". En la fase 4, `clap` (el parser de argumentos del CLI) lo usa automáticamente: `--project ACME` fallará al parsear los argumentos y nunca llegará a la aplicación.

`type Err = DomainError;` es un **tipo asociado**: cada implementación de `FromStr` elige su propio tipo de error.

> **Para comprobar:** ¿por qué `ProjectSlug` implementa `Display` y `TaskId` no? (Pista: busca dónde se usa `slug.to_string()` o `{slug}`.)

---

## 8. Enums

### 8.1 Variantes con datos: `Status::Other(String)`

```rust
pub enum Status {
    Todo, Ready, InProgress, InReview, Done,
    Other(String),
}
```

En Rust un enum puede llevar datos en cada variante. ClickUp permite estados personalizados ("Code Review", "QA"); en lugar de perderlos o forzarlos a una categoría, `Other` los conserva.

### 8.2 `match` exhaustivo

```rust
pub fn value(self) -> i64 {
    match self {
        Self::Urgent => 4,
        Self::High => 3,
        Self::Normal => 2,
        Self::Low => 1,
        Self::None => 0,
    }
}
```

Si mañana se agrega `Priority::Critical`, este `match` **no compila** hasta manejar la variante nueva. Por eso no se usa `_ => 0`: un comodín escondería el caso nuevo.

### 8.3 Patrones con `|`

```rust
Self::Space | Self::Other(_) => 1,
Self::List | Self::Repository => 3,
```

Un brazo para varias variantes. `Other(_)` significa "con cualquier dato dentro".

### 8.4 El orden de declaración define `Ord`

```rust
#[derive(PartialOrd, Ord)]
pub enum Priority { Urgent, High, Normal, Low, None }
```

Con `derive(Ord)`, la primera variante es la menor: `Urgent < High < ... < None`. Ordenar una lista de prioridades deja `Urgent` primero. El test `priority_order_and_value_agree` verifica que ese orden y `value()` coincidan.

### 8.5 Enum privado como herramienta interna: `Urgency`

```rust
enum Urgency { Overdue, DueToday, DueThisWeek, Later, NoDueDate }
```

No es `pub`: solo existe dentro de `policy.rs`. **Por qué un enum y no varios `if`:** primero se *clasifica* la tarea en exactamente una categoría y después se *decide* el bono con un `match`. Así es imposible que una tarea sume dos bonos de vencimiento: una variable de tipo `Urgency` solo puede tener un valor.

---

## 9. Derives: qué pide cada uno y cuándo usarlo

| Derive | Habilita | Por qué en Taskflow |
| --- | --- | --- |
| `Debug` | `{:?}` | Mensajes de `assert_eq!` y logs |
| `Clone` | `.clone()` (copia explícita) | Duplicar tareas, slugs, etc. |
| `Copy` | Copia implícita | Solo en tipos pequeños sin memoria en el heap: `Priority`, `ProjectKind`, `ScoringContext` |
| `PartialEq`, `Eq` | `==` | Comparar estados, ids; `Eq` promete que `a == a` siempre se cumple |
| `Hash` | Uso como clave en `HashMap` o `IndexMap` | `ProjectKey` es la clave de los grupos |
| `PartialOrd`, `Ord` | `<`, `.sort()` | Ordenar prioridades |
| `Default` | `T::default()` | `TaskLocation::default()` = ruta vacía |

**Por qué `Task` no es `Copy`:** tiene `String` y `Vec`, que poseen memoria en el heap. Copiarlos bit a bit dejaría dos dueños de la misma memoria. Rust lo impide: un tipo con `String` dentro no puede derivar `Copy`.

**Por qué `Status` no es `Copy`:** `Other(String)` tiene un `String`. Una sola variante con heap basta para que el enum entero no pueda ser `Copy`.

---

## 10. Ownership y lifetimes en `pending_comments_for`

```rust
pub fn pending_comments_for<'a>(
    &'a self,
    user: &'a UserId,
) -> impl Iterator<Item = &'a Comment> + 'a {
    self.comments
        .iter()
        .filter(move |c| !c.resolved && c.assignee.as_ref() == Some(user))
}
```

Paso a paso:

1. **Devuelve un iterador, no un `Vec`.** No reserva memoria. Quien llama decide qué hacer: `.count()`, `.collect()`, recorrerlo con un `for`...
2. **El iterador toma prestados dos datos:** `self.comments` (lo recorre) y `user` (lo compara). Mientras el iterador exista, ninguno de los dos puede liberarse.
3. **`'a`** le dice al compilador: "el iterador vive como mucho lo mismo que `self` y que `user`". Si intentas usar el iterador después de liberar la tarea, no compila.
4. **`move` en el closure:** el closure se *lleva* la referencia `user` (copia el puntero, no el `UserId`), así que no depende de la variable local `user`, que desaparece al terminar la función. En este caso concreto, `move` es opcional: desde la edición 2021 los closures capturan con precisión, y al ver que solo lee a través de la referencia, el compilador toma prestado directamente `*user` (con lifetime `'a`), no la variable local. Se deja `move` porque expresa la intención: "este closure se va con el iterador". En otros casos sí es obligatorio, por ejemplo cuando el closure usa un valor propio de la función, como un `String` creado dentro.
5. **`impl Iterator`:** "devuelvo algún tipo que implementa `Iterator`". El tipo real (`Filter<slice::Iter<Comment>, {closure}>`) no se puede escribir a mano, porque los closures no tienen nombre.

**Matiz de la edición 2024:** en esta edición, `impl Trait` en el retorno captura automáticamente todos los lifetimes de los parámetros, así que `+ 'a` ya no es obligatorio. Se dejó escrito porque hace explícito qué se toma prestado.

`c.assignee.as_ref() == Some(user)`: `assignee` es `Option<UserId>`. `.as_ref()` lo convierte en `Option<&UserId>` sin mover ni clonar, y así se puede comparar con `Some(user)`, que también es `Option<&UserId>`.

> **Para comprobar:**
> 1. Cambia `user: &'a UserId` por `user: &UserId` y ejecuta `cargo check -p domain`. El error `E0621: explicit lifetime required in the type of user` dice exactamente el punto 3: el iterador toma prestado `user`, así que su lifetime debe estar ligado al del resultado.
> 2. Vuelve a dejarlo como estaba, quita `move` y compila. ¿Por qué ahora sí compila? (Respuesta en el punto 4.)

---

## 11. Combinadores de `Option` y `bool`

En lugar de `if`/`else` anidados, el código usa métodos que transforman valores:

| Código | Equivale a |
| --- | --- |
| `matches!(self.due, Some(due) if due < now)` | `match self.due { Some(due) if due < now => true, _ => false }` |
| `cond.then(\|\| level.depth() + 1)` | `if cond { Some(level.depth() + 1) } else { None }` |
| `cond.then_some(0)` | `if cond { Some(0) } else { None }` (calcula el valor aunque `cond` sea falso) |
| `opt.map_or(0, \|p\| p.priority_boost)` | `match opt { Some(p) => p.priority_boost, None => 0 }` |
| `opt.is_some_and(\|p\| …)` | `match opt { Some(p) => …, None => false }` |
| `let Some(due) = due else { return …; };` | *let-else*: desempaqueta o sale temprano |

`then` recibe un closure (evaluación perezosa); `then_some` recibe el valor directamente. Para valores baratos como `0` da igual; para cálculos costosos se usa `then`.

---

## 12. `ProjectResolver`: iteradores y colecciones

### 12.1 La regla más específica gana

```rust
pub fn resolve(&self, task: &Task) -> Option<&Project> {
    self.projects
        .iter()
        .filter_map(|p| p.best_match(task).map(|score| (score, p)))
        .rev()
        .max_by_key(|(score, _)| *score)
        .map(|(_, p)| p)
}
```

1. `filter_map`: por cada proyecto, si alguna regla coincide, produce `(especificidad, proyecto)`; si no, lo descarta.
2. `max_by_key`: se queda con el de mayor especificidad.
3. **`rev()` antes de `max_by_key`:** si hay empate, `max_by_key` devuelve el **último** máximo. Al invertir el orden, "el último" pasa a ser el primero definido. Lo verifica el test `ties_go_to_the_first_defined_project`.
4. **Retorno `Option<&Project>`:** devuelve una referencia al proyecto, no una copia. El lifetime se infiere: la referencia vive lo mismo que `&self`.

### 12.2 `IndexMap` en lugar de `HashMap` o `BTreeMap`

| Colección | Orden al recorrerla |
| --- | --- |
| `HashMap` | Arbitrario (cambia entre ejecuciones) |
| `BTreeMap` | Por clave (alfabético) |
| `IndexMap` | **Por orden de inserción** |

Las tareas llegan ya ordenadas por prioridad. Al agruparlas, el primer grupo insertado es el del proyecto con la tarea más urgente. `IndexMap` conserva ese orden; con `HashMap` los grupos saldrían en cualquier orden y con `BTreeMap`, alfabéticos.

### 12.3 `entry().or_default()`

```rust
groups.entry(key).or_default().push(task);
```

"Si la clave no existe, inserta un `Vec` vacío; en cualquier caso, dame el `Vec` para modificarlo". Es una sola búsqueda en el mapa, en lugar de buscar con `get` y luego insertar con `insert`.

### 12.4 El lifetime `'t` en `group`

```rust
pub fn group<'t>(&self, tasks: &'t [Task]) -> IndexMap<ProjectKey, Vec<&'t Task>>
```

Los grupos contienen **referencias** a las tareas originales, no copias. `'t` dice que el resultado no puede vivir más que el slice `tasks`. No depende de `&self`: el resolver puede liberarse y los grupos siguen siendo válidos. Por eso se usa un lifetime con nombre en lugar del inferido.

---

## 13. La política de prioridad

### 13.1 Un trait con método por defecto

```rust
pub trait PrioritizationPolicy: Send + Sync {
    fn score(&self, task: &Task, ctx: &ScoringContext<'_>) -> i64;

    fn rank(&self, tasks: &mut [Task], ctx: &ScoringContext<'_>) {
        tasks.sort_by_cached_key(|t| Reverse(self.score(t, ctx)));
    }
}
```

- **Quien implementa el trait solo escribe `score`**; `rank` viene gratis. Una política futura puede sobrescribir `rank` si lo necesita.
- **`Send + Sync` (*supertraits*):** toda política debe poder compartirse entre hilos. Sin esto no se podría guardar en `Arc<dyn PrioritizationPolicy>` dentro de un `AppContext` compartido (§11.1).
- **`&mut [Task]`:** un *slice* mutable. Sirve igual para un `Vec<Task>` o un array; ordena en el lugar, sin crear una lista nueva.

### 13.2 `sort_by_cached_key` y `Reverse`

- `sort_by_key` puede llamar a la función de clave muchas veces por elemento (del orden de *n·log n* llamadas en total). `sort_by_cached_key` la llama **una vez por elemento** y guarda el resultado. Calcular el puntaje recorre comentarios y reglas de proyecto, así que conviene calcularlo una sola vez.
- `Reverse(x)` invierte el orden: ordena de mayor a menor puntaje sin escribir una función de comparación.
- **Es estable:** dos tareas con el mismo puntaje conservan su orden original (el del proveedor). Lo verifica `rank_orders_by_score_and_keeps_ties_stable`.

### 13.3 `ScoringContext<'a>`: un struct que toma prestado

```rust
#[derive(Clone, Copy)]
pub struct ScoringContext<'a> {
    pub now: DateTime<Utc>,
    pub offset: FixedOffset,
    pub me: &'a UserId,
    pub projects: &'a ProjectResolver,
}
```

Un struct que contiene referencias necesita un parámetro de lifetime: `'a` dice que el contexto no puede vivir más que el usuario y el resolver que referencia. Como solo contiene referencias y valores pequeños, puede ser `Copy`.

**Por qué no recibe `now` del sistema:** ya se explicó en §2.6. El llamador decide qué hora es; los tests usan `2026-10-10T15:00:00Z` y siempre dan el mismo resultado.

**Por qué `offset`:** "vence hoy" depende del día del usuario. A las 22:00 en Bogotá ya son las 03:00 del día siguiente en UTC. Sin `offset`, una tarea que vence esa noche se clasificaría como "mañana". Lo verifica `due_today_uses_the_user_local_day`.

### 13.4 Aritmética saturada y `fold`

```rust
let priority = self.priority_weight.saturating_mul(task.priority.value());
// ...
[urgency, comments, review, project]
    .into_iter()
    .fold(priority, i64::saturating_add)
```

- Los pesos vienen de la configuración del usuario. Con valores absurdos, una multiplicación normal podría desbordar el `i64`: en modo debug eso es un *panic*, en release da la vuelta a números negativos. `saturating_*` se queda en `i64::MAX` en lugar de desbordar.
- `fold` recorre la lista acumulando: empieza en `priority` y le suma cada término. `i64::saturating_add` se pasa como función, sin escribir un closure.

### 13.5 `Default` y la sintaxis de actualización de structs

```rust
impl Default for WeightedPolicy { /* pesos de config.toml */ }

let policy = WeightedPolicy {
    priority_weight: 0,
    ..WeightedPolicy::default()       // ← el resto de campos, del default
};
```

`..otro` copia los campos que no se mencionan. Los tests lo usan para cambiar un solo peso.

### 13.6 Uso como trait object

```rust
let policy: Box<dyn PrioritizationPolicy> = Box::new(WeightedPolicy::default());
policy.rank(&mut tasks, &ctx);
```

`dyn PrioritizationPolicy` significa "algún tipo que implementa el trait, decidido en ejecución". El test `policy_works_as_a_trait_object` existe para asegurar que el trait sigue siendo **compatible con `dyn`**. Si alguien le agregara un método genérico (`fn foo<T>()`), dejaría de serlo y ese test no compilaría.

---

## 14. Errores con `thiserror`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum DomainError {
    #[error("task id cannot be empty")]
    EmptyTaskId,
    #[error("invalid project slug `{0}`: ...")]
    InvalidSlug(String),
}
```

- **`thiserror`** genera `impl std::error::Error` e `impl Display` a partir de los atributos `#[error("...")]`. `{0}` es el primer campo de la variante.
- **`#[non_exhaustive]`:** fuera del crate, un `match` sobre `DomainError` **debe** incluir `_ =>`. Así agregar una variante nueva no rompe a los crates que lo usan. Dentro de `domain` no tiene efecto.
- **`PartialEq` en el error:** permite `assert_eq!(TaskId::new(""), Err(DomainError::EmptyTaskId))`.
- **Mensajes en inglés:** según la convención de idioma de `ARCHITECTURE.md` (§1), el código y los mensajes del CLI van en inglés.

---

## 15. Tests

### 15.1 Dónde viven

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task_id_rejects_empty_and_blank_values() { ... }
}
```

Los tests unitarios van **en el mismo archivo** que el código, en un módulo `tests` que solo se compila con `cargo test`. Como `tests` es un módulo hijo, puede acceder a elementos privados del módulo padre (`use super::*`).

### 15.2 Constructores en `testing.rs`

```rust
let mut t = task("1");                 // tarea válida con valores por defecto
t.priority = Priority::Urgent;         // solo cambia lo que importa al test
```

Cada test muestra **solo los datos relevantes** para lo que verifica. Si `Task` gana un campo nuevo, se actualiza un solo lugar (`testing::task`) y no 30 tests.

### 15.3 Aislar una regla: `only(...)`

```rust
let policy = only(|p| p.pending_comment_weight = 15);
```

`only` crea una política con **todos los pesos en cero** salvo el que se configura en el closure. Así un test de comentarios no se rompe si mañana cambia el peso de la prioridad. El parámetro es `impl FnOnce(&mut WeightedPolicy)`: una función que recibe la política para modificarla y se llama una sola vez.

### 15.4 Fechas fijas

Todos los tests usan `const NOW: &str = "2026-10-10T15:00:00Z";`. Un test que usa la hora real puede pasar hoy y fallar mañana. Por eso `now` es un parámetro, y por eso en la fase 2 existe el puerto `Clock`.

---

## 16. Decisiones que surgieron al implementar

Se agregaron a `ARCHITECTURE.md` §9.5:

| Decisión | Por qué |
| --- | --- |
| `ScoringContext.offset` | "Hoy" es el día local del usuario, no el día UTC |
| Bonos de vencimiento excluyentes (`Urgency`) | Una tarea vencida no debería sumar también "vence hoy" y "esta semana" |
| Empates entre proyectos: gana el primero definido | Resultado determinista y fácil de explicar al usuario |
| Orden estable en `rank` | Con puntajes iguales, se respeta el orden del proveedor |

---

## 17. Autoevaluación

1. ¿Qué pasaría con la regla "el dominio no depende de `reqwest`" si `domain` fuera un módulo dentro del mismo crate que el adaptador de ClickUp?
2. ¿Por qué `TaskId` tiene el campo privado, pero `Task` tiene todos sus campos públicos?
3. Si cambias `Vec<&'t Task>` por `Vec<Task>` en `ProjectResolver::group`, ¿qué tendrías que agregar dentro de la función y qué costo tiene?
4. ¿Qué test falla si quitas `.rev()` en `ProjectResolver::resolve`? Pruébalo.
5. ¿Por qué `rank` recibe `&mut [Task]` y no `Vec<Task>`? ¿Y por qué no devuelve un `Vec` nuevo?
6. Agrega una variante `Priority::Critical` sin tocar nada más y ejecuta `cargo check -p domain`. ¿Qué te obliga a cambiar el compilador?
