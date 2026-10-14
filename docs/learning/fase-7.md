# Fase 7: cerrar tareas (escritura en ClickUp)

Objetivo del roadmap (§24): *"Cerrar una tarea sin abrir ClickUp"*.

Hasta ahora Taskflow solo **leía** de ClickUp. En esta fase **escribe** por primera vez: cambia el estado de una tarea y publica un comentario.

```sh
taskflow close 86b2x1abc
#   Estado final para "Fix invoice rounding"
#   > to do (abierto)
#     in progress (en curso)
#     Code Review (en curso)
#     complete (terminado) · sugerido
#   Comentario de cierre (opcional): Corregido el redondeo por línea; incluye test.
#   Fix invoice rounding (86b2x1abc)
#   Estado: in progress → Code Review
#   Comentario: Corregido el redondeo por línea; incluye test.
#   ¿Confirmar cierre? [y/N] y
#   ✔ 86b2x1abc → Code Review · comentario publicado

taskflow close 86b2x1abc --status "code review" --comment "Listo" --yes   # sin preguntas
```

Hay **30 tests nuevos**: 28 fallan a propósito y 2 (los argumentos de `close`) ya pasan. Los comprobé con una implementación de referencia que después borré. Los tests de las fases anteriores siguen pasando.

**La sección "Especificación por paso" dice exactamente qué hacer en cada paso:** entradas, salidas, orden, condiciones y errores.

---

## Antes de empezar: las reglas de la escritura

Escribir en el sistema de otra persona (o del equipo) tiene reglas que leer no tenía:

1. **Confirmar antes de escribir (FR-12).** `close` muestra un resumen y pregunta. Por defecto la respuesta es **no**. `--yes` existe para scripts, cuando quien lo usa ya decidió.
2. **Validar antes de escribir.** Si el estado no existe, no se publica el comentario. Todas las comprobaciones van **antes** de la primera escritura.
3. **Los estados no se inventan.** Cada lista de ClickUp tiene sus propios estados, y cambian (ADR-023): `Code Review` en una, `Ready For Testing` en otra. Por eso se **piden a ClickUp** cada vez (`available_statuses`), y el usuario elige entre los que existen.
4. **La confirmación es de la interfaz, no de `application` (UC-04).** Los casos de uso no preguntan nada: reciben lo ya decidido. Así la app de escritorio podrá confirmar con un botón en vez de con `dialoguer`.

### El flujo completo

```text
CLI                                 application                         ClickUp
───                                 ───────────                         ───────
close 86b2x1abc
  │
  ├─ PrepareClose ────────────────► task + available_statuses ───────► GET /task/{id}
  │                                  suggest_final(...)                 GET /list/{list_id}
  │  ◄──── CloseOptions (estados y sugerido)
  │
  ├─ Select estado (o --status)
  ├─ Input comentario (o --comment)
  ├─ resumen + Confirm   ← si dice que no, termina aquí sin escribir nada
  │
  └─ CloseTask ───────────────────► validar estado (find_status)
                                     add_comment (si hay) ─────────────► POST /task/{id}/comment
                                     set_status ───────────────────────► PUT /task/{id}
     ◄──── CloseResult
```

---

## Mapa de cambios

| Capa        | Archivo                                    | Qué hay                                                                     |
| ----------- | ------------------------------------------ | ---------------------------------------------------------------------------- |
| Dominio     | `domain/src/task/status.rs`              | `StatusKind`, `StatusOption`, `find_status`, `suggest_final`         |
| Aplicación | `application/src/ports/task_provider.rs` | `available_statuses`, `set_status`, `add_comment` (ya hecho)           |
|             | `application/src/dto.rs`                 | `StatusOptionView`, `CloseOptions`, `CloseResult`                      |
|             | `application/src/tasks/close_task.rs`    | `PrepareClose`, `CloseTask`, `CloseCommand`                            |
| ClickUp     | `clickup/src/client.rs`                  | `send` (con cualquier método), `put`, `post`                          |
|             | `clickup/src/mapper.rs`                  | `status_option`                                                            |
|             | `clickup/src/provider.rs`                | Los tres métodos nuevos                                                     |
| CLI         | `cli/src/commands/close.rs`              | `status_choice`, `run`                                                   |
|             | `cli/src/presenter.rs`                   | `status_kind_label`, `status_label`, `close_summary`, `close_result` |

**Lo que ya hice yo:** los métodos del puerto, el fake (`FakeTaskProvider::with_statuses` y `writes()`, que registra cada escritura en orden), los argumentos de `close`, el despacho en `commands/mod.rs`, los envoltorios `get`/`put`/`post` del cliente y el fixture `list.json`.

---

## Pasos

| #           | Archivo                                 | Qué implementar                                        | Verificar con                                     |
| ----------- | --------------------------------------- | ------------------------------------------------------- | ------------------------------------------------- |
| **A** |                                         | **Dominio**                                       |                                                   |
| 1           | `domain/src/task/status.rs`           | `StatusKind::is_final` y su `Display`               | `cargo test -p domain status`                   |
| 2           | (mismo)                                 | `find_status`                                         | (mismo)                                           |
| 3           | (mismo)                                 | `suggest_final`                                       | (mismo)                                           |
| **B** |                                         | **Aplicación**                                   |                                                   |
| 4           | `application/src/dto.rs`              | `From<&StatusOption> for StatusOptionView`            | `cargo test -p application --lib dto`           |
| 5           | `application/src/tasks/close_task.rs` | `PrepareClose::execute`                               | `cargo test -p application --lib close_task`    |
| 6           | (mismo)                                 | `CloseTask::execute`                                  | (mismo)                                           |
| **C** |                                         | **ClickUp**                                       |                                                   |
| 7           | `clickup/src/client.rs`               | Que `send` use `method` y `body`                  | `cargo test -p adapter-clickup --test provider` |
| 8           | `clickup/src/mapper.rs`               | `status_option`                                       | `cargo test -p adapter-clickup --lib mapper`    |
| 9           | `clickup/src/provider.rs`             | `available_statuses`, `set_status`, `add_comment` | `cargo test -p adapter-clickup --test provider` |
| **D** |                                         | **CLI**                                           |                                                   |
| 10          | `cli/src/commands/close.rs`           | `status_choice`                                       | `cargo test -p taskflow close`                  |
| 11          | `cli/src/presenter.rs`                | Las cuatro funciones de cierre                          | `cargo test -p taskflow presenter`              |
| 12          | `cli/src/commands/close.rs`           | `run`                                                 | A mano (paso 13)                                  |
| 13          | —                                      | Probar contra tu ClickUp real                           | Ver "Paso 13"                                     |

Al terminar, **borra el bloque `#![allow(...)]` con `TODO(fase 7)`** en:

- `crates/domain/src/lib.rs`
- `crates/application/src/lib.rs`
- `crates/adapters/clickup/src/lib.rs`
- `apps/cli/src/main.rs`

Y también el comentario `// TODO(fase 7)` dentro de `send`, en `client.rs`.

```sh
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

---

## Conceptos nuevos

### `StatusKind` no es `Status`

|                  | `Status` (fase 1)                                                          | `StatusKind` (nuevo)                                          |
| ---------------- | ---------------------------------------------------------------------------- | --------------------------------------------------------------- |
| Quién lo decide | Taskflow, con `provider.status_map`                                        | El proveedor (`type` en ClickUp)                              |
| Valores          | `Todo`, `Ready`, `InProgress`, `InReview`, `Done`, `Other(name)` | `Open`, `Custom`, `Done`, `Closed`                      |
| Para qué        | Priorizar y agrupar                                                          | Saber qué estados **cierran** una tarea, para sugerirlos |

Un mismo estado de ClickUp, "Code Review", es `Status::InReview` para Taskflow (si lo configuraste) y `StatusKind::Custom` para ClickUp.

### Parsear estricto o tolerante

|                   | Prioridad (fase 3)                                           | `type` de un estado (esta fase)                                                                        |
| ----------------- | ------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------- |
| Valor desconocido | **Error** (`UnknownPriority`)                        | **`Custom`**                                                                                     |
| Por qué          | Una prioridad mal leída cambia el orden de todas tus tareas | Un tipo nuevo de ClickUp no debe impedir cerrar tareas. Lo peor que pasa es que ese estado no se sugiera |

No hay una regla única: depende de qué cuesta equivocarse.

### Métodos HTTP y cuerpo JSON

| Método  | Para qué                          | En esta fase                                         |
| -------- | ---------------------------------- | ---------------------------------------------------- |
| `GET`  | Leer                               | Tarea y lista                                        |
| `PUT`  | Reemplazar o actualizar un recurso | `PUT /task/{id}` con `{"status": "Code Review"}` |
| `POST` | Crear algo nuevo                   | `POST /task/{id}/comment` crea un comentario       |

Con `reqwest`, el cuerpo JSON se agrega al builder con `.json(&valor)`: serializa el valor y pone el header `Content-Type: application/json`.

### `json!`: JSON sin crear un struct

```rust
let body = serde_json::json!({ "status": status });   // serde_json::Value
```

Para un cuerpo de dos campos que solo se envía, un struct con `#[derive(Serialize)]` sería ceremonia. La macro `json!` construye un `serde_json::Value` con sintaxis de JSON y acepta variables de Rust (`status`) como valores.

### `IgnoredAny`: aceptar cualquier respuesta y descartarla

`PUT /task/{id}` responde con la tarea completa (unos 40 campos) y no la necesitamos. Pero `client.put` es genérico: hay que decirle **qué tipo** deserializar.

```rust
let _: IgnoredAny = self.client.put(&path, &body).await?;
```

`serde::de::IgnoredAny` acepta **cualquier** JSON válido y no guarda nada. Comprueba que la respuesta fue un éxito y un JSON bien formado, sin definir structs para datos que no usamos.

`let _: IgnoredAny = ...` le da el tipo a la variable, y Rust deduce la `T` de `put` a partir de ahí, como en la fase 3.

### El builder de `reqwest` se consume en cada paso

```rust
let mut request = self.http.request(method.clone(), &url).header(...).query(query);
if let Some(body) = body {
    request = request.json(body);   // `json` toma `request` por valor y devuelve uno nuevo
}
```

Cada método del builder toma `self` **por valor** y devuelve un builder nuevo. Para agregar algo **condicionalmente**, se reasigna la variable: `request = request.json(body)`. No se puede llamar `request.json(body)` y seguir usando el `request` anterior, porque ya se movió.

### `method.clone()` dentro del bucle

El bucle de reintentos construye la petición en cada vuelta. `request(method, ...)` toma el `Method` **por valor**, y `Method` no es `Copy` (puede contener un método personalizado, con texto). Sin `clone()`, la primera vuelta lo movería y la segunda no tendría nada que usar:

```text
error[E0382]: use of moved value: `method`
```

### El orden de las escrituras y los fallos parciales

`CloseTask` publica el comentario **antes** de cambiar el estado: si el cambio dispara notificaciones, el comentario ya está ahí para darles contexto.

Pero son dos peticiones independientes. Si la segunda falla (red, permisos), el comentario queda publicado y el estado no cambia. No hay forma de deshacer un comentario de forma atómica, así que el error lo dice tal cual y volver a ejecutar `close` termina el trabajo. Las validaciones, en cambio, van todas antes de la primera escritura: un estado inválido no publica nada.

### `match` sobre una tupla de tres valores

`status_choice` decide según tres datos a la vez. Un `match` sobre la tupla `(status, yes, suggested)` escribe la tabla de decisión casi literalmente, y el compilador comprueba que no falte ninguna combinación:

```rust
match (status, yes, suggested) {
    (Some(status), _, _) => ...,     // `_` = "cualquier valor"
    (None, false, _) => ...,
    (None, true, Some(suggested)) => ...,
    (None, true, None) => ...,
}
```

Los brazos se revisan **en orden**: el primero gana aunque `yes` sea `true`.

---

## Especificación por paso

### Paso 1: `StatusKind::is_final` y `Display` (`domain/src/task/status.rs`)

**`is_final`:** `true` para `Done` y `Closed`, `false` para `Open` y `Custom`. Una línea con `matches!`:

```rust
matches!(self, Self::Done | Self::Closed)
```

`matches!(valor, patrón)` devuelve `true` si el valor encaja en el patrón. Es un `match` de dos brazos (`patrón => true, _ => false`) en una expresión.

**`Display`:**

| Variante   | Texto        |
| ---------- | ------------ |
| `Open`   | `"open"`   |
| `Custom` | `"custom"` |
| `Done`   | `"done"`   |
| `Closed` | `"closed"` |

Forma compacta: un solo `f.write_str(match self { Self::Open => "open", ... })`.

### Paso 2: `find_status` (mismo archivo)

- **Entrada:** `options: &'a [StatusOption]`, `name: &str`.
- **Salida:** `Option<&'a StatusOption>`, el **primer** estado cuyo `name` coincide **sin distinguir mayúsculas**.
- **Cómo comparar:** `option.name.eq_ignore_ascii_case(name)`, la misma de `StatusMap` en la fase 3.

```rust
options.iter().find(|option| option.name.eq_ignore_ascii_case(name))
```

`iter()` da `&'a StatusOption` y `find` devuelve el elemento encontrado, así que la referencia apunta dentro de `options`: por eso la salida comparte el lifetime `'a`.

| `name`          | Resultado con los estados del test                                       |
| ----------------- | ------------------------------------------------------------------------ |
| `"code review"` | `Some` del estado `"Code Review"` (con el nombre **original**) |
| `"QA"`          | `None`                                                                 |

### Paso 3: `suggest_final` (mismo archivo)

Tres reglas en orden, la primera que encuentre algo gana:

| # | Regla                                                                                                                      | Ejemplo (estados: to do, in progress, Code Review, complete, Closed)                               |
| - | -------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| 1 | El primer nombre de `preferred`, **en el orden de `preferred`**, que exista en `options` (usa `find_status`) | `preferred = ["closed", "complete"]` → `Closed` (porque "closed" va primero en `preferred`) |
| 2 | Si ninguno existe: el **primer** estado de `options` con `kind.is_final()`                                       | `preferred = ["shipped"]` → `complete`                                                        |
| 3 | Si tampoco: `None`                                                                                                       | Solo `to do` → `None`                                                                         |

```rust
preferred
    .iter()
    .find_map(|name| find_status(options, name))                // regla 1
    .or_else(|| options.iter().find(|option| option.kind.is_final()))   // regla 2
```

- **`find_map`** recorre `preferred`, aplica la función a cada nombre y devuelve el **primer `Some`**. Es `map` + `find` en un paso.
- **`or_else`** se ejecuta solo si lo anterior es `None`. La regla 3 sale sola: si la regla 2 tampoco encuentra nada, el resultado es `None`.

### Paso 4: `StatusOptionView` (`application/src/dto.rs`)

| Campo    | Valor                                                  |
| -------- | ------------------------------------------------------ |
| `name` | `option.name.clone()`                                |
| `kind` | `option.kind.to_string()`: el `Display` del paso 1 |

### Paso 5: `PrepareClose::execute(id, preferred)` (`application/src/tasks/close_task.rs`)

| # | Qué                                            | Código clave                                                                                                   |
| - | ----------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| 1 | Pedir **a la vez** la tarea y sus estados | `let (task, statuses) = futures::try_join!(self.ctx.tasks.task(id), self.ctx.tasks.available_statuses(id))?;` |
| 2 | Calcular el nombre sugerido                     | `suggest_final(&statuses, preferred).map(\|option\| option.name.clone())`                                       |
| 3 | Armar `CloseOptions`                          | Tabla de abajo                                                                                                  |

| Campo de `CloseOptions` | Valor                                                     |
| ------------------------- | --------------------------------------------------------- |
| `task_id`               | `task.id.as_str().to_owned()`                           |
| `title`                 | `task.title` (se mueve: `task` es tuyo)               |
| `current_status`        | `task.status.to_string()` → `"in progress"`          |
| `statuses`              | `statuses.iter().map(StatusOptionView::from).collect()` |
| `suggested`             | El `Option<String>` del punto 2                         |

Calcula `suggested` **antes** de construir el struct, en su propia variable: `suggest_final` devuelve una referencia **dentro** de `statuses`, y con `.map(|option| option.name.clone())` la conviertes en un `String` propio que ya no depende de `statuses`.

Dentro del struct literal puedes mover `task.title` y leer `task.id` y `task.status` en cualquier orden: son **campos distintos** de un valor que te pertenece (movimientos parciales, como en `CreateProject` de la fase 5).

### Paso 6: `CloseTask::execute(command)` (mismo archivo)

**En este orden exacto.** Hasta el punto 3 no se escribe nada:

| # | Qué                                                                                                                       | Si falla                                                                                |
| - | -------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| 1 | `let statuses = self.ctx.tasks.available_statuses(&command.task_id).await?;`                                             | `NotFound` si la tarea no existe (lo espera `missing_task_is_not_found`)            |
| 2 | `let status = find_status(&statuses, &command.status)`                                                                   | `None` → `Err(AppError::InvalidInput(unknown_status(&command.status, &statuses)))` |
| 3 | Comentario recortado: `command.comment.as_deref().map(str::trim).filter(\|t\| !t.is_empty())`                              | —                                                                                      |
| 4 | Si el punto 3 dio `Some(text)` → `self.ctx.tasks.add_comment(&command.task_id, text).await?`                          | Error del proveedor                                                                     |
| 5 | `self.ctx.tasks.set_status(&command.task_id, &status.name).await?`: con `status.name`, **no** `command.status` | Error del proveedor (el comentario ya quedó publicado)                                 |
| 6 | Devolver `CloseResult`                                                                                                   | —                                                                                      |

| Campo de `CloseResult` | Valor                                                                                       |
| ------------------------ | ------------------------------------------------------------------------------------------- |
| `task_id`              | `command.task_id.as_str().to_owned()`                                                     |
| `final_status`         | `status.name.clone()` → `"Code Review"`, aunque el usuario escribió `"code review"` |
| `commented`            | `comment.is_some()` (el del punto 3)                                                      |

Para el punto 2, `ok_or_else` convierte el `Option` en `Result` y el `?` sale con el error:

```rust
let status = find_status(&statuses, &command.status)
    .ok_or_else(|| AppError::InvalidInput(unknown_status(&command.status, &statuses)))?;
```

`unknown_status` ya está hecha y arma: `unknown status `QA`; available: to do, Code Review, complete`.

| Comentario recibido    | Resultado del punto 3 | ¿Se publica?  |
| ---------------------- | --------------------- | -------------- |
| `None`               | `None`              | No             |
| `Some("   ")`        | `None`              | No             |
| `Some("  Listo.  ")` | `Some("Listo.")`    | Sí, recortado |

### Paso 7: `send` con cualquier método (`clickup/src/client.rs`)

`send` ya contiene **tu `get` de la fase 3**, movido sin cambios: por eso todos los tests anteriores siguen pasando. Pero todavía envía siempre `GET` y sin cuerpo. Son **tres cambios** dentro del `loop`, más un renombre:

**1 y 2. Usar `method` y agregar el cuerpo si hay.** Reemplaza:

```rust
let response = self
    .http
    .get(&url)
    .header("Authorization", &self.token)
    .query(query)
    .send()
    .await
    .map_err(|e| ProviderError::Network(e.to_string()))?;
```

por:

```rust
let mut request = self
    .http
    .request(method.clone(), &url)          // cambio 1: el método recibido
    .header("Authorization", &self.token)
    .query(query);
if let Some(body) = body {                  // cambio 2: el cuerpo, solo si hay
    request = request.json(body);
}
let response = request
    .send()
    .await
    .map_err(|e| ProviderError::Network(e.to_string()))?;
```

**3. `method.clone()`:** ya está en el código de arriba. Ver "Conceptos" para saber por qué hace falta.

**4. Renombrar la variable del final.** Abajo tienes `let body = response.text()...` y `serde_json::from_str::<T>(&body)`. Funciona, porque ese `body` nuevo *oculta* (shadowing) al parámetro, pero confunde: hay dos cosas distintas llamadas `body`. Llámala `text`:

```rust
let text = response.text().await.map_err(...)?;
serde_json::from_str::<T>(&text).map_err(...)
```

Borra el comentario `// TODO(fase 7)` de arriba de `send`.

### Paso 8: `status_option` (`clickup/src/mapper.rs`)

| `status.kind` (el `type` de ClickUp) | `StatusKind` |
| ---------------------------------------- | -------------- |
| `"open"`                               | `Open`       |
| `"done"`                               | `Done`       |
| `"closed"`                             | `Closed`     |
| `"custom"` **y cualquier otro**  | `Custom`     |

```rust
let kind = match status.kind.as_str() {
    "open" => StatusKind::Open,
    "done" => StatusKind::Done,
    "closed" => StatusKind::Closed,
    _ => StatusKind::Custom,
};
StatusOption { name: status.status, kind }
```

`status` llega por valor: `status.status` (el nombre) se mueve sin clonar. `as_str()` hace falta porque un `match` con literales (`"open"`) compara `&str`, y `status.kind` es `String`.

### Paso 9: los tres métodos de escritura (`clickup/src/provider.rs`)

**`available_statuses(id)`:**

| # | Petición                                                  | Tipo                  |
| - | ---------------------------------------------------------- | --------------------- |
| 1 | `GET /task/{id}` → `format!("/task/{}", id.as_str())` | `ApiTask`           |
| 2 | `GET /list/{list_id}`, con `task.list.id`              | `ApiList`           |
| 3 | `list.statuses.into_iter().map(status_option).collect()` | `Vec<StatusOption>` |

Las dos peticiones van **en secuencia** (no con `try_join!`): la segunda necesita el `list.id` que da la primera.

**`set_status(id, status)`:**

```rust
let _: IgnoredAny = self
    .client
    .put(&format!("/task/{}", id.as_str()), &json!({ "status": status }))
    .await?;
Ok(())
```

**`add_comment(id, body)`:**

| Campo del JSON   | Valor     | Por qué                                                                                                                                            |
| ---------------- | --------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `comment_text` | `body`  | El texto                                                                                                                                            |
| `notify_all`   | `false` | Con `true`, ClickUp también te notifica a **ti**, que escribiste el comentario. Los asignados y seguidores de la tarea reciben aviso igual |

Misma forma que `set_status`, con `.post(&format!("/task/{}/comment", id.as_str()), &json!({ ... }))`.

Los tests de wiremock comprueban el método, la ruta, el header y el **cuerpo exacto** (`body_json`). Si falta `notify_all` o sobra un campo, no coinciden y fallan con 404.

### Paso 10: `status_choice` (`cli/src/commands/close.rs`)

La tabla está en el comentario de la función. Con un `match` sobre la tupla:

```rust
match (status, yes, suggested) {
    (Some(status), _, _) => Ok(StatusChoice::Use(status)),
    (None, false, _) => Ok(StatusChoice::Ask),
    (None, true, Some(suggested)) => Ok(StatusChoice::Use(suggested.to_owned())),
    (None, true, None) => anyhow::bail!("no hay un estado sugerido para cerrar esta tarea; indícalo con --status"),
}
```

- `anyhow::bail!(...)` es un atajo de `return Err(anyhow::anyhow!(...))`.
- `suggested.to_owned()`: `suggested` es `&str`, prestado de las opciones. `StatusChoice::Use` guarda un `String` propio.
- El test `yes_without_suggestion_asks_for_the_flag` comprueba que el mensaje mencione `--status`.

### Paso 11: presentador (`cli/src/presenter.rs`)

**`status_kind_label(kind)`:**

| `kind`       | Texto           |
| -------------- | --------------- |
| `"open"`     | `"abierto"`   |
| `"custom"`   | `"en curso"`  |
| `"done"`     | `"terminado"` |
| `"closed"`   | `"cerrado"`   |
| cualquier otro | el mismo texto  |

Igual que `kind_label` de la fase 5.

**`status_label(status, suggested)`:**

- Base: `format!("{} ({})", status.name, status_kind_label(&status.kind))` → `"Code Review (en curso)"`.
- Si `suggested` es `true`, agregar `" · sugerido"` → `"complete (terminado) · sugerido"`.

**`close_summary(options, status, comment)`:** tres líneas unidas con `\n`, **sin** salto al final:

| Línea | Texto                                                                                                                |
| ------ | -------------------------------------------------------------------------------------------------------------------- |
| 1      | `"{title} ({task_id})"` → `Fix invoice rounding (86b2x1abc)`                                                    |
| 2      | `"Estado: {current_status} → {status}"` → `Estado: in progress → Code Review`                                 |
| 3      | `"Comentario: {texto}"`, donde el texto es el comentario recortado o, si es `None` o queda vacío, `(ninguno)` |

Para el comentario: `comment.map(str::trim).filter(|t| !t.is_empty()).unwrap_or("(ninguno)")`. Es el mismo recorte que en el paso 6: el resumen debe mostrar exactamente lo que se va a publicar.

**`close_result(result)`:**

| `commented` | Texto                                                     |
| ------------- | --------------------------------------------------------- |
| `true`      | `"✔ 86b2x1abc → Code Review · comentario publicado"` |
| `false`     | `"✔ 86b2x1abc → Code Review"`                         |

### Paso 12: `close::run` (`cli/src/commands/close.rs`)

Usa `ColorfulTheme::default()` en todas las preguntas.

| # | Qué          | Cómo                                                                                                                                                                                                                                                       |
| - | ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1 | Opciones      | `PrepareClose::new(ctx).execute(task_id, &settings.provider.status_map.done).await?`                                                                                                                                                                      |
| 2 | Estado        | `match status_choice(status, yes, options.suggested.as_deref())?` → con `Use(s)`, `s`; con `Ask`, el `Select` de abajo                                                                                                                           |
| 3 | Comentario    | `match comment`: `Some(c)` → `Some(c)`; `None if yes` → `None`; `None` → `Some(Input…)`                                                                                                                                                   |
| 4 | Confirmación | Solo si `!yes`: imprimir `presenter::close_summary(&options, &status, comment.as_deref())` y `Confirm` con `"¿Confirmar cierre?"` y `.default(false)`. Si devuelve `false`: `println!("Cancelado. No se cambió nada.")` y `return Ok(())` |
| 5 | Cerrar        | `CloseTask::new(ctx).execute(CloseCommand { task_id: task_id.clone(), status, comment }).await?`                                                                                                                                                          |
| 6 | Mostrar       | `println!("{}", presenter::close_result(&result))`                                                                                                                                                                                                        |

El `Select` del punto 2:

| Parte           | Valor                                                                                                                |
| --------------- | -------------------------------------------------------------------------------------------------------------------- |
| `with_prompt` | `format!("Estado final para \"{}\"", options.title)`                                                               |
| `items`       | Por cada estado de `options.statuses`: `presenter::status_label(s, Some(s.name.as_str()) == suggested)`          |
| `default`     | La posición del sugerido: `options.statuses.iter().position(\|s\| Some(s.name.as_str()) == suggested).unwrap_or(0)` |
| Resultado       | El índice elegido → `options.statuses[index].name.clone()`                                                       |

Donde `suggested` es `options.suggested.as_deref()`, un `Option<&str>`. Comparar `Option<&str>` con `Option<&str>` evita clonar.

El `Input` del punto 3: `Input::<String>::with_theme(&theme).with_prompt("Comentario de cierre (opcional)").allow_empty(true).interact_text()?`.

---

## Paso 13: contra tu ClickUp real

**Cuidado: este comando escribe en ClickUp.** Pruébalo con una tarea de prueba, no con una de un cliente.

1. Crea en ClickUp una tarea de prueba asignada a ti y copia su id (el final de la URL).
2. Ejecuta sin flags y responde **no** a la confirmación:

   ```sh
   cargo run -p taskflow -- close <id>
   ```

   Comprueba en ClickUp que **nada** cambió.
3. Otra vez, ahora con **sí**. Comprueba el estado y el comentario en ClickUp.
4. Un estado que no existe, para ver el error:

   ```sh
   cargo run -p taskflow -- close <id> --status "no existe" --yes
   ```

   Debe listar los estados válidos de esa lista y no escribir nada.
5. Si quieres que la sugerencia sea tu estado de cierre habitual:

   ```sh
   cargo run -p taskflow -- config set provider.status_map.done '["Code Review", "complete", "closed"]'
   ```

---

## Limitaciones conocidas

- **Sin deshacer:** si `set_status` falla después de publicar el comentario, el comentario queda publicado. Volver a ejecutar `close` termina el cambio de estado.
- **Un comentario a la vez:** no se pueden asignar comentarios a otra persona (`assignee`) ni publicar uno sin cerrar. Puede llegar con la sesión de trabajo (fase 8).
- **El límite de peticiones** sigue pendiente (README, "Pendientes importantes"). `close` hace 3 o 4 peticiones: no lo roza.

---

## Qué observar mientras implementas

- **`application` no pregunta nada.** Toda la conversación (`Select`, `Input`, `Confirm`) vive en `close.rs` del CLI. `CloseTask` solo recibe lo decidido.
- **La primera escritura reutiliza todo lo de lectura.** Autenticación, semáforo, reintentos y errores HTTP salen de `send`, que ya tenías. Por eso el paso 7 son solo tres cambios.
- **`writes()` del fake** es lo que permite probar el **orden** de las escrituras sin ClickUp: comentario primero, estado después.

Cuando termines, avísame y reviso. Las dudas las sumamos al final de este documento.

---

# Conceptos aplicados

La fase quedó completa. Los pasos 1 a 8 los implementaste tú; los pasos 9 a 12 y las correcciones de abajo los hice yo.

## Paso 5: no le pongas a una variable el nombre de una función

```rust
let suggest_final = suggest_final(&statuses, preferred).map(...);   // compila, pero confunde
let suggested = suggest_final(&statuses, preferred).map(...);       // ✅
```

La variable **oculta** (*shadowing*) a la función desde esa línea: en la misma línea, `suggest_final` es primero una función y después un `Option<String>`. Si más abajo quisieras volver a llamar a la función, el error del compilador sería desconcertante. Con `suggested`, que además coincide con el nombre del campo, puedes usar la forma corta `CloseOptions { suggested, .. }`.

## Paso 6: `unwrap` sobre un `None` que esperas

La primera versión terminaba la cadena del comentario con `.unwrap()` y llamaba a `add_comment` siempre:

```rust
let clean_comment = command.comment.as_deref().map(str::trim).filter(|t| !t.is_empty()).unwrap();
self.ctx.tasks.add_comment(&command.task_id, clean_comment).await?;
```

Sin comentario, o con uno de solo espacios, el `filter` da `None` y el `unwrap()` hace panic. Eso es lo que detectó `blank_comments_are_not_published`. La versión final:

```rust
let clean_comment = command.comment.as_deref().map(str::trim).filter(|t| !t.is_empty());
if let Some(text) = clean_comment {
    self.ctx.tasks.add_comment(&command.task_id, text).await?;
}
// ...
commented: clean_comment.is_some(),   // lo que realmente pasó, no command.comment
```

**Regla práctica:**

| `None` significa... | Qué usar |
| --- | --- |
| Un error de programación imposible (el semáforo nunca se cierra) | `expect("por qué no puede pasar")` |
| Un caso normal que esperas (el usuario no comentó) | `if let`, `match`, `map`, `unwrap_or` |

Y un detalle relacionado: `commented` debe salir del comentario **limpio**, no de `command.comment`. Si el usuario escribió `"   "`, `command.comment` es `Some(...)`, pero no se publicó nada.

### Dos detalles que clippy marca sin el bloque `#![allow]`

| Escrito | Problema | Corrección |
| --- | --- | --- |
| `&status.name.as_str()` | `&&str`: el `&` sobra (`needless_borrow`). Compila porque Rust quita el sobrante solo (*deref coercion*) | `&status.name` |
| `Ok({ CloseResult { ... } })` | Un bloque `{ }` que solo devuelve el struct: no hace nada | `Ok(CloseResult { ... })` |

## Paso 7: el cambio que faltaba en `send`

Agregaste el cuerpo con `request.json(body)`, pero el builder seguía empezando con `.get(&url)`:

```rust
.get(&url)                         // ❌ siempre GET, aunque `method` sea PUT o POST
.request(method.clone(), &url)     // ✅ el método recibido
```

Ningún test de los pasos 1 a 8 lo podía detectar: los únicos que envían `PUT` y `POST` son los de `set_status` y `add_comment`, del paso 9. En cuanto el paso 9 estuvo hecho, wiremock habría recibido un `GET` donde esperaba un `PUT`, y habría respondido 404 (ninguna ruta coincide). Es un buen ejemplo de por qué los tests de integración prueban **el comportamiento completo** y no cada pieza por separado.

También quedó:

- **Un `;` después de `request = request.json(body)`.** Sin él compila igual (una asignación no devuelve nada), pero `rustfmt` y la convención lo agregan.
- **Un `use reqwest::Body` sin usar**, que clippy encontró al quitar el bloque `#![allow]`.

## Paso 8: el comodín ya incluye todo

```rust
"custom" | _ => StatusKind::Custom,   // ❌ clippy: wildcard_in_or_patterns
_ => StatusKind::Custom,              // ✅
```

`_` coincide con **cualquier** valor, `"custom"` incluido, así que `"custom" | _` es lo mismo que `_`. Si querías que se leyera que `"custom"` es un caso esperado, el comentario de la función ya lo dice.

## Paso 9: leer en secuencia, ignorar respuestas

```rust
let task: ApiTask = self.client.get(&format!("/task/{}", id.as_str()), &[]).await?;
let list: ApiList = self.client.get(&format!("/list/{}", task.list.id), &[]).await?;
Ok(list.statuses.into_iter().map(status_option).collect())
```

Estas dos peticiones van **en secuencia**, sin `try_join!`: la segunda necesita un dato (`task.list.id`) que solo trae la primera. Concurrencia solo cuando las peticiones son independientes.

```rust
let _: IgnoredAny = self.client.put(&path, &json!({ "status": status })).await?;
```

- `json!` arma el cuerpo sin definir un struct para dos campos.
- `let _: IgnoredAny` le da a `put` su tipo `T` y descarta la tarea completa que devuelve ClickUp. Igual comprueba que la respuesta sea un éxito y un JSON válido.

## Pasos 10 a 12: la conversación vive en el CLI

- **`status_choice`** es una función pura con un `match` sobre `(status, yes, suggested)`. Toda la lógica de decisión se prueba sin terminal; `run` solo pregunta cuando le dicen `Ask`.
- **`None if yes => None`** en el comentario: un brazo con guarda dentro de un `match` sobre un `Option`. Con `--yes` no se pregunta nada, ni siquiera si quieres comentar.
- **`Some(s.name.as_str()) == suggested`** compara dos `Option<&str>` sin clonar nada, tanto para marcar "sugerido" como para elegir el `default` del `Select`.
- **`Confirm` con `.default(false)`:** si presionas Enter sin pensar, no se escribe nada (FR-12).

## Al cerrar la fase

- Se borraron los cuatro bloques `#![allow(...)]` y el `// TODO(fase 7)` de `send`.
- `cargo test --workspace`: 241 tests. Clippy y `cargo fmt` sin avisos.
- Falta el **paso 13**, contra tu ClickUp real, con una tarea de prueba.
