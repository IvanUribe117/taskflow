# Fase 2: lo que te toca implementar

Las firmas, los fakes y los tests ya están escritos. Tu trabajo es reemplazar cada `todo!()` hasta que pase:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Ahora mismo **19 tests fallan a propósito** con `not yet implemented`. Están en este orden de dificultad; conviene hacerlos así, porque cada paso usa el anterior.

| # | Archivo                                             | Qué implementar                               | Tests que lo verifican                     | Conceptos                                                              |
| - | --------------------------------------------------- | ---------------------------------------------- | ------------------------------------------ | ---------------------------------------------------------------------- |
| 1 | `crates/domain/src/task/status.rs`                | `impl Display for Status` y `for Priority` | `cargo test -p domain display`           | Trait`Display`, `match`, `Formatter::write_str`                  |
| 2 | `crates/application/src/ports/clock.rs`           | `SystemClock::now` y `offset`              | `cargo test -p application clock`        | Implementar un trait,`chrono::Local`                                 |
| 3 | `crates/application/src/dto.rs`                   | `TaskView::from_task`                        | `cargo test -p application task_view`    | Préstamos (`&Task`), `to_string()`, `to_owned()` vs `clone()` |
| 4 | `crates/application/src/dto.rs`                   | `impl From<&Project> for ProjectView`        | `cargo test -p application project_view` | Trait`From`, `match` sobre `ProjectKind`                         |
| 5 | `crates/application/src/dto.rs`                   | `TaskGroup::flat` y `for_project`          | `cargo test -p application group`        | Iteradores (`iter().map().collect()`), `Option::map`               |
| 6 | `crates/application/src/tasks/get_today_tasks.rs` | `GetTodayTasks::execute`                     | `cargo test -p application get_today`    | `async`/`.await`, `?`, `try_join_all`, `Arc<dyn Trait>`      |

Al terminar, **borra las líneas `#![allow(...)]`** marcadas con `TODO(fase 2)` en `crates/domain/src/lib.rs` y `crates/application/src/lib.rs`. Están ahí solo para que el esqueleto compile sin avisos mientras hay `todo!()`.

## Pistas por paso

Intenta primero sin leerlas.

<details>
<summary>1. Display</summary>

- `Display` se implementa una vez y `to_string()` viene gratis (por el trait `ToString`).
- `f.write_str("to do")` escribe un texto fijo. Para `Other(name)` también sirve `f.write_str(name)`.
- `write!(f, "...")` existe, pero para textos fijos `write_str` es más directo.

</details>

<details>
<summary>2. SystemClock</summary>

- `Utc::now()` ya está disponible en `application`, que activa la feature `clock` de chrono (en `domain` no lo está, a propósito).
- Para el desfase local: `chrono::Local::now()` devuelve un `DateTime<Local>`; mira qué devuelve su método `.offset()` y cómo obtener un `FixedOffset` a partir de él.

</details>

<details>
<summary>3. TaskView::from_task</summary>

- La tarea ya sabe responder `is_overdue(now)` y `pending_comments_for(me)`. No reimplementes esas reglas: el DTO solo traduce.
- `status` y `priority` usan el `Display` del paso 1.
- Recibes `&Task`, no `Task`: para los `String` necesitas copiar (`clone()` o `to_owned()`). ¿Por qué no puedes moverlos?

</details>

<details>
<summary>5. Grupos</summary>

- `for_project` recibe `&[&Task]` (un slice de referencias), mientras que `flat` recibe `&[Task]`. Fíjate en qué recibe el closure de `.map(...)` en cada caso.
- `ProjectResolver::get(slug)` ya existe y devuelve `Option<&Project>`. Combínalo con `ProjectView::from`.

</details>

<details>
<summary>6. GetTodayTasks::execute</summary>

- Los pasos están en el comentario `///` del método y en `ARCHITECTURE.md` §11.2.
- Los tests fijan el orden importante: los comentarios se piden **solo para tareas abiertas** y **antes** de ordenar (`pending_comments_count_before_ranking`).
- "En paralelo": `futures::future::try_join_all` recibe un iterador de futures y espera a todos; si uno falla, devuelve el primer error.
- `ScoringContext` necesita `now` **y** `offset`: ambos salen de `self.ctx.clock`.
- El `?` convierte `ProviderError` y `RepoError` en `AppError` solo, gracias a `#[from]` en `crates/application/src/error.rs`.

</details>

## Qué observar mientras implementas

- **Nada de esto toca la red.** `FakeTaskProvider` (en `crates/application/src/testing.rs`) responde con datos en memoria y registra las llamadas. Así se cumple NFR-09.
- **El caso de uso no sabe que existe ClickUp.** Solo ve `Arc<dyn TaskProvider>`. En la fase 3, el adaptador real se enchufa en el mismo lugar sin cambiar `execute`.
- **Los DTOs no exponen el dominio.** La interfaz recibirá `String` y `bool`, no `Status` ni `Task`.

---

# Conceptos aplicados

Explicaciones que surgieron mientras implementabas cada paso, ordenadas por paso. Cada una responde una duda concreta.

## Paso 1: `Display` para `Status` y `Priority`

### Implementar `Display` y obtener `to_string()` gratis

```rust
impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Urgent => "urgent",
            // ...
        })
    }
}
```

La librería estándar tiene una implementación genérica: *todo tipo que implementa `Display` implementa `ToString`*. Por eso, al escribir `Display`, ya puedes llamar a `.to_string()` y usar el tipo en `format!("{}", x)` y `println!`.

- **`f.write_str(texto)`:** escribe un texto fijo. Es lo más directo para enums sin datos.
- **`write!(f, "...{}", x)`:** sirve cuando hay que formatear valores.
- **`Self::Urgent` en lugar de `Priority::Urgent`:** dentro de un `impl`, `Self` es el tipo que se implementa. Si renombras el tipo, no hay que tocar el cuerpo.

### ¿Puede el enum "traer" su texto, como en otros lenguajes?

No de forma nativa. En otros lenguajes existe `enum Status { Todo = "to do" }`, pero en Rust un enum solo puede llevar **números** como valor asociado. Hay cuatro formas de acercarse:

1. **Discriminantes numéricos** (solo en enums sin datos):

   ```rust
   enum Priority { Urgent = 4, High = 3, Normal = 2, Low = 1, None = 0 }
   let v = Priority::High as i64; // 3
   ```

   `Status` no puede, porque `Other(String)` lleva datos. En `Priority` se prefirió `value()` con `match` porque el enum también usa `derive(Ord)`: el orden por declaración y el valor numérico son dos ideas distintas que conviene no mezclar.

2. **Método `as_str()` + `Display` que lo usa:** el `match` vive en un solo lugar y devuelve `&str` sin crear un `String`. Es lo más común en código profesional cuando el texto se usa en varios sitios.

   ```rust
   impl Status {
       pub fn as_str(&self) -> &str { match self { Self::Todo => "to do", /* ... */ Self::Other(s) => s } }
   }
   ```

3. **Macro que genera el `match`: crate `strum`.** El texto queda pegado a cada variante:

   ```rust
   #[derive(strum::Display)]
   enum Priority {
       #[strum(serialize = "urgent")] Urgent,
       // ...
   }
   ```

   También puede generar el camino inverso (texto → enum, `EnumString`). El costo: una dependencia más y código generado que no ves.

4. **`serde` con `#[serde(rename = "to do")]`:** pega un texto a cada variante, pero **solo para serializar** (JSON, TOML). No afecta a `Display`. Lo usarás en DTOs y configuración.

Escribir el `match` a mano fue intencional: es el ejercicio de implementar un trait estándar. Para el proyecto real, las opciones 2 y 3 son razonables. Queda como decisión para cuando aparezca `status_map` en la configuración, que es donde más se convertirá entre texto y enum.

## Paso 2: `SystemClock`

```rust
fn now(&self) -> DateTime<Utc> { Utc::now() }
fn offset(&self) -> FixedOffset { *chrono::Local::now().offset() }
```

- **`Utc::now()` existe aquí y no en `domain`:** `application` activa la feature `clock` de chrono; `domain` no, a propósito. El dominio recibe la hora como parámetro y nunca la lee del sistema (ver `fase-1.md`, §2.6).
- **`Local::now().offset()` devuelve `&FixedOffset`:** una referencia al desfase guardado dentro del `DateTime<Local>`. Para devolver un `FixedOffset` con dueño hay que copiarlo:
  - `*... .offset()`: desreferencia. Funciona porque `FixedOffset` es `Copy` (son unos pocos bytes).
  - `.offset().to_owned()`: crea una copia propia. Mismo resultado.
- **Por qué `Clock` es un trait:** en producción se usa `SystemClock`; en los tests, `FixedClock`, que siempre devuelve `2026-10-10T15:00:00Z`. Así los tests no dependen del día en que se ejecutan.

## Paso 3: `TaskView::from_task` y los préstamos

### El error E0507: "cannot move out of a shared reference"

```rust
pub fn from_task(task: &Task, ...) -> Self {
    Self { title: task.title, ... }   // ❌ E0507
}
```

1. **`&Task` es un préstamo:** puedes **mirar** la tarea, pero no **llevarte** nada. Al terminar la función, el dueño la sigue usando.
2. **`title: task.title` es un *move*:** el `String` cambiaría de dueño y la tarea quedaría con un `title` inválido. El compilador lo impide.
3. **Solución:** crear una copia propia.

| Campo | Tipo | ¿Funciona directo desde `&Task`? | Por qué |
| --- | --- | --- | --- |
| `due: task.due` | `Option<DateTime<Utc>>` | Sí | `DateTime` es `Copy`: se copia bit a bit |
| `overdue: task.is_overdue(now)` | `bool` | Sí | El método solo lee y devuelve un valor nuevo |
| `status: task.status.to_string()` | `String` | Sí | `to_string()` **crea** un `String` nuevo |
| `title: task.title` | `String` | **No** | Intenta mover algo prestado |

**Regla:** desde un `&T`, los campos `Copy` se toman directamente; los que no son `Copy` (`String`, `Vec`...) se copian.

### `clone()` vs `to_owned()` vs `to_string()`

Las tres reservan memoria y copian los bytes: cuestan lo mismo. La diferencia es **qué le comunican a quien lee el código**:

| Método | Qué dice | Cuándo usarlo |
| --- | --- | --- |
| `clone()` | "Duplico este valor tal cual" | Mismo tipo de origen y destino (`String` → `String`) |
| `to_owned()` | "Paso de prestado a propio" | De tipo prestado a su versión con dueño (`&str` → `String`) |
| `to_string()` | "Convierto esto en texto" | El origen no es texto, o se usa su `Display` (`Status` → `String`) |

Aplicado a `from_task`:

```rust
title: task.title.clone(),              // String → String
id: task.id.as_str().to_owned(),        // &str → String
status: task.status.to_string(),        // enum → texto con Display
```

Clippy pedantic **exige** `clone()` para `String` → `String`: `to_string()` ahí dispara el lint `implicit_clone`, porque sugiere una conversión que no existe.

### `.count()` ya devuelve `usize`

```rust
pending_comments: task.pending_comments_for(me).count(),   // sin `as usize`
```

`pending_comments_for` toma prestados `task` y `me`, recorre los comentarios sin copiarlos, y `count()` devuelve un número nuevo que sí es tuyo.

### ¿Se puede evitar el `clone()`?

Sí, pero en este caso no conviene:

| Opción | Cómo | Copias | Problema aquí |
| --- | --- | --- | --- |
| `clone()` | Lo que se hizo | Sí, pequeñas | Ninguno |
| **A. Lifetimes** | `TaskView<'a> { title: &'a str }` | No | El `'a` se contagia a `TaskGroup<'a>` y `TodayReport<'a>`. `execute` no puede devolver el reporte porque las tareas son una variable local: `E0515: cannot return value referencing local variable` |
| **B. Mover** | `from_task(task: Task)` | No | Quien llama pierde la tarea; `group()` trabaja con `&Task`, así que habría que reorganizar el agrupado |
| **C. `Arc<str>`** | `Task { title: Arc<str> }`; clonar solo incrementa un contador | Solo el contador | Cambia todo el dominio para ahorrar microsegundos |

**Regla mental:**
- Lo que **cruza una frontera** (DTOs, respuestas, lo que se guarda) → con dueño, se copia.
- Lo que **se usa en el momento** (iteradores, agrupaciones temporales, cálculos) → prestado, con lifetimes. Ya lo usas así en `pending_comments_for` y en `ProjectResolver::group`.

Copiar unos textos por tarea (hasta 50 tareas, NFR-06) cuesta microsegundos. Las copias se evitan en bucles intensivos o con datos grandes.

### `#[must_use]`

```rust
#[must_use]
pub fn from_task(...) -> Self
```

Le dice al compilador: **si alguien llama a esto y descarta el resultado, avisa**.

```rust
TaskView::from_task(&t, &me, now);   // ⚠ unused return value ... that must be used
```

- **Por qué:** si una función solo calcula y devuelve algo, ignorar el resultado es casi seguro un error.
- **Ya lo usas sin saberlo:** `Result` es `#[must_use]` en la librería estándar; por eso ignorar un error da aviso.
- **En los builders de prueba** (`FakeTaskProvider::with_tasks`) evita un error típico: escribir `provider.with_tasks(v);` sin reasignar pierde la configuración.
- **De dónde sale:** `clippy::pedantic` (lint `must_use_candidate`) lo sugiere en funciones públicas que devuelven un valor sin efectos secundarios.
- **Variante con mensaje:** `#[must_use = "this only builds the view; it does not print anything"]`.

**Regla:** ponlo en funciones que *calculan*; no en las que *hacen* algo (escribir, enviar, guardar) y devuelven un dato secundario.

## Paso 4: `From<&Project> for ProjectView`

### El trait `From`

```rust
impl From<&Project> for ProjectView {
    fn from(project: &Project) -> Self { ... }
}
```

- Crea la función `ProjectView::from(&project)`, que se puede pasar por nombre a `map` (paso 5).
- **`Into` gratis:** implementar `From<A> for B` hace que `a.into()` produzca un `B`.
- Es la forma estándar de expresar "este tipo se construye a partir de aquel".

### `Display` para `ProjectKind`: ¿en el dominio o en el DTO?

Agregaste `impl Display for ProjectKind` en `domain`. Es válido y coherente con `Status` y `Priority`:

| Opción | Ventaja | Desventaja |
| --- | --- | --- |
| `Display` en `domain` (lo que hiciste) | Un solo lugar; toda interfaz obtiene el mismo texto | El dominio decide un texto que se muestra |
| `match` en `ProjectView::from` | La presentación queda en `application` | Si otra parte necesita el texto, se repite el `match` |

Para `ProjectKind` es buena elección porque `"client"` e `"internal"` son el **nombre canónico** del valor: el mismo que el usuario escribe en `project.toml` (`kind = "client"`, `ARCHITECTURE.md` §14.1). Por eso el test exige minúsculas: así el JSON de salida y la configuración hablan el mismo idioma. Si el texto fuera puramente visual (por ejemplo `"🏢 Cliente"`), iría en la interfaz.

## Paso 5: `TaskGroup::flat` y `for_project`

### Qué significan los nombres

Son **constructores**: funciones asociadas (sin `self`) que devuelven un `TaskGroup`. Cada uno corresponde a un valor de `GroupBy`:

- **`flat`** ("plano"): todas las tareas en **un único grupo** con `project: None`. Es la vista sin agrupar de `taskflow today`.
- **`for_project`** ("para un proyecto"): el grupo de **un** proyecto concreto. Se llama una vez por cada grupo que devuelve `ProjectResolver::group`.

```text
taskflow today --by-project
ACME Corp        ← for_project(ProjectKey::Project("acme"), ...)
Interno          ← for_project(ProjectKey::Project("interno"), ...)
Sin proyecto     ← for_project(ProjectKey::Unassigned, ...)
```

En Rust no hay sobrecarga de constructores; se usan nombres descriptivos. Las convenciones más comunes son `new` (principal), `from_x` / `From` (a partir de otro tipo) y `with_x` (variante con una opción). `flat` y `for_project` no son convenciones estándar, sino nombres que describen qué grupo construyen.

### Construir el campo `project` en `for_project`

**El problema:** la clave solo trae el slug (`"acme"`), pero `ProjectView` necesita `slug`, `name` **y** `kind`. Esos datos no están en la clave; están en `projects`. Por eso `for_project` recibe el `ProjectResolver`: no para resolver tareas, sino **como una agenda** donde buscar el proyecto completo por su slug.

| `key` que llega | `project` que hay que devolver |
| --- | --- |
| `ProjectKey::Project("acme")` y "acme" existe | `Some(ProjectView { slug: "acme", name: "ACME Corp", kind: "client" })` |
| `ProjectKey::Project("zeta")` y "zeta" no existe | `None` |
| `ProjectKey::Unassigned` | `None` |

```rust
let project = match key {
    ProjectKey::Project(slug) => projects.get(slug).map(ProjectView::from),
    ProjectKey::Unassigned => None,
};
```

1. **`match key`:** separa los dos tipos de clave.
2. **`projects.get(slug)`:** busca en la agenda y devuelve `Option<&Project>`, porque el slug podría no existir.
3. **`.map(ProjectView::from)`:** convierte el proyecto en vista **solo si se encontró**.

```text
Some(&Project acme)  ──.map(ProjectView::from)──►  Some(ProjectView acme)
None                 ──.map(ProjectView::from)──►  None              (no llama a la función)
```

**Pasar una función por su nombre:** `.map(ProjectView::from)` equivale a `.map(|p| ProjectView::from(p))`. La forma corta es la idiomática y la que prefiere clippy.

**Un `match` legible le gana a una cadena larga.** Se puede escribir en una sola expresión, pero dos brazos claros se entienden mejor. En Rust, la legibilidad le gana a la brevedad.

### `&&Task`: tres formas de quitar una capa de referencia

`for_project` recibe `tasks: &[&Task]`: un slice prestado de referencias. `.iter()` produce **referencias a los elementos**, y como cada elemento ya es `&Task`, cada vuelta entrega `&&Task`: un puntero a un puntero.

```text
slice:  [ ref0 , ref1 , ref2 ]     cada refN es un &Task
           │      │      │
           ▼      ▼      ▼
        Task0  Task1  Task2
```

| Forma | Código | Quién quita la capa |
| --- | --- | --- |
| Patrón en el parámetro | `.map(\|&task\| TaskView::from_task(task, me, now))` | El patrón `&task` *desarma* la referencia |
| Desreferencia explícita | `.map(\|task\| TaskView::from_task(*task, me, now))` | Tú, con `*`, al usar el valor |
| Auto-deref | `.map(\|task\| TaskView::from_task(task, me, now))` | El compilador (*deref coercion*): ve que la función espera `&Task` e inserta el `*` |
| Adaptador | `.iter().copied().map(\|task\| ...)` | `.copied()` convierte `&&Task` en `&Task` antes del `map` |

- **En los patrones, `&` significa "desempaquetar"**, al revés que en las expresiones. Es lo mismo que `let &x = &5;` → `x` vale `5`.
- **Las formas `|&x|` y `*x` copian lo que hay detrás**, así que solo funcionan si es `Copy`:

| Lo que hay detrás | `\|&x\|` o `*x` | Por qué |
| --- | --- | --- |
| `&Task`, `i32`, `bool`, `Priority` | ✅ | Son `Copy` (copiar una referencia compartida es copiar un puntero) |
| `String`, `Task` | ❌ E0507 | Copiarlos sería mover algo prestado, igual que en el paso 3 |

**Regla práctica:** con `&&T` o valores `Copy` pequeños, `|&x|` es idiomático. Si dudas, pasa la variable tal cual y deja que la auto-deref lo resuelva.

## Paso 6: `GetTodayTasks::execute`

### `iter()` vs `iter_mut()` vs `into_iter()`

| Método | La colección... | Cada elemento es | Úsalo cuando |
| --- | --- | --- | --- |
| `iter()` | se presta (sigue siendo tuya) | `&T` | solo vas a leer |
| `iter_mut()` | se presta para modificar | `&mut T` | vas a modificar elementos en su lugar |
| `into_iter()` | **se consume** (deja de existir) | `T` (con dueño) | ya no necesitas la colección y quieres los elementos |

En `execute` se usan las tres, cada una por una razón:

```rust
// iter(): solo se lee el id; `tasks` debe seguir existiendo.
try_join_all(tasks.iter().map(|task| self.ctx.tasks.comments(&task.id)))

// iter_mut() en `tasks`: se modifica cada tarea en su lugar.
// zip(comments) llama a into_iter() implícitamente: cada `comment` es un Vec<Comment>
// con dueño, así que `task.comments = comment` lo MUEVE sin clonar.
for (task, comment) in tasks.iter_mut().zip(comments) {
    task.comments = comment;
}

// into_iter(): el IndexMap de `group` es temporal; consumirlo entrega `key` y `group`
// con dueño. Por eso se pasa `&key` a `for_project`.
projects.group(&tasks).into_iter().map(|(key, group)| ...)
```

**Dos detalles que confunden:**
- **`for x in coleccion`** es azúcar para `coleccion.into_iter()`: consume la colección. `for x in &coleccion` recorre prestado (equivale a `.iter()`).
- **`into_iter()` sobre una referencia** (un `&[T]` o `&Vec<T>`) no puede consumir lo que no es tuyo, así que se comporta como `iter()`. Clippy lo marca (`into_iter_on_ref`) y pide escribir `.iter()`.

### `&Vec<T>` se convierte solo en `&[T]`

`group()` devuelve `Vec<&Task>` y `for_project` pide `&[&Task]`. No hay que escoger uno ni transformar a mano: **`&Vec<T>` se convierte automáticamente en `&[T]`** (deref coercion, porque `Vec<T>` se desreferencia a `[T]`). Basta con pasar `&group`.

**Por qué las firmas piden slices:** `&[T]` acepta un `Vec`, un array o un pedazo de otro vector; `&Vec<T>` solo acepta `Vec`. Clippy marca `&Vec<T>` en parámetros (lint `ptr_arg`).

**Regla:** en parámetros, pide `&[T]` y `&str`, no `&Vec<T>` ni `&String`.

> **Error típico:** pasar `tasks` (todas las tareas, `Vec<Task>`) en lugar de `&group` (las de ese proyecto). El compilador lo detecta como `expected &[&Task], found Vec<Task>`.

### Mover vs clonar al construir el query

```rust
.search_tasks(&TaskQuery { assignee: Some(me.clone()), ..TaskQuery::default() })
```

`Some(me)` **movería** `me` dentro del `TaskQuery`, y después ya no podrías usar `&me` en `ScoringContext` ni en los grupos (`borrow of moved value: me`). `me.clone()` le da al query su propia copia y tú conservas la original.

### Movimientos parciales: `if let Some(slug) = query.project`

`query` es tuyo (se recibe por valor), así que `if let Some(slug) = query.project` **mueve** el slug fuera del struct. Después todavía puedes usar `query.limit` y `query.group_by`, porque Rust sigue la pista de cada campo por separado: solo `query.project` quedó movido. Si quisieras volver a usar `query` completo, tendrías que usar `&query.project`.

### Inicialización abreviada de campos

```rust
ScoringContext { now, offset, me: &me, projects: &projects }
```

Cuando la variable se llama igual que el campo, `offset: offset` se abrevia a `offset`.

### `try_join_all`: concurrencia, no hilos

**No hay hilos aquí.** Es concurrencia dentro de un mismo hilo.

1. **Un future no hace nada hasta que se espera.** Llamar a una función `async` solo crea una "receta" de trabajo pendiente:

   ```rust
   let fut = self.ctx.tasks.comments(&task.id);   // ← ninguna petición todavía
   let comments = fut.await;                       // ← recién aquí se ejecuta
   ```

2. **`map` crea N futures sin ejecutarlos.**

3. **`try_join_all` los ejecuta a la vez y espera a todos.** Mientras uno espera la red, avanza los demás. Devuelve un `Vec` **en el mismo orden de entrada**, y por eso funciona el `zip` con `tasks`.

```text
Secuencial (un .await por tarea dentro de un for):
  tarea1 ████████
  tarea2         ████████
  tarea3                 ████████          total ≈ suma de todas

try_join_all:
  tarea1 ████████
  tarea2 ██████
  tarea3 █████████                          total ≈ la más lenta
```

Con 50 tareas y unos 200 ms por petición: unos 10 s en secuencia frente a menos de 1 s concurrente. Esa es la razón de NFR-06.

- **El `try_`:** si **un** future devuelve `Err`, corta, cancela los demás y devuelve ese error. Por eso termina en `?`. Existe también `join_all`, que espera a todos y te da `Vec<Result<...>>` para decidir qué hacer con cada fallo.
- **¿Y los hilos?** Los gestiona tokio, el runtime (`#[tokio::main]`, `#[tokio::test]`). Tokio hace que un `.await` que espera la red libere el hilo para otro trabajo. `try_join_all` reparte el tiempo de **un** hilo; para peticiones de red es ideal, porque el tiempo se va esperando, no calculando.
- **Pendiente para la fase 3:** `try_join_all` lanza todas las peticiones a la vez. Con 50 tareas son 50 peticiones simultáneas a ClickUp, que tiene límite (NFR-07). Lo resolverá el adaptador con un `Semaphore`, sin cambiar `execute`.

### `?` y `#[from]`

```rust
let me = self.ctx.tasks.current_user().await?;     // ProviderError → AppError
let projects = self.ctx.projects.list().await?;     // RepoError → AppError
```

`AppError` declara `Provider(#[from] ProviderError)` y `Repo(#[from] RepoError)`. `thiserror` genera `impl From<ProviderError> for AppError`, y `?` llama a esa conversión cuando el error no es del tipo que devuelve la función. Sin `#[from]` tendrías que escribir `.map_err(AppError::Provider)?` en cada llamada.

### `Arc<dyn TaskProvider>`: el caso de uso no sabe quién responde

`self.ctx.tasks` es un `Arc<dyn TaskProvider>`. En los tests apunta a `FakeTaskProvider`; en la fase 3 apuntará al adaptador de ClickUp. `execute` no cambia: llama a los métodos del trait y Rust decide en tiempo de ejecución qué implementación usar (*dispatch dinámico*).

### Corrección al documento de arquitectura

El ejemplo de `GetTodayTasks` en `ARCHITECTURE.md` §11.2 construía `ScoringContext` sin `offset`, aunque §9.5 ya lo exigía desde la fase 1. Ya está corregido; la implementación correcta es la que pide el compilador: `offset` sale de `self.ctx.clock.offset()`.

---

Cuando termines, avísame y reviso tu implementación: corrección, idiomática y lo que haría diferente un proyecto profesional.
