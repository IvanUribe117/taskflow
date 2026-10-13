TODO(fase 2)TODO(fase 2)TODO(fase 2)TODO(fase 2)TODO(fase 2)TODO(fase 2)TODO(fase 2)TODO(fase 2)TODO(fase 2)n/src/lib.rsn/src/lib.rsn/src/lib.rsn/src/lib.rsn/src/lib.rscrates/application/src/lib.rs.hay mejor manera de No# Fase 3: adaptador de ClickUp (lectura)

Objetivo del roadmap (§24): *"`adapter-clickup` de lectura: imprime las tareas reales con su ubicación"*.

Al terminar, este comando mostrará **tus tareas reales de ClickUp**, priorizadas por el `GetTodayTasks` que escribiste en la fase 2:

```sh

```

El caso de uso no cambia ni una línea: solo se enchufa un `TaskProvider` real donde antes estaba el fake. Esa es la promesa de la arquitectura hexagonal, y en esta fase la vas a ver cumplirse.

Las firmas, los tipos del JSON, los fixtures y los tests ya están escritos. Ahora mismo **27 tests fallan a propósito** (17 unitarios y 10 de integración HTTP). Comprobé los tests con una implementación de referencia que después borré: si uno falla, el problema está en la implementación, no en el test.

---

## Mapa del crate

```text
crates/adapters/clickup/
├── Cargo.toml
├── src/
│   ├── lib.rs          ← módulos y API pública (ClickUpConfig, ClickUpProvider, StatusMap)
│   ├── config.rs       ← ClickUpConfig                                        [hecho]
│   ├── api_types.rs    ← structs que reflejan el JSON de ClickUp              [hecho]
│   ├── status_map.rs   ← nombres de ClickUp ⇄ Status del dominio              [pasos 1]
│   ├── mapper.rs       ← JSON de ClickUp → Task, Comment                      [pasos 2-6]
│   ├── query.rs        ← TaskQuery → parámetros de la URL                     [paso 7]
│   ├── client.rs       ← HTTP: token, errores, semáforo, reintentos           [pasos 8-9]
│   └── provider.rs     ← impl TaskProvider for ClickUpProvider                [paso 10]
├── tests/
│   ├── provider.rs     ← tests HTTP contra un servidor falso (wiremock)
│   └── fixtures/       ← JSON reales de ClickUp: user, task, páginas, comentarios
└── examples/
    └── today.rs        ← tus tareas reales en la terminal                     [hecho]
```

El flujo de una petición atraviesa las capas así:

```text
GetTodayTasks ──search_tasks(&TaskQuery)──► provider.rs
                                              │ query.rs: TaskQuery → [("assignees[]","183"), ...]
                                              ▼
                                           client.rs: GET + token + semáforo + reintentos
                                              │ JSON
                                              ▼
                                           api_types.rs: serde → ApiTask
                                              │
                                              ▼
                                           mapper.rs: ApiTask → Task (dominio)
                                              │
GetTodayTasks ◄──────── Vec<Task> ────────────┘
```

---

## Pasos

| #  | Archivo           | Qué implementar                                           | Verificar con                                                                                                                                                                                            | Conceptos                                                   |
| -- | ----------------- | ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| 1  | `status_map.rs` | `to_domain`, `names_for`                               | `cargo test -p adapter-clickup --lib status_map`                                                                                                                                                       | Closures,`iter().any`, `eq_ignore_ascii_case`           |
| 2  | `mapper.rs`     | `parse_millis`                                           | `cargo test -p adapter-clickup --lib timestamps`                                                                                                                                                       | `str::parse`, `Option` → `Result`                    |
| 3  | `mapper.rs`     | `map_priority`                                           | `cargo test -p adapter-clickup --lib priorities`                                                                                                                                                       | `match` sobre `Option<&str>` con literales              |
| 4  | `mapper.rs`     | `map_location`                                           | `cargo test -p adapter-clickup --lib location`                                                                                                                                                         | `Option::filter`, `as_ref`                              |
| 5  | `mapper.rs`     | `map_task`                                               | `cargo test -p adapter-clickup --lib mapper`                                                                                                                                                           | Mover campos,`transpose`, `?` con `#[from]`           |
| 6  | `mapper.rs`     | `TryFrom<ApiComment> for Comment`                        | `cargo test -p adapter-clickup --lib comments`                                                                                                                                                         | `TryFrom`, regla del huérfano                            |
| 7  | `query.rs`      | `task_search_params`                                     | `cargo test -p adapter-clickup --lib query`                                                                                                                                                            | Construir`Vec`, `match` con `continue`                |
| 8  | `client.rs`     | `status_error`                                           | `cargo test -p adapter-clickup --lib client`                                                                                                                                                           | `StatusCode`, guardas en `match`                        |
| 9  | `client.rs`     | `ClickUpClient::get`                                     | (se prueba en el paso 10)                                                                                                                                                                                | `reqwest`, genéricos, `Semaphore`, bucle de reintentos |
| 10 | `provider.rs`   | `current_user`, `search_tasks`, `task`, `comments` | `cargo test -p adapter-clickup --test provider`                                                                                                                                                        | Paginación,`try_join_all`, tests de integración         |
| 11 | —                | Probar contra tu ClickUp real                              | `cargo run comments_are_mappedcomments_are_mappedcomments_are_mappedcomments_are_mappedcomments_are_mappedcomments_are_mappedcomments_are_mappedcomments_are_mapped-p adapter-clickup --example today` | Variables de entorno,`.env`                               |

En el paso 5, `cargo test -p adapter-clickup --lib mapper` también ejecuta los tests de los pasos 2, 3, 4 y 6, porque todos viven en `mapper.rs`.

Al terminar, **borra el bloque `#![allow(...)]`** marcado con `TODO(fase 3)` en `crates/adapters/clickup/src/lib.rs` y comprueba:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

---

## Antes de empezar: conceptos nuevos de esta fase

### `serde`: del JSON a structs de Rust

```rust
#[derive(Debug, Deserialize)]
pub(crate) struct ApiStatus {
    pub status: String,
    #[serde(rename = "type")]
    pub kind: String,
}
```

`#[derive(Deserialize)]` genera en tiempo de compilación el código que lee el JSON y llena el struct. Las reglas que usa `api_types.rs`:

| Situación en el JSON               | En Rust                                                      | Qué pasa si no coincide                         |
| ----------------------------------- | ------------------------------------------------------------ | ------------------------------------------------ |
| Campo siempre presente              | `pub name: String`                                         | Error si falta                                   |
| Campo que puede ser`null`         | `pub priority: Option<ApiPriority>`                        | `null` → `None`                             |
| Campo que puede**faltar**     | `#[serde(default)]` + `Option<T>`, `Vec<T>` o `bool` | Usa`Default` (`None`, `vec![]`, `false`) |
| Nombre reservado en Rust (`type`) | `#[serde(rename = "type")] pub kind`                       | —                                               |
| Campos que no nos interesan         | No se declaran                                               | `serde` los **ignora** por defecto       |

Esto último es importante: una tarea de ClickUp trae unos 40 campos y `ApiTask` declara 12. Si mañana ClickUp agrega un campo nuevo, nada se rompe.

**Por qué `api_types` está separado del dominio:** los structs de la API siguen las decisiones de ClickUp (ids numéricos, fechas en texto, carpetas "ocultas"). Si `Task` se deserializara directo del JSON, esas decisiones contaminarían el dominio. `api_types` + `mapper` forman una **capa anticorrupción**: lo raro de ClickUp entra, se traduce y no pasa de ahí.

### Leer JSON en tests sin abrir archivos: `include_str!`

```rust
let task: ApiTask = serde_json::from_str(include_str!("../tests/fixtures/task.json")).unwrap();
```

`include_str!` es una macro que **pega el contenido del archivo dentro del binario al compilar**. El test no lee el disco en ejecución, y si el archivo no existe, no compila. La ruta es relativa al archivo `.rs` que la usa.

### Tests unitarios vs tests de integración

|                   | Unitarios                                                     | Integración                                                |
| ----------------- | ------------------------------------------------------------- | ----------------------------------------------------------- |
| Dónde            | `#[cfg(test)] mod tests` dentro de `src/`                 | Archivos en`tests/`                                       |
| Qué ven          | Todo, incluido lo privado (`pub(crate)`)                    | Solo la API pública:`ClickUpConfig`, `ClickUpProvider` |
| Cómo se compilan | Dentro del crate                                              | Como un crate aparte que depende del tuyo                   |
| Aquí prueban     | Traducciones puras: estados, fechas, prioridades, parámetros | El adaptador completo hablando HTTP                         |

`tests/provider.rs` usa el crate exactamente como lo usará `bootstrap` en la fase 4. Si algo funciona ahí, funciona para quien lo use.

### `wiremock`: un servidor HTTP falso

```rust
let server = MockServer::start().await;          // servidor real en un puerto local
Mock::given(path("/user"))                       // cuando llegue GET /user...
    .and(header("Authorization", TOKEN))         // ...con este header...
    .respond_with(ok("user"))                    // ...responde con user.json...
    .expect(1)                                   // ...y debe llamarse exactamente 1 vez
    .mount(&server)
    .await;
```

- **Por qué no llamar a ClickUp de verdad:** los tests serían lentos, necesitarían tu token, fallarían sin internet y no podrías simular un 429 o un 500. NFR-09 exige probar sin red real.
- **`.expect(n)`:** al terminar el test, wiremock verifica que el mock se usó exactamente `n` veces. `expect(0)` comprueba que algo **no** ocurrió (por ejemplo, pedir una página de más).
- **`up_to_n_times(1)`:** el mock responde solo una vez; las siguientes peticiones pasan al siguiente mock montado. Así se simula "primero 429, después 200".
- **`base_url` en la configuración** existe para esto: en producción apunta a `https://api.clickup.com/api/v2`, en los tests a `server.uri()`. Es inyección de dependencias con un simple `String`.

### Secretos y `Debug`

`ClickUpConfig` y `ClickUpClient` implementan `Debug` **a mano** para mostrar `token: "***"`. Con `#[derive(Debug)]`, el token aparecería en cualquier `{:?}`, en un panic o en un log (NFR-04). El test `debug_never_shows_the_token` lo vigila.

---

## Pistas por paso

Intenta primero sin leerlas.

<details>
<summary>1. StatusMap</summary>

- Una closure local evita repetir la búsqueda en cada lista:
  `let has = |list: &[String]| list.iter().any(|n| n.eq_ignore_ascii_case(name));`
- `eq_ignore_ascii_case` compara sin distinguir mayúsculas y sin crear `String` nuevos.
- Después, una cadena de `if has(&self.todo) { ... } else if ...` en el orden del comentario.
- `names_for`: un `match` sobre `status` que devuelve un `clone()` de la lista correspondiente. ¿Por qué hace falta `clone()` si `self` es `&self`? (Mismo motivo que en `TaskView`.)

</details>

<details>
<summary>2. parse_millis</summary>

- `raw.parse::<i64>()` devuelve `Result<i64, ParseIntError>`. El `::<i64>` (*turbofish*) le dice a `parse` a qué tipo convertir.
- `DateTime::from_timestamp_millis(ms)` devuelve `Option<DateTime<Utc>>`: `None` si está fuera de rango.
- Hay dos fallos posibles, pero ambos deben convertirse en el mismo error. Una forma: pasar el `Result` a `Option` con `.ok()`, encadenar con `.and_then(...)` y volver a `Result` con `.ok_or_else(|| MapError::InvalidTimestamp(raw.to_owned()))`.

</details>

<details>
<summary>3. map_priority</summary>

- `priority.map(|p| p.id.as_str())` convierte `Option<&ApiPriority>` en `Option<&str>`.
- Sobre eso, un `match` con patrones literales: `None`, `Some("1")`, ..., `Some(other)`.

</details>

<details>
<summary>4. map_location</summary>

- Construye un `Vec<LocationSegment>` y termina con `TaskLocation::new(segments)`.
- `task.folder.as_ref().filter(|f| !f.hidden)` da `Option<&ApiFolder>` solo si existe **y** no está oculta. Úsalo con `if let Some(folder) = ...`.
- Recibes `&ApiTask`: para los `String` necesitas `clone()`.

</details>

<details>
<summary>5. map_task</summary>

- Aquí recibes `ApiTask` **por valor**: puedes **mover** sus `String` (`title: task.name`) sin clonar.
- **Cuidado con el orden:** `map_location(&task)` toma prestada la tarea completa. Si primero mueves `task.name` y después llamas `map_location(&task)`, el compilador se queja (`borrow of partially moved value`). Calcula la ubicación **antes** de mover campos.
- `due_date` es `Option<String>` y `parse_millis` devuelve `Result`. `task.due_date.as_deref().map(parse_millis)` da `Option<Result<...>>`; `.transpose()` lo convierte en `Result<Option<...>>`, y con `?` queda `Option<DateTime<Utc>>`.
- `description`: `task.text_content.filter(|t| !t.is_empty())`.
- `TaskId::new(...)` devuelve `Result<_, DomainError>`; el `?` lo convierte en `MapError` gracias a `#[from] DomainError`.

</details>

<details>
<summary>6. TryFrom<ApiComment> for Comment</summary>

- Para el `assignee`, combina lo del paso 5: `comment.assignee.as_ref().map(user_id).transpose()?`.
- `user_id` ya existe en `mapper.rs`.
- `author`: `username.clone().unwrap_or_else(|| id.to_string())`.

</details>

<details>
<summary>7. task_search_params</summary>

- Empieza con un `Vec` que tenga los tres parámetros fijos y ve haciendo `push`.
- Para las ubicaciones, un `match` que elige la clave según el nivel. Los niveles que no corresponden pueden usar `_ => continue`: `continue` es una expresión de tipo `!` ("nunca"), así que encaja en cualquier brazo.
- Para estados: por cada `Status`, `statuses.names_for(status)` puede dar varios nombres, y cada uno es un `statuses[]` distinto.

</details>

<details>
<summary>8. status_error</summary>

- `status.is_success()`, `status.is_server_error()`.
- En un `match`, las constantes (`StatusCode::NOT_FOUND`) funcionan como patrones, y `A | B` agrupa varios.
- Para rangos: un brazo con guarda, `s if s.is_server_error() => ...`.

</details>

<details>
<summary>9. ClickUpClient::get</summary>

- **El permiso del semáforo:** `let _permit = self.permits.acquire().await...;`. El permiso se devuelve cuando la variable se destruye (**RAII**). Ojo: `let _ = ...` (guion bajo **solo**) destruye el valor de inmediato y el semáforo no limitaría nada. `_permit` (con nombre) lo mantiene vivo hasta el final de la función.
- **La petición:** `self.http.get(&url).header("Authorization", &self.token).query(query).send().await`.
- **El bucle de reintentos:** un `loop` con un contador. Si es 429 y quedan intentos, `tokio::time::sleep(self.retry_delay).await` y `continue`.
- **El cuerpo:** `response.json::<T>().await`. Cualquier error ahí es `InvalidResponse`.
- **Errores de `send()`:** `.map_err(|e| ProviderError::Network(e.to_string()))?`.

</details>

<details>
<summary>10. ClickUpProvider</summary>

- El tipo de destino de `get` se infiere de la variable: `let page: ApiTasksPage = self.client.get(&path, &params).await?;`.
- `map_task` devuelve `MapError` y el método devuelve `ProviderError`: el `?` funciona gracias a `impl From<MapError> for ProviderError` (ya está en `mapper.rs`).
- Paginación: `loop`, pedir la página, mapear y acumular, decidir si era la última (`last_page.unwrap_or(cantidad < PAGE_SIZE)`), y si no, `page += 1`.
- Búsqueda por ids: lo mismo que hiciste con los comentarios en la fase 2, `try_join_all(query.ids.iter().map(|id| self.task(id)))`.
- Comentarios: `page.comments.into_iter().map(Comment::try_from).collect::<Result<Vec<_>, _>>()?`. `collect` sobre un iterador de `Result` se detiene en el primer error.

</details>

---

## Paso 11: probarlo contra tu ClickUp real

1. Crea un token personal en ClickUp: *Settings → Apps → API Token* (empieza con `pk_`).
2. Busca el ID del Workspace: es el número que aparece en la URL al abrir ClickUp (`app.clickup.com/<team_id>/...`).
3. En la raíz del repo:

   ```sh
   cp .env.example .env
   # completa TASKFLOW_CLICKUP_TOKEN y TASKFLOW_CLICKUP_TEAM_ID
   cargo run -p adapter-clickup --example today
   ```

`.env` está en `.gitignore`: el token nunca llegará al repositorio. `examples/today.rs` es pegamento temporal; en la fase 4 lo reemplazan `bootstrap` y el CLI.

**Qué puede salir distinto con datos reales:**

- **Estados que aparecen como `Other`:** tus estados de ClickUp tienen nombres que `StatusMap::default()` no conoce. Es esperado; en la fase 4 se configuran en `config.toml`.
- **Error `invalid timestamp` o `unknown priority id`:** ClickUp envió algo que el mapper no espera. Mira qué tarea es, y si es un caso legítimo, agrega un fixture y un test que lo reproduzcan **antes** de corregir el mapper.

---

## Limitaciones conocidas (aceptadas para esta fase)

- **Comentarios:** solo los 25 más recientes por tarea. Un comentario asignado más antiguo no se contaría como pendiente. Ejercicio opcional: paginar con `start` y `start_id` (ver `ARCHITECTURE.md` §16).
- **Espera ante 429:** fija (`retry_delay`). ClickUp envía headers como `X-RateLimit-Reset` que permitirían esperar lo justo; se puede mejorar cuando haga falta.
- **Nombre del espacio:** la tarea solo trae su id; el nombre llegará con `TaskProvider::locations()` en la fase 5.

---

## Qué observar mientras implementas

- **`GetTodayTasks` no cambia.** El mismo `execute` que probaste con fakes funciona con ClickUp real.
- **La concurrencia ahora tiene freno.** En la fase 2, `try_join_all` lanzaba todas las peticiones de comentarios a la vez. El semáforo del cliente deja pasar como máximo 8 (`max_concurrent_requests`) sin que la capa de aplicación lo sepa (NFR-07).
- **Cada error de ClickUp tiene un significado en el dominio:** 401 → `Unauthorized`, 404 → `NotFound`, JSON raro → `InvalidResponse`. Ninguna capa superior ve un `reqwest::Error`.

Cuando termines, avísame y reviso tu implementación. Las dudas que surjan las agregamos al final de este documento, como en la fase 2.

---

# Conceptos aplicados

Explicaciones que surgieron mientras implementabas cada paso, ordenadas por paso. Cada una responde una duda concreta.

## Paso 4: `map_location`

### Una tarea no tiene varias ubicaciones: tiene una ruta

`TaskLocation` no es una lista de ubicaciones alternativas. Es **una sola ubicación** descrita como un camino desde la raíz hasta la lista que contiene la tarea:

```text
Space (90110001) → Folder "Backend" (90110002) → List "ACME – Sprint" (901100200300)
```

Cada tramo de ese camino es un `LocationSegment { level, id, name }`. Por eso el `Vec<LocationSegment>` tiene dos o tres elementos: espacio, carpeta (si es visible) y lista.

### ¿De dónde sale `LocationLevel`?

`LocationLevel` es un enum del **dominio** (`domain::task::LocationLevel`, en `crates/domain/src/task/location.rs`). No viene del JSON: tú eliges el nivel según de qué campo de `ApiTask` estés leyendo:

| Campo de `ApiTask`                  | Nivel que asignas         |
| ------------------------------------- | ------------------------- |
| `task.space`                        | `LocationLevel::Space`  |
| `task.folder` (si no es `hidden`) | `LocationLevel::Folder` |
| `task.list`                         | `LocationLevel::List`   |

El enum es genérico a propósito (`Workspace`, `Repository`, `Other`...): otro proveedor, como GitHub, usaría los mismos tipos con otros niveles. El dominio describe "una ruta jerárquica", no "la jerarquía de ClickUp".

## Paso 5: `map_task`

### ¿Por qué `clone()` y no siempre referencias?

Una referencia (`&String`) es un préstamo: apunta a un dato que **le pertenece a otro**. El problema es que `Task` (del dominio) guarda `String`, no `&str`:

```rust
pub struct Task {
    pub title: String,   // Task es dueña de su título
    // ...
}
```

Si `Task` guardara referencias, necesitaría un lifetime (`Task<'a>`) y no podría vivir más que el `ApiTask` del que salió. Pero el `ApiTask` se descarta apenas termina el mapeo, mientras que la `Task` viaja por toda la aplicación (casos de uso, DTOs, el CLI). Tiene que ser **dueña** de sus datos.

Para que `Task` sea dueña hay dos caminos:

| Recibes                 | Cómo obtienes el `String` | Costo                         |
| ----------------------- | ---------------------------- | ----------------------------- |
| `&ApiTask` (prestado) | `task.list.name.clone()`   | Copia el texto                |
| `ApiTask` (por valor) | `task.name` (mover)        | Gratis: solo cambia de dueño |

Por eso `map_location(&task)` clona (solo tiene un préstamo, y además se llama antes de mover campos) y `map_task(task)` mueve. La regla práctica:

- Si la función **consume** el dato y ya nadie lo necesita después, recíbelo por valor y mueve.
- Si solo lo **lee**, recíbelo por referencia.
- Clona solo cuando necesitas un dueño nuevo y el original debe seguir existiendo.

Clonar unos pocos `String` cortos por tarea cuesta prácticamente nada comparado con la petición HTTP que los trajo. Llenar el código de lifetimes para evitarlo sería complejidad sin beneficio real.

### ¿Por qué `pub(crate)`?

La visibilidad en Rust tiene niveles:

| Visibilidad    | Quién puede usarlo                                                     |
| -------------- | ----------------------------------------------------------------------- |
| (nada)         | Solo el módulo actual y sus hijos                                      |
| `pub(crate)` | Cualquier módulo **de este crate** (`adapter-clickup`)         |
| `pub`        | Cualquiera, incluidos otros crates (si el módulo también es público) |

`mapper.rs` lo usan `provider.rs` y los tests de este crate, así que necesita salir del módulo. Pero **no debe salir del crate**: `map_task` recibe un `ApiTask`, que es un detalle de ClickUp. Si fuera `pub`, otros crates podrían depender del JSON de ClickUp, que es justo lo que la capa anticorrupción (`api_types` + `mapper`) quiere evitar.

`pub(crate)` documenta la intención y además el compilador la hace cumplir.

## Paso 6: `TryFrom<ApiComment> for Comment`

### Errores de la primera versión y por qué importan

| Problema                                    | Consecuencia                                                                         | Corrección                                                   |
| ------------------------------------------- | ------------------------------------------------------------------------------------ | ------------------------------------------------------------- |
| `assignee.unwrap()` para obtener el autor | Panic con cualquier comentario sin asignado, y además usaba a la persona equivocada | El autor sale de `comment.user`, no de `comment.assignee` |
| `.ok().expect(...)` en la fecha           | Panic con una fecha inválida en vez de devolver error                               | `parse_millis(&comment.date)?`                              |

La idea de fondo: **un adaptador nunca debe hacer panic por datos externos.** ClickUp puede mandar cualquier cosa; el contrato de `TryFrom` es devolver `Err` cuando no se puede traducir. Un `unwrap` o `expect` sobre datos de la API convierte un dato raro en un programa caído.

Los tests `comment_author_falls_back_to_the_author_id`, `comment_author_ignores_the_assignee` e `invalid_comment_date_is_an_error` existen para que esos errores no vuelvan.

### `as_ref().map(user_id).transpose()?`, paso a paso

```rust
comment.assignee                // Option<ApiUser>
    .as_ref()                   // Option<&ApiUser>          (no mueve el valor)
    .map(user_id)               // Option<Result<UserId, MapError>>
    .transpose()                // Result<Option<UserId>, MapError>
    ?                           // Option<UserId>             (o sale con el error)
```

Es el mismo patrón que `due` en `map_task`: cuando un campo es opcional **y** su conversión puede fallar, `transpose` invierte las capas para poder usar `?`.

## Paso 8: `status_error`

### Los brazos del `match` se revisan en orden

```rust
match status {
    StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Some(ProviderError::Unauthorized),
    StatusCode::NOT_FOUND => Some(ProviderError::NotFound(path.to_owned())),
    StatusCode::TOO_MANY_REQUESTS => Some(ProviderError::RateLimited { retry_after: None }),
    s if s.is_success() => None,
    s if s.is_server_error() => Some(ProviderError::Network(format!("{s} on {path}"))),
    s => Some(ProviderError::InvalidResponse(format!("{s} on {path}"))),
}
```

- El último brazo (`s =>` o `_ =>`) atrapa **todo** lo que no coincidió antes. Si falta el brazo de `is_success`, un 200 cae ahí y se vuelve error: todas las peticiones fallarían.
- `s if condición` es una **guarda**: el brazo coincide solo si la condición es verdadera. Sirve para rangos (2xx, 5xx) que no se pueden escribir como constantes.
- `s =>` en lugar de `_ =>` le da nombre al valor para usarlo en el mensaje. `_` coincide igual, pero lo descarta.
- Los mensajes con `format!("{s} on {path}")` (por ejemplo `502 Bad Gateway on /team/123/task`) son los que verás cuando algo falle contra el ClickUp real. Un mensaje vacío no ayuda a depurar.

## Paso 9: `ClickUpClient::get`

### `async` y `.await`

- **Una función `async` no se ejecuta al llamarla.** Devuelve un `Future`: una receta que todavía no se ha cocinado.

  ```rust
  let futuro = self.http.get(&url).send(); // todavía no salió nada por la red
  ```
- **`.await` ejecuta el `Future` hasta tener el resultado.** Si hay que esperar (red, temporizador, semáforo), la función se **pausa** y tokio usa el hilo para otras tareas. Cuando el resultado está listo, la función sigue en la línea siguiente.

  ```rust
  let resultado = self.http.get(&url).send().await; // Result<Response, reqwest::Error>
  ```
- **Solo se puede usar `.await` dentro de funciones `async`**, porque lo que se pausa es la función completa.

Para leer código async, piensa en `.await` como "espera el resultado aquí". El código se lee de arriba abajo, igual que el código normal.

En `get` hay cuatro esperas:

| Línea                                         | Qué espera                                    |
| ---------------------------------------------- | ---------------------------------------------- |
| `self.permits.acquire().await`               | Que haya un lugar libre en el semáforo        |
| `.send().await`                              | Que llegue la respuesta (status y headers)     |
| `tokio::time::sleep(self.retry_delay).await` | Que pase el tiempo antes de reintentar         |
| `response.json::<T>().await`                 | Que llegue el cuerpo completo y se deserialice |

### reqwest: el patrón *builder*

```rust
self.http                                   // reqwest::Client (se reutiliza entre peticiones)
    .get(&url)                              // RequestBuilder: todavía no se envía nada
    .header("Authorization", &self.token)   // RequestBuilder
    .query(query)                           // RequestBuilder: agrega ?clave=valor&...
    .send()                                 // Future<Output = Result<Response, reqwest::Error>>
    .await                                  // Result<Response, reqwest::Error>
```

Cada método configura y devuelve el builder. Solo `send()` crea el envío y solo `.await` lo ejecuta.

### Tres lugares donde puede fallar una petición

| Dónde                                                | Qué significa                                                      | Error del dominio          |
| ----------------------------------------------------- | ------------------------------------------------------------------- | -------------------------- |
| `send()` devuelve `Err`                           | La petición **no llegó**: DNS, timeout, conexión rechazada | `Network`                |
| `send()` devuelve `Ok`, pero el status es 4xx/5xx | El servidor respondió, pero con un error                           | Lo decide `status_error` |
| `response.json::<T>()` devuelve `Err`             | El cuerpo no es el JSON esperado                                    | `InvalidResponse`        |

Lo que más confunde al principio: **para reqwest, un 404 es una respuesta exitosa**. Llegó una respuesta, así que `send()` da `Ok`. Revisar el código es trabajo tuyo.

`json()` **consume** el `Response` (lo recibe por valor): lee `response.status()` antes de llamarlo.

### El flujo completo

```text
format!(url)
    │
acquire().await ──► _permit (vive hasta el final de la función)
    │
┌─► send().await ──► ¿Err? ──► return Network
│       │ Ok(response)
│       ▼
│   ¿429 y attempt < max_retries? ──sí──► attempt += 1; sleep().await; continue ─┐
│       │ no: break response                                                     │
└───────┼◄───────────────────────────────────────────────────────────────────────┘
        ▼
status_error(status) ──► ¿Some(error)? ──► return Err(error)
        │ None
        ▼
response.json::<T>().await ──► Ok(T) o InvalidResponse
        │
fin de la función: se destruye _permit y el lugar vuelve al semáforo
```

### `loop` es una expresión

En Rust, `loop` puede devolver un valor con `break valor`:

```rust
let mut attempt = 0;
let response = loop {
    let response = /* send().await.map_err(...)? */;
    if response.status() == StatusCode::TOO_MANY_REQUESTS && attempt < self.max_retries {
        attempt += 1;
        tokio::time::sleep(self.retry_delay).await;
        continue;              // vuelve al inicio del loop
    }
    break response;            // el loop "devuelve" la respuesta
};
```

- `continue` repite la petición.
- `break response` sale del bucle, y la variable `response` de afuera toma ese valor.
- El `?` dentro del `loop` sigue funcionando: un error de red sale directo de la función.

### ¿Cuántos intentos hay?

Como máximo `1 + max_retries`. Con el valor por defecto (`max_retries: 2`), son 3 peticiones:

| Petición | `attempt` antes | Si responde 429…                                                    |
| --------- | ----------------- | -------------------------------------------------------------------- |
| 1ª       | 0                 | `0 < 2`: espera `retry_delay` y reintenta                        |
| 2ª       | 1                 | `1 < 2`: espera y reintenta                                        |
| 3ª       | 2                 | `2 < 2` es falso: sale y `status_error` devuelve `RateLimited` |

En el peor caso son unos 4 segundos de espera (2 × 2 s). Solo se reintenta el 429: un error de red, un 404 o un 500 salen al primer intento. Los tests de wiremock usan un `retry_delay` corto para no esperar de verdad.

### `map_err`: convertir el error de un `Result`

`map_err` transforma **solo el error** y deja el `Ok` intacto:

```rust
Result<Response, reqwest::Error>
        │ .map_err(|e| ProviderError::Network(e.to_string()))
        ▼
Result<Response, ProviderError>
```

| Método         | Si es `Ok(v)`       | Si es `Err(e)`       |
| --------------- | --------------------- | ---------------------- |
| `.map(f)`     | `Ok(f(v))`          | `Err(e)` sin cambios |
| `.map_err(f)` | `Ok(v)` sin cambios | `Err(f(e))`          |

**¿Por qué hace falta?** Por el `?`. El `?` devuelve el error convertido al tipo de error de la función, y para eso usa `From`. `get` devuelve `ProviderError`, pero `send()` da `reqwest::Error`, y no existe `From<reqwest::Error> for ProviderError`:

```rust
.send().await?                                                     // ❌ no sabe convertir el error
.send().await.map_err(|e| ProviderError::Network(e.to_string()))? // ✅ la conversión la haces tú
```

**¿Por qué en `mapper.rs` el `?` funcionaba solo?** Porque ahí existe `impl From<MapError> for ProviderError`. Para `reqwest::Error` no se creó un `From`, por dos motivos:

1. `ProviderError` vive en `application`. Para implementar ese `From` en `application`, ese crate tendría que depender de `reqwest`, y el núcleo no debe saber nada de HTTP. Implementarlo en el adaptador tampoco se puede, por la **regla del huérfano**: ni el trait (`From`) ni el tipo (`ProviderError`) son de este crate.
2. El mismo `reqwest::Error` significa cosas distintas según dónde ocurre: en `send()` es `Network` y en `json()` es `InvalidResponse`. Con `map_err` decides en cada lugar.

La closure `|e| ProviderError::Network(e.to_string())` se lee: "recibe el error `e`, conviértelo en texto y envuélvelo en `Network`".

### Genéricos: `<T>`

Un genérico es **un tipo que se decide después**, cuando alguien usa la función o el struct. Ya los usabas sin notarlo:

```rust
Vec<T>        // Vec<Task>, Vec<String>: un solo Vec para cualquier tipo
Option<T>     // Option<DateTime<Utc>>, Option<&str>
Result<T, E>  // Result<Task, MapError>
```

En `get`:

```rust
pub(crate) async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(String, String)])
    -> Result<T, ProviderError>
```

| Parte                  | Significado                                                                                                                               |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `<T>`                | `get` sirve para cualquier tipo `T`                                                                                                   |
| `: DeserializeOwned` | Restricción (*trait bound*): solo tipos que serde sepa construir desde JSON. Tus `Api*` cumplen gracias a `#[derive(Deserialize)]` |
| `Result<T, ...>`     | Devuelve el mismo `T` que eligió quien llama                                                                                           |

Sin genéricos habría que escribir `get_user`, `get_tasks_page`, `get_task` y `get_comments`, todas con el mismo código HTTP.

**Quien llama decide qué es `T`**, y casi siempre Rust lo infiere del tipo de la variable:

```rust
let me: ApiUserEnvelope = self.client.get("/user", &[]).await?;       // T = ApiUserEnvelope
let page: ApiTasksPage = self.client.get(&path, &params).await?;     // T = ApiTasksPage
```

Si no hay una variable con tipo, se indica con el *turbofish* `::<>`, como en `parse::<i64>()`:

```rust
self.client.get::<ApiTask>(&path, &[]).await?
```

Dentro de `get`, `response.json::<T>()` le pasa ese mismo `T` a serde. El tipo "viaja" desde la variable en `provider.rs`, pasa por `get` y llega hasta serde.

**Monomorfización:** al compilar, Rust genera una copia de `get` por cada `T` que se usa (una para `ApiUserEnvelope`, otra para `ApiTasksPage`...). Por eso los genéricos no tienen costo en tiempo de ejecución, a diferencia de `dyn Trait`, que decide en tiempo de ejecución (fase 2, `Arc<dyn TaskProvider>`).

### `acquire`: pedir un lugar en el semáforo

Piensa en el `Semaphore` como un estacionamiento con 8 lugares (`max_concurrent_requests`):

```rust
let _permit = self
    .permits
    .acquire()
    .await
    .expect("the semaphore is never closed");
```

| Pieza            | Qué hace                                                                                                                                      |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `acquire()`    | Pide un lugar. Devuelve un `Future`: todavía no pasa nada                                                                                   |
| `.await`       | Si hay lugar, lo toma al instante. Si los 8 están ocupados, la función se pausa hasta que alguno se libere, sin bloquear el hilo             |
| `.expect(...)` | `acquire` da `Result<SemaphorePermit, AcquireError>`, que solo falla si alguien llamó a `close()` sobre el semáforo. Nunca lo cerramos |
| `_permit`      | El permiso: tu "ticket de estacionamiento"                                                                                                     |

**No existe un `release()`.** El lugar se libera cuando `_permit` se destruye, al salir de la función por cualquier camino: `Ok`, `return Err` o un `?`. Este patrón se llama **RAII** (el recurso dura lo que dura la variable). Es imposible olvidarse de liberarlo.

Por eso importa el nombre de la variable:

```rust
let _permit = ...;  // ✅ vive hasta el final de get: limita de verdad
let _ = ...;        // ❌ se destruye en esa misma línea: el semáforo no limita nada
```

**¿Para qué sirve?** En el paso 10, `search_tasks` por ids lanza muchas peticiones a la vez con `try_join_all`. Con 50 ids, sin semáforo saldrían 50 peticiones simultáneas y ClickUp respondería 429. Con el semáforo hay como máximo 8 en curso, y las demás esperan su turno en `acquire().await`. La capa de aplicación no sabe nada de esto: el límite vive en el adaptador (NFR-07).

### Otros detalles de esta implementación

- **`format!("{}{path}", self.base_url)` y no `self.base_url + path`.** El operador `+` sobre `String` **consume** el `String` de la izquierda, y no puedes mover `self.base_url` porque `self` está prestado (`&self`). `format!` crea un `String` nuevo sin tocar el original.
- **La URL se calcula antes del `loop`**, porque es la misma en cada reintento.
- **Un `;` al final de `.send().await;` descarta el resultado.** La petición se hace, pero la respuesta se pierde. Siempre guárdala en una variable o devuélvela.

## Paso 10: `ClickUpProvider`

### Construir una vez, usar muchas veces

Hay dos momentos distintos, y mezclarlos es el error más común:

| Momento             | Qué pasa                                                                                               | Quién lo hace                                                                                                                                        |
| ------------------- | ------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Construir** | `ClickUpProvider::new(config)` crea el cliente HTTP, el semáforo y guarda `team_id` y `statuses` | Quien **usa** el provider, una sola vez: `provider(&server)` en los tests, `main` en `examples/today.rs`, y `bootstrap` desde la fase 4 |
| **Usar**      | `provider.current_user().await`, `provider.search_tasks(...)`                                       | Los casos de uso, a través de `Arc<dyn TaskProvider>`                                                                                              |

Dentro de un método, el provider **ya existe**: eso es `self`. No hay `config` disponible, y no hace falta, porque todo lo que venía de ella ya está guardado en `self.client`, `self.team_id` y `self.statuses`.

```rust
async fn current_user(&self) -> Result<UserId, ProviderError> {
    let provider = ClickUpProvider::new(config)?;   // ❌ `config` no existe aquí
    let envelope: ApiUserEnvelope = self.client.get("/user", &[]).await?;   // ✅
    // ...
}
```

Además, construir un provider en cada llamada crearía un cliente HTTP y un semáforo nuevos cada vez: el límite de 8 peticiones simultáneas dejaría de funcionar, porque cada llamada tendría su propio semáforo.

Es como un auto: `new` lo fabrica y los métodos lo manejan. No fabricas un auto nuevo cada vez que sales.

### La receta de los cuatro métodos

```text
1. Armar el path           →  format!("/task/{}", id.as_str())
2. Pedirlo con el cliente  →  let api: TipoApi = self.client.get(&path, &params).await?;
3. Traducir al dominio     →  map_task(...)?, user_id(...)?, Comment::try_from...
4. Devolver                →  Ok(...)
```

- El **tipo de la variable** en el paso 2 decide la `T` de `get` (ver "Genéricos" en el paso 9).
- `&[]` es un slice vacío: "sin parámetros de query".
- Los `?` del paso 3 funcionan gracias a `impl From<MapError> for ProviderError`.

### `Ok(user_id(&envelope.user)?)`: desenvolver y volver a envolver

`user_id` devuelve `Result<UserId, MapError>`, pero el método devuelve `Result<UserId, ProviderError>`. No se puede devolver tal cual porque el tipo de error es otro:

- `?` desenvuelve el `UserId` y, si hay error, lo convierte a `ProviderError` con `From`.
- `Ok(...)` vuelve a envolver el `UserId` en el `Result` correcto.

### Búsqueda por ids: el provider se usa a sí mismo

```rust
if !query.ids.is_empty() {
    return try_join_all(query.ids.iter().map(|id| self.task(id))).await;
}
```

- `self.task(id)` es el método del mismo provider. Así la lógica de "pedir una tarea" vive en un solo lugar.
- `try_join_all` lanza todos los `Future` a la vez y devuelve los resultados **en el mismo orden** que los ids, no en el orden en que terminan. Si uno falla, devuelve ese error.
- No hace falta limitar la concurrencia aquí: lo hace el semáforo dentro de `get`.
- El `return` temprano evita un `else` gigante con la paginación adentro.

### Paginación: un `loop` que acumula

```rust
let mut tasks = Vec::new();
let mut page = 0;
loop {
    let params = task_search_params(query, &self.statuses, page);
    let response: ApiTasksPage = self.client.get(&path, &params).await?;

    let count = response.tasks.len();          // antes del for
    for api_task in response.tasks {           // el for mueve el Vec
        tasks.push(map_task(api_task, &self.statuses)?);
    }

    if response.last_page.unwrap_or(count < PAGE_SIZE) {
        break;
    }
    page += 1;
}
Ok(tasks)
```

- **`count` se calcula antes del `for`.** `for api_task in response.tasks` *mueve* el `Vec` (es un `into_iter` implícito). Después ya no existe y no puedes llamar a `.len()`. A cambio, cada `ApiTask` llega por valor y `map_task` puede mover sus `String` sin clonar.
- **`last_page.unwrap_or(count < PAGE_SIZE)`:** si ClickUp dice si es la última página, se le cree. Si no lo dice (`None`), se asume que es la última cuando llegaron menos de 100 tareas.
- A diferencia del `loop` de reintentos, aquí `break` no lleva valor: el resultado se va guardando en `tasks`.

### Un 404 en los tests: leer el error antes de buscar en el código

```text
called `Result::unwrap()` on an `Err` value: NotFound("/team/9001/tasks")
```

1. `NotFound` sale de `status_error`, así que el servidor respondió 404.
2. wiremock responde **404 a cualquier ruta que no tenga registrada**. El test registró `/team/9001/task` y el código pidió `/team/9001/tasks`.
3. El path del error es el que pidió tu código, gracias a `NotFound(path.to_string())` del paso 8.

El compilador no puede saber que la ruta correcta es `task` y no `tasks`: para él las dos son texto válido. Los tests de wiremock existen justamente para comprobar que el código habla el mismo "idioma" que la API (rutas, headers, parámetros), sin tocar ClickUp real.

## Cerrar la fase: el bloque `#![allow(...)]`

### Qué era y por qué existía

```rust
#![allow(
    unused_variables,
    dead_code,
    clippy::unused_async,
    clippy::needless_pass_by_value
)]
```

`#![allow(...)]` apaga esos avisos en **todo el crate**. El `!` hace que el atributo se aplique al elemento que lo contiene, en este caso el crate entero desde `lib.rs`. Sin `!` (`#[allow(...)]`) se aplica solo al elemento que viene justo después.

Con las firmas y cuerpos `todo!()`, el esqueleto disparaba justo esos avisos:

| Lint                               | Por qué saltaba con `todo!()`                                          |
| ---------------------------------- | ------------------------------------------------------------------------- |
| `unused_variables`               | Los parámetros (`id`, `query`...) no se usaban                       |
| `dead_code`                      | `map_task`, `task_search_params`... existían, pero nadie los llamaba |
| `clippy::unused_async`           | Funciones `async` sin ningún `.await` adentro                        |
| `clippy::needless_pass_by_value` | Parámetros por valor que no se consumían                                |

Como el CI ejecuta clippy con `-D warnings` (todo aviso es error), sin el bloque el esqueleto no habría pasado el CI aunque estuviera bien encaminado.

### Por qué se borra al terminar

Con el código terminado, esos avisos detectan **errores reales**: un parámetro que olvidaste usar (por ejemplo, filtros que se ignoran sin que nadie lo note), una función que nadie llama, un `async` innecesario o un valor tomado por dueño sin motivo. Dejar el bloque los silenciaría para siempre.

Por eso estaba marcado con `TODO(fase 3)`: era un andamio temporal a propósito. Patrón general: cuando silencies un aviso, hazlo en el lugar más pequeño posible (una función, no el crate) y deja escrito por qué y hasta cuándo.

## Paso 11: contra el ClickUp real

### "error decoding response body": el error que no dice nada

El primer intento contra la cuenta real dio:

```text
Error: Provider(InvalidResponse("error decoding response body"))
```

Lo que **sí** se podía deducir:

- `Provider(...)`: falló el adaptador, no la lógica de la aplicación.
- `InvalidResponse`: falló el último paso de `get`, la deserialización. Así que la red, el token y el status estuvieron bien: si no, el error habría sido `Network` o `Unauthorized`.
- Conclusión: ClickUp respondió 200, pero el JSON real no encaja con algún struct de `api_types.rs`.

Lo que **no** decía: qué endpoint, qué campo ni por qué.

### Por qué se perdía el detalle: la cadena de errores

serde sí sabía exactamente qué falló. Pero `response.json()` envuelve el error de serde dentro de un `reqwest::Error`, y el `to_string()` de reqwest solo muestra su propio mensaje genérico. El error de serde queda guardado como su **causa** (`source()`), y `to_string()` no la incluye.

En Rust los errores pueden encadenarse así: cada error puede tener una causa, y esa causa otra. `Display` (`to_string()`) normalmente muestra **solo el primer eslabón**.

### La solución: deserializar en dos pasos

```rust
let body = response
    .text()
    .await
    .map_err(|e| ProviderError::Network(e.to_string()))?;

serde_json::from_str::<T>(&body)
    .map_err(|e| ProviderError::InvalidResponse(format!("{path}: {e}")))
```

| Paso                          | Qué hace                                        | Si falla                                                                |
| ----------------------------- | ------------------------------------------------ | ----------------------------------------------------------------------- |
| `response.text().await`     | Descarga el cuerpo como texto, sin interpretarlo | El cuerpo no llegó completo: `Network`                               |
| `serde_json::from_str::<T>` | Lo mismo que hacía `json()` por dentro        | `InvalidResponse` con el mensaje **directo** de serde y el path |

Con eso el mismo fallo pasó a decir:

```text
/task/86bcdb227/comment: invalid value: integer `-4270804`, expected u64 at line 1 column 3633
```

Endpoint, valor recibido, tipo esperado y posición exacta en el JSON. Lección general: **un mensaje de error es parte del diseño**. Invertir dos líneas en que diga *dónde* y *qué* ahorra horas de depuración.

### Ids negativos: `u64` contra `i64`

| Tipo    | Rango                          | Significado                                 |
| ------- | ------------------------------ | ------------------------------------------- |
| `u64` | 0 … ~1.8×10¹⁹              | *unsigned*: sin signo, solo 0 o positivos |
| `i64` | ~−9.2×10¹⁸ … ~9.2×10¹⁸ | *signed*: con signo                       |

ClickUp envía ids negativos para usuarios que no son personas (automatizaciones, integraciones). La documentación no lo menciona y el fixture solo tenía personas, así que `ApiUser.id: u64` rechazaba el comentario. serde es estricto: no convierte un número fuera de rango, falla.

La corrección fue cambiar una palabra (`u64` → `i64`), y **nada más cambió** en el resto del sistema:

- `user_id` hace `user.id.to_string()`, y `UserId` es texto.
- El dominio nunca supo que los ids de ClickUp eran números.

Ese es el beneficio concreto de la capa anticorrupción (`api_types` + `mapper`) y del newtype `UserId(String)`: una sorpresa de la API se arregla en el adaptador, sin tocar el dominio ni los casos de uso.

### Primero el test, después la corrección

Ante un fallo con datos reales, el orden fue:

1. **Leer el error** y entender qué dato lo causa.
2. **Escribir un test que falle igual** (`comments_from_system_users_have_negative_ids`, con el JSON mínimo que reproduce el caso).
3. Comprobar que **falla**: así sabes que el test reproduce el problema y no pasa por casualidad.
4. **Corregir** (`i64`) y ver el test en verde.

Si solo se corrige el tipo, el arreglo depende de que ese comentario siga existiendo en tu cuenta. Con el test, el caso queda cubierto para siempre: si alguien vuelve a poner `u64`, el test lo detecta.

Es la diferencia entre fixtures escritos **a partir de la documentación** (lo que la API dice que envía) y casos **a partir de la realidad** (lo que de verdad envía). Los segundos se agregan a medida que aparecen.

## Resumen de la fase 3

| Concepto                                                    | Dónde lo usaste                            |
| ----------------------------------------------------------- | ------------------------------------------- |
| `serde::Deserialize`, `rename`, `default`, `Option` | `api_types.rs`                            |
| Capa anticorrupción (structs de la API + mapper)           | `api_types.rs`, `mapper.rs`             |
| `TryFrom` y regla del huérfano                           | `Comment::try_from`                       |
| `Option::filter`, `as_ref`, `as_deref`, `transpose` | `map_location`, `map_task`, comentarios |
| Mover contra clonar, `pub(crate)`                         | `map_task`, `map_location`              |
| `match` con constantes, `                                 | ` y guardas                                 |
| `async`/`.await`, `Future`                            | `get`, todo `provider.rs`               |
| Builder de reqwest, tres niveles de error                   | `get`                                     |
| `loop` como expresión (`break valor`)                  | Reintentos en `get`                       |
| `loop` acumulador                                         | Paginación en `search_tasks`             |
| `map_err` contra `From` + `?`                         | `get` frente a `mapper.rs`              |
| Genéricos, *trait bounds*, inferencia, *turbofish*     | `get<T: DeserializeOwned>`                |
| `Semaphore`, RAII, `_permit` contra `_`               | `get`                                     |
| `try_join_all` y orden de resultados                      | Búsqueda por ids                           |
| Tests de integración con wiremock                          | `tests/provider.rs`                       |
| Cadena de errores, mensajes útiles                         | `text()` + `serde_json::from_str`       |
| `u64` contra `i64`, test primero ante datos reales      | Ids negativos en comentarios                |
