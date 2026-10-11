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

Cuando termines, avísame y reviso tu implementación: corrección, idiomática y lo que haría diferente un proyecto profesional.
