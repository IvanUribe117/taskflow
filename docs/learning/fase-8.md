# Fase 8: la sesión de trabajo

Objetivo del roadmap (§24): *`start` → reporte → `approve` → `done` con el contador de ClickUp, estados sugeridos y validados por lista, publicación explícita del reporte y notificación de escritorio.*

Esta fase la implementé completa, a pedido tuyo ("de una vez lo atacamos"). Esta guía explica **cómo quedó y por qué**, capa por capa, con los conceptos nuevos de Rust al final.

```sh
taskflow start 86bcjdn6v            # contador + análisis con IA + estado sugerido "listo"
taskflow session                    # dónde vas: estado, tiempo, reporte, qué sigue
taskflow report show                # el reporte (sessions/<id>/report.md; puedes editarlo)
taskflow reject "falta el caso X"   # otro análisis, con tu nota
taskflow approve                    # → estado sugerido "en progreso" (o --status "code review")
taskflow pause / resume             # detiene y reanuda el contador
taskflow session status blocked     # cambia el estado de la tarea sin cambiar la sesión
taskflow report publish             # lo publica como comentario (pregunta antes)
taskflow done                       # estado final + comentario + contador detenido (como close)
taskflow abort --restore-status     # cancela y devuelve la tarea a su estado original
taskflow session retry              # si algo falló a mitad, sigue desde ahí
```

---

## 0. Lo que se resolvió antes de empezar

| Pendiente | Qué se hizo |
| --- | --- |
| Probar la fase 7 contra ClickUp real | Se creó la tarea [[taskflow] Tarea de prueba de Ivan — ignorar](https://app.clickup.com/t/86bcjdn6v) en IDK Backlog. `close --status "needs approval" --comment ... --yes` cambió el estado y publicó el comentario; un estado inválido dio el error con la lista de estados válidos. **La fase 7 queda probada.** |
| Endpoints del contador de ClickUp | Verificados en la documentación oficial: `GET /team/{id}/time_entries/current`, `POST .../start` con `{"tid": ...}` y `POST .../stop`. `current` también se probó en modo solo lectura contra tu cuenta. La documentación **no** dice qué pasa sin contador; ver la sección 4. |
| El estado original exacto | `Task.status` es el estado **normalizado** ("to do") y perdía el nombre real ("Open"). Para que `abort --restore-status` pueda volver a él, `Task` ahora tiene `status_name`, el nombre exacto que manda el proveedor. |

---

## 1. El flujo completo

```text
taskflow start 86b1
  │
  ├─ StartSession::check ───► ¿sesión activa?  ¿contador en OTRA tarea? → preguntar
  │
  └─ StartSession::execute
       │  WorkSession::start(task) ──► (sesión, [StartTimer, Analyze])        ← dominio: decide
       │
       └─ runner::run(sesión, efectos)                                         ← aplicación: ejecuta
            ├─ StartTimer ──────────────► TimeTracker::start ─────────────► POST time_entries/start
            ├─ Analyze ─────────────────► SummarizeTask → report.md
            │     └─ session.handle(AnalysisReady) ──► [SetStatus(Suggested(Ready)), Notify]
            │                                           (se ponen al frente de la cola)
            ├─ SetStatus(Suggested) ────► primer nombre de status_map.ready que exista ─► PUT /task
            └─ Notify ──────────────────► MultiNotifier → osascript
       guardar sessions/active.json
```

Lo importante: **el dominio no hace nada, decide.** `WorkSession::handle(comando)` devuelve una lista de `Effect` (datos), y `application` los ejecuta en orden con los puertos. Por eso toda la lógica de transiciones se prueba con datos, sin red, sin reloj y sin archivos.

---

## 2. Dominio: la máquina de estados (`crates/domain/src/session/`)

```text
 start ─► Analyzing ──AnalysisReady/Failed──► AwaitingApproval ──approve──► InProgress ──done──► Closed
              ▲                                      │                     pause │ ▲ resume
              └──────────────reject──────────────────┘                          ▼ │
                                                                               Paused ──done──► Closed
 En cualquier estado no terminal: status (cambiar estado) · publish · abort ─► Aborted
```

| Archivo | Qué tiene |
| --- | --- |
| `state.rs` | `SessionState` (con datos: `Analyzing { note }`, `Paused { since }`), `ReportOutcome` (`Ready` / `Unavailable { reason }`), `Publication` (`Draft` / `Published { at }`) |
| `command.rs` | `SessionCommand`: lo que pides (`Approve { status }`, `Finish { status, comment }`...) |
| `effect.rs` | `Effect` (lo que hay que hacer), `StatusTarget` (`Suggested(Stage)`, `Explicit(name)`, `Original`), `CommentKind`, `NoticeKind`; `is_critical()` |
| `mod.rs` | `WorkSession`, `Interval`, `SessionError` y `handle` |

### `handle`: un `match` sobre una tupla

```rust
let (next, effects) = match (&self.state, command) {
    (S::Analyzing { .. }, C::AnalysisReady) => { /* ... */ (S::AwaitingApproval, vec![...]) }
    (S::AwaitingApproval, C::Approve { status }) => { /* ... */ }
    (S::InProgress, C::Pause) => { self.close_interval(now); (S::Paused { since: now }, vec![E::StopTimer]) }
    (state, C::ChangeStatus { status }) if !state.is_terminal() => (state.clone(), vec![...]),
    (state, C::PublishReport) if !state.is_terminal() => (state.clone(), self.publish()?),
    (S::InProgress | S::Paused { .. }, C::Finish { status, comment }) => (S::Closed, self.finish(...)),
    (state, C::Abort { restore_status }) if !state.is_terminal() => { /* ... */ }
    (state, command) => return Err(SessionError::InvalidTransition {
        from: state.name(), command: command.name(), allowed: state.allowed_commands(),
    }),
};
self.state = next;
Ok(effects)
```

- **Cada combinación válida es una rama.** La última atrapa todo lo demás y explica qué se permite: `cannot 'done' while the session is 'awaiting_approval'; allowed: approve, reject, status, publish, abort`.
- **`self.state = next` va al final**: si el comando no vale, se devuelve el error antes y la sesión queda **igual** (hay un test que lo comprueba con `assert_eq!(session, before)`).
- **No existe `Stage::Final`**: `Finish` exige un `String` con el estado. El tipo hace imposible cerrar sin haber elegido uno.

### El tiempo: intervalos

`intervals: Vec<Interval { start, end: Option }>`. `start` y `resume` abren uno; `pause`, `done` y `abort` lo cierran. `elapsed(now)` suma los tramos, y los que siguen abiertos cuentan hasta `now`. Así las pausas no cuentan, y el tiempo es correcto aunque el proveedor no tenga contador.

`stop_timer` solo pide `StopTimer` si el contador corre: si terminas desde una pausa, no se detiene lo que ya estaba detenido.

### Lo pendiente: `pending`

`pending: Vec<Effect>` guarda lo que no se aplicó porque falló algo **crítico** (NFR-21). `session retry` lo reintenta. Es la versión simple del "diario de efectos" del diseño.

---

## 3. Aplicación (`crates/application/src/session/`, `ports/`)

### Tres puertos nuevos

| Puerto | Métodos | Implementaciones |
| --- | --- | --- |
| `TimeTracker` | `start(task)`, `stop()`, `current()` | `ClickUpTimeTracker`; `LocalTimeTracker` (objeto nulo) |
| `Notifier` | `notify(&Notice)` | `MacNotifier`; `MultiNotifier` (*composite*); `NoopNotifier` |
| `SessionStore` | `active`, `save`, `save_report`, `load_report`, `archive(session)`... | `FsSessionStore` |

Y en `AppContext`: `time`, `notifier`, `sessions`. Cada uno tiene su fake en `testing.rs`.

### El ejecutor de efectos (`runner.rs`)

```rust
let mut queue: VecDeque<Effect> = effects.into();
while let Some(effect) = queue.pop_front() {
    session.pending = /* este efecto + lo que falta */;
    ctx.sessions.save(&session).await?;            // si se corta aquí, retry sabe por dónde seguir
    match apply(ctx, prefs, &mut session, &effect).await {
        Ok(step) => { /* avisos; step.then va al FRENTE de la cola */ }
        Err(reason) if !effect.is_critical() => warnings.push(...),   // notificar: solo aviso
        Err(reason) => return Err(AppError::EffectFailed { ... }),     // crítico: se detiene
    }
}
```

| Efecto | Qué hace | Si falla |
| --- | --- | --- |
| `StartTimer` | Si ya corre en esta tarea, nada; si no, `start` | Crítico |
| `StopTimer` | Solo detiene si el contador es **de esta tarea**; si iniciaste otro a mano, lo deja y avisa | Crítico |
| `Analyze` | `SummarizeTask` → `report.md` + `agent-prompt.md` → `handle(AnalysisReady)`. Si la IA falla → `handle(AnalysisFailed)`: la sesión sigue sin reporte | Nunca corta por la IA |
| `SetStatus(Suggested)` | El primer nombre de `status_map.ready`/`in_progress` que exista en la lista; si ninguno existe, **aviso** y no cambia nada | Crítico solo si falla ClickUp |
| `SetStatus(Explicit/Original)` | Debe existir en la lista | Crítico |
| `PostComment(Report)` | Lee `report.md` **del disco** (con tus ediciones) y lo publica | Crítico |
| `MarkReportPublished` | Marca la fecha; va siempre **después** del comentario | — |
| `Notify` | Aviso de escritorio | **No crítico** |

**Efectos que generan efectos:** `Analyze` le pasa el resultado a la sesión (`handle(AnalysisReady)`), que devuelve `[SetStatus, Notify]`. Esos van **al frente** de la cola, en orden (`rev` + `push_front`).

### Los casos de uso

| Caso de uso | Qué hace |
| --- | --- |
| `StartSession::check` / `execute` | En dos pasos, como `close`, porque la interfaz pregunta en medio (¿detener el contador ajeno?). Una sesión a la vez. |
| `HandleSession::execute(command)` | Cualquier comando. Antes de tocar nada, busca los nombres de estado en la lista de la tarea y los cambia por el exacto (`"CODE REVIEW"` → `"code review"`). Si hay algo pendiente, pide `retry` primero. |
| `RetrySession` | Aplica lo pendiente; si no hay nada y la sesión quedó en `Analyzing` (cortaste con Ctrl+C), vuelve a analizar. |
| `ShowSession` | La vista y el reporte del disco. |

`SessionPrefs` reúne lo que la sesión toma de la configuración: `status_map.ready`, `status_map.in_progress` y los límites de la IA.

### El reporte (`report.rs`)

`report.md` lleva qué se pide, el plan y las preguntas: es lo que se **publica**. El prompt para el agente va aparte, en `agent-prompt.md`, porque es para ti y no para quien lea la tarea. La nota de `reject` llega a la IA como `<user_feedback>` al final del mensaje (`SummaryInput::note`).

---

## 4. Adaptadores

### `FsSessionStore` (`adapters/fs/src/sessions.rs`)

```text
sessions/
├── active.json                      la sesión activa, en JSON legible
├── 86bcjdn6v/report.md              edítalo antes de publicar
├── 86bcjdn6v/agent-prompt.md
└── archive/2026-10-14_204310_86bcjdn6v.json
```

`archive` **primero escribe** el archivo y **después** borra `active.json`: si algo falla en medio, la sesión sigue activa y no se pierde.

### `ClickUpTimeTracker` (`adapters/clickup/src/time.rs`)

La documentación no dice qué responde `current` sin contador (en la prueba real: `{"data": null}`). Por eso `data` se acepta **ausente, `null` o `{}`**, y `stop` pregunta primero con `current` si hay algo que detener: nunca hace el POST a ciegas. `start` llega como texto (`"1595293042560"`), pero también se acepta como número. Tiene tests unitarios del JSON y tests HTTP con `wiremock` (`tests/time.rs`).

### `MacNotifier` (crate nuevo `adapters/desktop`)

```text
osascript -e 'on run argv' -e 'display notification (item 2 of argv) with title (item 1 of argv)' -e 'end run' "<título>" "<texto>"
```

El título y el texto van como **argumentos**, no pegados dentro del código `AppleScript`. Un título de tarea con comillas no puede romper el guion ni inyectar nada.

### `bootstrap`

- `TaskProviderDescriptor` tiene `build_time: Option<TimeBuildFn>`. ClickUp lo trae; un proveedor sin contador pondría `None`.
- `time`: el del proveedor si existe y `session.track_time = true`; si no, `LocalTimeTracker`.
- `notifier`: un `MultiNotifier` con los canales activos (hoy, el escritorio en macOS si `session.notify`).

### Configuración nueva

```toml
[session]
dir = "sessions"     # relativo a la carpeta de config.toml, o absoluto
track_time = true    # usar el contador del proveedor
notify = true        # notificaciones de escritorio
```

---

## 5. CLI

- `commands/session.rs`: `start`, `handle` (approve, reject, pause, resume, status), `done` (reutiliza `PrepareClose` y `status_choice` de `close`), `abort`, `session` y `report`. Aquí solo se pregunta y se muestra.
- `presenter::session`: estado, tiempo, reporte, publicado, pendiente y **qué puedes hacer ahora** según el estado.
- `commands::run` se dividió: la configuración y el `AppContext` se cargan **una vez** y `with_context` despacha. Antes cada rama repetía esas dos líneas, y con nueve comandos nuevos la función pasaba el límite de clippy.

---

## 6. Conceptos aplicados

- **Máquina de estados con `match` sobre una tupla.** `match (&self.state, command)` combina el estado y el comando en un solo patrón. Las guardas (`if !state.is_terminal()`) y los patrones con `|` (`S::InProgress | S::Paused { .. }`) evitan repetir ramas. El compilador obliga a que el `match` sea exhaustivo: la rama final `(state, command)` es la de "no permitido".
- **Efectos como datos (*functional core, imperative shell*).** `handle` no llama a ClickUp: devuelve `Vec<Effect>`. Se prueba comparando listas (`assert_eq!(effects, [...])`). Quien ejecuta, `runner`, es la única parte con I/O.
- **Enums con datos en vez de `bool`.** `Publication::Published { at }` en lugar de `published: bool` + `published_at: Option`: no puede haber "publicado sin fecha". Lo mismo con `ReportOutcome::Unavailable { reason }`.
- **`serde` con etiqueta interna.** `#[serde(tag = "state", rename_all = "snake_case")]` guarda `{"state": "paused", "since": "..."}`; las variantes sin datos quedan como `{"state": "in_progress"}`. Para `StatusTarget(Stage)`, que es una variante *newtype*, se usa `tag` + `content`.
- **Deserializar validando.** `TaskId` y `ProjectSlug` implementan `Serialize`/`Deserialize` a mano: se guardan como texto y al leerlos pasan por `TaskId::new` / `parse`. Un `active.json` con un id vacío da un error, nunca un `TaskId` inválido ("parse, don't validate", también al leer archivos).
- **`<String as serde::Deserialize>::deserialize(d)`.** Dentro de `impl Deserialize for TaskId`, el trait no está importado. Esa sintaxis (*fully qualified*) dice a qué trait pertenece el método sin un `use`.
- **Ownership como regla de negocio: `archive(session: WorkSession)`.** Recibe la sesión **por valor**: después de archivarla ya no existe para quien llamó, y el compilador impide seguir usándola. En `runner::finish` por eso se arma la vista **antes** de archivar.
- **`Option::then`.** `self.timer_running().then(|| { self.close_interval(now); Effect::StopTimer })` devuelve `Some(efecto)` solo si corría; `effects.extend(opción)` agrega cero o uno, porque `Option` es iterable.
- **`VecDeque` como cola.** `pop_front` saca el siguiente; `push_front` en orden inverso (`rev`) pone los efectos nuevos al frente **conservando su orden**.
- **Crítico o no: `is_critical()`.** Una notificación que falla no debe impedir cerrar la tarea. La decisión vive en el dominio (`Effect::is_critical`) y el ejecutor solo la consulta.
- **Objeto nulo (`LocalTimeTracker`, `NoopNotifier`).** En vez de `Option<Arc<dyn TimeTracker>>` y un `if let` en cada uso, una implementación que no hace nada. El ejecutor no sabe si hay contador real.
- **Composite + `join_all` (`MultiNotifier`).** Varios canales se ven como uno. `join_all` los ejecuta a la vez y espera a **todos**, y después se devuelve el primer error. A diferencia de `try_join_all`, un canal caído no cancela a los demás.
- **Puntero a función opcional en el descriptor.** `build_time: Option<TimeBuildFn>`: el registro de proveedores dice qué sabe hacer cada uno, sin `if proveedor == ClickUp`.
- **Segregación de interfaces.** `TimeTracker` no se agregó a `TaskProvider`: GitHub Issues no tiene contador, y no debería tener que implementar métodos que no usa.
- **Dos pasos cuando la interfaz pregunta en medio.** `StartSession::check` + `execute`, igual que `PrepareClose` + `CloseTask` en la fase 7: la aplicación no pregunta, devuelve lo necesario para que la interfaz lo haga.
- **Argumentos en vez de interpolar.** `osascript ... on run argv` recibe los textos como argumentos del proceso. Es la misma idea que las consultas SQL con parámetros: los datos nunca se mezclan con el código.
- **`clippy::too_many_lines`.** `handle`, `apply` y `commands::run` pasaron las 100 líneas. Se extrajeron `publish`, `finish`, `stop_timer`, `analyze` y `set_status`. Cada uno tiene un nombre que explica qué hace, y el `match` principal quedó como un índice.

---

## 7. Probado contra ClickUp real (2026-10-14)

Sobre [la tarea de prueba](https://app.clickup.com/t/86bcjdn6v), con una copia de tu configuración, `session.track_time = false` (para no tocar tu contador) y `ai.model = haiku`:

| Paso | Resultado |
| --- | --- |
| `start` | Reporte en ~9 s. Aviso: `ningún estado de provider.status_map.ready (preparado) existe en esta lista` |
| `start` otra vez | `ya hay una sesión activa` |
| `pause` antes de aprobar | `cannot 'pause' while the session is 'awaiting_approval'; allowed: ...` |
| `approve`, `session status blocked`, `pause`, `resume` | La tarea pasó a "blocked" en ClickUp |
| `report publish --yes` (dos veces) | Comentario publicado; la segunda vez, `already published` |
| `done --status "needs approval" --comment ... --yes` | Comentario, estado, sesión archivada en `sessions/archive/` |
| `start` → `approve --status "in progress"` → `abort --restore-status` | La tarea volvió a "needs approval", su estado al empezar |
| `start` → `reject "Agrega un paso para borrar la tarea..."` | El nuevo reporte incluye ese paso |

**Contador real de ClickUp** (`track_time = true`), probado sin contadores tuyos corriendo:

| Paso | ClickUp |
| --- | --- |
| Antes | `GET current` → `{"data":null}`: así responde sin contador (no estaba documentado) |
| `start` | Corre en la tarea de prueba |
| `approve` + `pause` | Sin contador |
| `resume` | Corre otra vez |
| `done` | Sin contador; la sesión quedó archivada |

---

## 8. Ajustes que te sugiero

- **Tu `status_map.ready` no existe en las listas de IDK** ("preparado"). Si quieres que la sesión mueva la tarea al tener el reporte, configura un estado que sí exista. Por ejemplo:

  ```sh
  taskflow config set provider.status_map.ready '["needs approval"]'
  ```

  Si prefieres que no la mueva hasta que apruebes, déjalo así: solo verás el aviso.

## 9. Limitaciones conocidas

- **El comentario se publica como texto:** ClickUp no interpreta el Markdown de `comment_text`, así que los `##` se ven tal cual.
- **Una sesión a la vez**, como el contador de ClickUp.
- **Sin ganchos (`RunHook`)**: llegan con los Playbooks (fase 10).
- **El reporte no se guarda en el historial** (`history`): vive en `sessions/<id>/` y la sesión archivada guarda el estado. Si te sirve, se puede sumar.

## 10. Archivos

| Capa | Archivos |
| --- | --- |
| Dominio | `domain/src/session/{mod,state,command,effect}.rs`; `Task::status_name`; serde de `TaskId` y `ProjectSlug` |
| Aplicación | `ports/{time_tracker,notifier,session_store}.rs`; `session/{mod,runner,report,tests}.rs`; `dto.rs` (`SessionView`, `SessionOutcome`, `StartCheck`, `TimerView`); `error.rs` (`Session`, `EffectFailed`); `settings` (`[session]`); `SummaryInput::note`; fakes |
| Adaptadores | `fs/src/sessions.rs`; `clickup/src/time.rs` + `tests/time.rs`; crate `adapters/desktop`; `claude-code/src/prompts.rs` (`<user_feedback>`) |
| Bootstrap | `build_time` en el descriptor; `time_tracker`, `notifier`, `session_dir` |
| CLI | `args.rs`, `commands/session.rs`, `commands/mod.rs` (`with_context`), `presenter.rs` |

Tests: de 354 a 415.

---

## Ajuste posterior: el contador detenido fuera de Taskflow

Pasó en tu primera prueba (2026-10-14). Después de `start`, detuviste el contador a mano en ClickUp. La sesión siguió creyendo que corría, y `resume` no servía para reanudarlo: solo vale desde una pausa.

- **`taskflow session` compara con el proveedor.** Si la sesión cree que el contador corre y el proveedor no lo tiene en esta tarea, muestra:
  `⚠ El contador no corre en el proveedor (¿lo detuviste fuera de Taskflow?). Para reanudarlo: taskflow resume`
- **`taskflow resume` con la sesión corriendo** no es una transición. `HandleSession` aplica solo `StartTimer`, que no hace nada si el contador ya corre en esta tarea. Desde una pausa sigue siendo la transición normal.
- **`TimeTracker::is_local()`** (por defecto `false`; `LocalTimeTracker` devuelve `true`). Con el contador local, `current` siempre es `None` y no sirve para comparar, así que no se compara.

**Concepto: métodos con implementación por defecto en un trait.** `fn is_local(&self) -> bool { false }` vive en el trait: los adaptadores existentes no cambian, y solo el objeto nulo lo sobrescribe.

**Límite:** el tiempo que muestra `session` sigue contando mientras el contador estuvo detenido fuera de Taskflow, porque la sesión no sabe cuándo lo detuviste. El tiempo real queda en ClickUp.

---

## Segunda parte: lo que faltaba de la fase 8 y `taskflow agent`

Al revisar los requisitos de la sesión (FR-52 a FR-72), encontré varios que se me habían pasado. Los completé, y agregué `taskflow agent`, que pediste.

### El flujo completo

```sh
taskflow start                  # FR-52: sin ID, eliges de tu lista priorizada
                                # FR-66: si hay otra sesión, ofrece terminarla o abortarla
                                # FR-57: muestra el reporte completo al terminar el análisis
taskflow report edit            # FR-72: lo abre en $VISUAL / $EDITOR / vi
taskflow approve                # FR-59: el prompt completo al portapapeles
taskflow agent                  # Claude Code en el repo del proyecto, con el prompt
taskflow done
```

### Qué se agregó

| Requisito | Cómo quedó |
| --- | --- |
| FR-56 | `TaskSummary` tiene `acceptance_criteria` y `risks`. El esquema JSON los exige, y `#[serde(default)]` mantiene válidos los análisis viejos. Aparecen en el reporte y en `show --ai`. |
| FR-54 | `Task` tiene `subtasks` y `checklists` (`GET /task/{id}?include_subtasks=true`; las listas de verificación vienen siempre). La IA las recibe como `<subtasks>` y `<checklists>`, con casillas `[x]` / `[ ]`. |
| FR-57 | `start` y `reject` imprimen el reporte completo antes del estado. |
| FR-59 | `approve` copia el prompt completo al portapapeles (`pbcopy`) y dice cómo abrir el agente. Si falla, solo avisa: la aprobación ya ocurrió. |
| FR-52 | `start` sin ID muestra tu lista de `today` para elegir. |
| FR-66 | `start` con una sesión activa ofrece: terminarla (`done`), abortarla o cancelar. "Pausarla y empezar otra" no existe: hay una sola sesión a la vez, como el contador de ClickUp. |
| FR-67 | `[statuses]` en `project.toml` (`ready`, `in_progress`, `done`). Una etapa con lista vacía usa la global. Lo usan la sesión y la sugerencia de `done`, y `project show` los muestra. |
| FR-72 | `report edit`. Corre con `sh -c` para que un `$EDITOR` con argumentos (`code --wait`) funcione. |

```toml
# projects/idk-base/project.toml
[statuses]
ready = ["needs approval"]
in_progress = ["in progress"]
done = ["code review"]
```

### `taskflow agent`

`PrepareAgent` (caso de uso) arma el prompt **completo**, en este orden:

1. `agent-prompt.md`: lo que escribió la IA.
2. `report.md` **tal como lo dejaste**: tus correcciones llegan al agente.
3. Tus secciones `instruction` con uso `agent_prompt` (estilo de código, aprendizaje...).
4. La tarea y su URL, con la regla de FR-61: *no cambies su estado ni la cierres; eso lo hago yo con Taskflow*.

El CLI abre `session.agent_command` (por defecto `claude`) en el `repo_path` del proyecto, con el prompt como primer mensaje. Es interactivo: hereda tu terminal, y al salir la sesión sigue abierta. Solo funciona con la sesión **aprobada** (`in_progress` o `paused`). `taskflow agent --print` solo imprime el prompt.

Como Claude Code se abre **dentro** del repo, también lee el `CLAUDE.md` de IDK.

### Conceptos aplicados

- **`#[serde(default)]` para evolucionar un formato.** Un `TaskSummary` guardado antes de agregar `risks` se sigue leyendo (lista vacía). El esquema que se le pide a la IA sí los exige.
- **Subtareas: un struct que lee solo lo que necesita.** Cada subtarea de ClickUp es una tarea completa, pero `ApiSubtask` declara solo `id`, `name` y `status`, y `serde` ignora el resto.
- **`iter::once(...).chain(...)` en vez de `push_str(&format!(...))`.** Clippy (`format_push_string`) evita crear un `String` temporal por cada línea. Armar la lista y unir con `join` es más claro.
- **La sugerencia más específica gana, con respaldo.** `suggested_statuses` intenta el proyecto y, si no tiene, usa la configuración global. Si el proyecto no se puede leer (`.ok()`), también la global: una sugerencia nunca rompe nada.
- **Un proceso interactivo hijo.** `Command::new("claude").arg(prompt).current_dir(repo).status()` hereda stdin, stdout y stderr, así que el agente usa tu terminal. `status()` espera a que termine; `output()` capturaría la salida y no serviría para algo interactivo.
- **`sh -c '<editor> "$1"' sh <archivo>`.** El archivo va como argumento (`$1`), no pegado en el texto del comando. Una ruta con espacios (`Application Support`) o comillas no rompe nada; es la misma idea que `osascript ... argv`.
- **Una comodidad que nunca falla en voz alta.** `copy_to_clipboard` devuelve `bool`: sin `pbcopy` (Linux), solo no dice "copiado".
- **`if cond && let Ok(x) = ...` (*let chains*, Rust 2024).** Junta una condición y un patrón en un solo `if`. Clippy lo sugiere en vez de dos `if` anidados (`collapsible_if`).

### Probado (2026-10-14)

Con la configuración de prueba (`track_time = false`, `agent_command = echo`), para no tocar tu sesión ni tu contador:

| Prueba | Resultado |
| --- | --- |
| `start 86bcjdn6v --yes` | El reporte completo en pantalla, con criterios de aceptación y riesgos |
| `report edit` (con un editor de prueba que agrega una línea) | La línea quedó en `report.md` |
| `agent` antes de aprobar | `approve the report first with taskflow approve` |
| `approve` sin `pbcopy` | Muestra la ruta y `taskflow agent`, sin "copiado" |
| `agent --print` | El prompt de la IA, `# Reporte aprobado` con la línea editada, `# Reglas para trabajar` (estilo de código y aprendizaje de Rust) y la tarea |
| `agent` (con `echo`) | Abrió en `/Users/dev/swapps/idk` con el prompt |
| `start` sin ID, en una terminal simulada | La lista de tus tareas; luego, con la sesión activa, el menú, y "cancelar" no cambió nada |

### A tener en cuenta

- **Tu sección `90-aprendizaje-rust` llega al agente de IDK**, porque tiene el uso `agent_prompt`. IDK es Python y Django; si no quieres que el agente de IDK la reciba, quítale `agent_prompt` del `usage`.
- **`close` (fase 7) todavía usa solo los estados globales.** La sugerencia por proyecto la usan la sesión y `done`.

---

## Tercera parte: tu prueba real y sus mejoras (2026-10-14)

Hiciste el flujo completo con IDK ("cambiar el fondo del login"). Pediste cuatro mejoras:

### 1. Publicar lo que **se hizo**, no el plan

Antes, `report publish` publicaba `report.md`: el plan, lo que *se iba* a hacer. Ahora publica `work-report.md`, **lo que se hizo**:

```text
taskflow agent      → abre Claude Code con un id de conversación conocido (--session-id <uuid>)
                      y lo guarda en la sesión (agent_session)
taskflow report publish
  → PrepareWorkReport: retoma ESA conversación (claude -p --resume <uuid>) y le pide al mismo
    agente el reporte de lo que hizo, con los cambios de git de la rama como respaldo
  → work-report.md: qué se hizo, cambios, cómo se verificó, pendiente, rama y commits
  → lo ves; publicar, editarlo antes o no publicar
```

- **Si no se puede retomar la conversación** (no abriste el agente, o se abrió en otra carpeta), el reporte sale del plan aprobado y del diff de git, y se avisa. En la prueba, este camino escribió con honestidad "no consta ninguna verificación".
- **Solo se publica con la sesión aprobada** (`in_progress` o `paused`). Antes no hay nada hecho que contar: el dominio lo rechaza.
- `report show --work` y `report edit --work` para verlo o corregirlo; `report publish --regenerate` para volver a armarlo.
- **Nuevo puerto `RepoInspector`** con `GitRepoInspector` (crate `adapter-git`, solo lectura). Detecta la rama, la base (`origin/HEAD`, o `main`/`master`), los commits desde el punto de partida y el diff, incluidos los cambios sin commit. El diff se recorta a 20.000 caracteres.

### 2. Markdown en los comentarios de ClickUp

`comment_text` se publica tal cual: los `##` se veían como texto. Ahora `add_comment` manda `comment`, la lista de tramos con formato. `adapters/clickup/src/markdown.rs` convierte, línea por línea: títulos, viñetas (anidadas), listas numeradas, casillas `[x]`, citas, bloques de código, **negrita**, *cursiva*, `código` y enlaces.

Lo verifiqué contra la API: un comentario de prueba con todos los formatos se guardó con sus atributos, incluidos los títulos (`{"header": 2}`), que la documentación no menciona.

### 3. Claude Code en segundo plano (no prioritario): diseño

No está hecho. Así encajaría:

```text
taskflow agent --background
  → claude -p --session-id <uuid> --permission-mode acceptEdits --allowedTools "<lista>" \
       --output-format stream-json  (en el repo, sin terminal) > sessions/<id>/agent.log
  → al terminar, el agente responde con un JSON: {"status": "done" | "needs_input", "questions": [...]}
  → notificación: "El agente terminó" o "El agente tiene preguntas"
taskflow agent --reply "usa el degradado azul"   → claude -p --resume <uuid> "…" (otra vez en segundo plano)
taskflow agent --log                             → lo que está haciendo
```

- **Lo difícil son los permisos.** Sin terminal nadie puede aprobar un `Edit` o un `Bash`, así que hay que dar una lista cerrada (`Edit`, `Write`, `Bash(git status)`, `Bash(pytest *)`…) y nunca `bypassPermissions`.
- Las piezas ya existen: el id de conversación, `--resume`, `MultiNotifier` y la sesión en disco.

### 4. Un flujo de corrido, sin escribir comandos

En una terminal interactiva, cada paso ofrece el siguiente:

```text
taskflow start
  ¿Qué hacemos con el reporte?    > Aprobar y abrir el agente · Aprobar sin agente · Editar · Rechazar con nota · Decidir después
  … trabajas con el agente; al salir:
  Saliste del agente. ¿Y ahora?   > Publicar lo hecho y terminar · Terminar sin publicar · Volver al agente · Dejar abierta
  ¿Publicar este reporte?         > Publicar · Editarlo antes · No publicar
  Estado final / comentario / ¿Terminar la sesión?
```

- Cada decisión sigue siendo tuya (FR-59, FR-61). Con `--yes`, o sin terminal (un script), no se pregunta nada.
- `taskflow approve` también ofrece abrir el agente.
- `taskflow agent` retoma la misma conversación si vuelves a abrirlo (`--resume`). Si falla al instante, empieza una nueva.

### Conceptos aplicados

- **Retomar la conversación en vez de adivinar.** Quien mejor sabe qué se hizo es el agente que lo hizo. Con un id que elegimos nosotros (`--session-id`), Taskflow puede volver a esa conversación (`--resume`) sin capturar la salida de un programa interactivo. `--no-session-persistence` hace que esa consulta no se agregue a la conversación.
- **Degradar con respaldo y avisar.** Si retomar falla, el resumen sale de los datos objetivos (plan y git), y el aviso dice por qué. Es el mismo patrón que `PlanMyDay` cuando falla la IA.
- **Convertir formatos en el adaptador.** El dominio y la aplicación hablan Markdown; solo el adaptador de ClickUp sabe que ClickUp quiere tramos. Otro proveedor (GitHub, que entiende Markdown) lo mandaría tal cual.
- **Mover en vez de copiar (`needless_pass_by_value`).** `json!` serializa por referencia y no aprovecha que el `Map` ya es nuestro. Armar el objeto con `Map::insert` mueve los valores sin copiarlos.
- **Romper ciclos entre funciones `async`.** `approve` → `agent` → `done` → `handle` no puede volver a `approve`: una función `async` recursiva necesita `Box::pin`. Por eso hay un núcleo (`apply`) que solo ejecuta y muestra, y las funciones que ofrecen el paso siguiente están por encima de él, nunca por debajo.
- **`IsTerminal`.** `std::io::stdin().is_terminal()` distingue una persona de un script: las preguntas solo aparecen cuando alguien puede responderlas.
- **`git -C <repo>`**, y que un comando que falla sea un dato (`Ok(None)`), no un error: "no es un repositorio" o "no hay `origin/HEAD`" son respuestas válidas.

### Probado (2026-10-14)

Con un repo de git falso y la tarea de prueba:

| Prueba | Resultado |
| --- | --- |
| Comentario con todos los formatos (API directa) | Guardado con sus atributos; revisa que se vea bien en la tarea |
| `report publish` sin conversación del agente | Reporte desde el plan y el diff (rama, commit, archivo), publicado con formato; el aviso dice por qué |
| `agent` → conversación nueva con id; `report publish --regenerate` | "lo escribió el mismo agente": incluye lo que solo él sabía (revisado en el navegador, contraste pendiente) más lo que confirma git |
| `agent` con un id que no existe | Aviso y conversación nueva |
| `start` en una terminal → "Decidir después" | La sesión queda esperando tu aprobación |

Tests: de 427 a 444.

---

## Cuarta parte: el commit lo hace Taskflow, y el selector ya no se desordena

### El selector de tareas se desordenaba

Con títulos más largos que la terminal, cada opción ocupaba dos líneas. `dialoguer` borra **una línea por opción** al redibujar, así que al moverte quedaban copias en pantalla. Ahora cada opción se recorta al ancho de la terminal (`dialoguer::console::Term::size()`, menos el margen del cursor). Probado en 80 columnas: todas las opciones miden 78.

### `taskflow commit`

Al salir del agente, si hay cambios, la primera opción del menú es **"Hacer commit de los cambios"**. También existe como comando suelto:

```text
taskflow commit
  Repositorio: ~/swapps/idk · rama `CU-86bcjdn6v`
  ? Archivos del commit (espacio para marcar, Enter para seguir)
    ✔  M idk/templates/account/login.html
    ✔  M idk/assets/sass/new_theme/_sigin.scss
       ?? artifacts/CU-86bc1994z-propuestas-mejora.md (nuevo)   ← sin marcar: no es de la tarea
  Mensaje del commit:
  CU-86bcjdn6v - feat: fondo degradado en el login
  ? ¿Hacer el commit de 2 archivos?  > Hacer el commit · Editar el título · Editar todo en tu editor · Cancelar
  ✔ Commit 2aef913: CU-86bcjdn6v - feat: fondo degradado en el login
```

- **El mensaje lo propone la IA** a partir del diff y de los **últimos 10 commits del repo**, para que siga la convención (`CU-<id> - feat: …`). Le llegan también tus reglas (secciones `instruction`).
- **El commit lo hace Taskflow** con `git`: `git add -A -- <archivos>` y `git commit -F - -- <archivos>`. Solo entran los archivos que elegiste, aunque hubiera otros preparados.
- **Los archivos nuevos vienen sin marcar.** Pueden ser tuyos y no de la tarea, como tu `artifacts/…md` de IDK. Con `--yes` o sin terminal, entran solo los que git ya conoce.
- **Los hooks del repo se ejecutan** (los de pre-commit de IDK). Si uno falla, ves su salida y no se hace el commit.
- **Si estás en la rama base** (`master`/`main`), avisa: conviene crear la rama de la tarea primero.

Por capa:

| Capa | Qué |
| --- | --- |
| Puerto | `RepoCommitter`: `changed_files`, `pending_diff`, `recent_commits`, `commit`. Separado de `RepoInspector` (que solo lee) |
| IA | `Summarizer::commit_message(&CommitInput) -> CommitMessage { subject, body }`. Claude Code lo resuelve sin herramientas: el diff ya va en el mensaje |
| Casos de uso | `PrepareCommit` (archivos + mensaje propuesto + avisos; `pending_files` sin IA, para el menú) y `CommitChanges` |
| Adaptador | `GitRepoInspector` implementa los dos puertos. `git status --porcelain=v1 -z -uall` |
| CLI | `taskflow commit [--yes]` y la opción en el menú al salir del agente |

### Conceptos aplicados

- **`-z` en la salida de git.** Sin él, git pone comillas en rutas con espacios o tildes, y un renombre se escribe `viejo -> nuevo`. Con `-z` cada campo termina en NUL, y un renombre trae la ruta original en el campo siguiente: se parsea sin ambigüedad.
- **`git commit -- <rutas>`** confirma solo esas rutas, aunque haya otras en el índice. Así nunca entra algo que no elegiste.
- **El mensaje por la entrada estándar (`-F -`)**, con `tokio::process` y `AsyncWriteExt::write_all`. No hay que escapar comillas ni saltos de línea en un argumento.
- **Un menú como datos: `Vec<(String, AfterAgent)>`.** Las opciones cambian (el commit solo aparece si hay cambios). Cada etiqueta va con su acción en un `enum`, y `swap_remove(index).1` devuelve la elegida sin comparar textos.
- **`let ... else`** en vez de un `match` con dos brazos donde uno sale de la función (`clippy::single_match_else`).
- **`clone_into`** reutiliza la memoria del `String` existente en vez de crear otro y soltar el viejo (`clippy::assigning_clones`).

### Probado (2026-10-14)

Con un repo de git falso:

| Prueba | Resultado |
| --- | --- |
| `commit --yes` con un archivo modificado y uno nuevo tuyo | Commit `CU-86bcjdn6v - feat: aplicar clase login--gradient al body del login`; tu archivo nuevo quedó fuera |
| Salir del agente (terminal simulada) → "Hacer commit" → Enter → "Hacer el commit" | Mensaje propuesto, commit hecho, y de vuelta al menú |
| Selector de tareas en 80 columnas | Todas las opciones miden 78 |

Tests: de 444 a 452.

---

## Quinta parte: idiomas, tareas devueltas y la salida en la terminal

### Idiomas

| Qué | Idioma | Clave |
| --- | --- | --- |
| Análisis, plan del día, reporte de trabajo (lo que se publica), tu conversación con Taskflow y con el agente | español | `ai.report_language` |
| El prompt del agente, el código, los comentarios del código, los mensajes de commit y todo lo que queda en el repo | English | `ai.agent_language` |

- Cada llamada a la IA termina con su regla de idioma (`prompts::with_rule`). El análisis escribe todo en español, salvo `agent_prompt`, que va en inglés y pide código, comentarios y commits en inglés.
- Lo que Taskflow le agrega al prompt del agente también va en inglés: `# Approved plan`, `# Rules for this work`, `Language: …` y `Task: …`.
- El mensaje de commit va en inglés aunque los commits anteriores del repo estén en otro idioma.

### Retomar una tarea que te devolvieron

```text
taskflow start 86bcjdn6v
Esta tarea ya se trabajó: sesión cerrada el 2026-10-14 19:50 · reporte publicado · con conversación del agente.
? ¿Cómo sigues?
> Continuar donde quedó (la IA ve lo hecho y lo nuevo; el agente retoma su conversación)
  Empezar de cero
```

Al continuar:

1. **El análisis sabe qué se hizo antes.** Recibe `<previous_session>` con la fecha de cierre y el reporte de trabajo, y se le pide analizar solo lo que piden los comentarios posteriores. En la prueba, el reporte nuevo empezó con "Cerrar lo que falta del cambio…" y listó los pendientes, sin repetir lo hecho.
2. **El agente retoma su conversación**, pero con el análisis nuevo: `claude --resume <id> "<prompt nuevo>"`. La primera vez se le da el prompt (`agent_briefed` pasa a `true`); las siguientes, solo `--resume`.
3. **El reporte de trabajo cuenta solo lo nuevo** (`<since>`): lo de antes ya se publicó.
4. **El `work-report.md` anterior se quita** al empezar cualquier sesión (`take_work_report`), para que nunca se vuelva a publicar por error.

`start --fresh` empieza de cero; con `--yes`, continúa. Por capa:

- **Dominio:** `WorkSession::continue_from`, `ended_at`, y los campos `agent_briefed` y `continues`.
- **Puerto:** `SessionStore::last_archived` y `take_work_report`.
- **Aplicación:** `StartSession::fresh`, `PrepareAgent::mark_briefed`, `SummaryInput::previous` y `WorkInput::since`.

### Comentarios con menos Markdown

El reporte que se publica ya no tiene títulos:

```text
Trabajo realizado: se cambió el fondo del login a un degradado azul…

Cambios:
- `idk/assets/sass/new_theme/_sigin.scss`: …

Cómo se verificó: sin registrar.

Pendiente: nada.

Rama: `CU-86bcjdn6v` · 1 commit
```

A la IA también se le pide no usar títulos ni negritas en ese reporte.

### Markdown en la terminal

`apps/cli/src/markdown.rs` da formato a los reportes y los planes al mostrarlos: títulos en negrita y color, viñetas `•`, `código` en amarillo, negrita, cursiva y enlaces. Si la salida no es una terminal (un archivo, una tubería), queda texto limpio, sin `#` ni `**`.

### Rutas entre comillas

Todas las rutas que imprime Taskflow van entre comillas (`presenter::path`): `"/Users/dev/Library/Application Support/taskflow/…"`. Así el espacio de `Application Support` no corta la ruta al copiarla o al hacer clic. La excepción es `taskflow config path`, que sigue sin comillas para poder usarla en scripts (`cat "$(taskflow config path)"`).

### Conceptos aplicados

- **Un constructor con opciones (`StartSession::new(ctx).fresh(true)`).** Agrega una opción sin cambiar la firma de `execute`, que usan muchos tests y la interfaz.
- **`Option::clone_from`** reutiliza la memoria del valor que ya había.
- **`if !fresh && let Some(previous) = …`** (*let chains*): una condición y un patrón en el mismo `if`.
- **`Style::force_styling(true)`.** `console` desactiva los colores cuando cree que no hay terminal. En los tests se fuerza para comprobar que hay códigos ANSI y que, sin ellos, el texto es el mismo.
- **Separar idioma y rol.** Los roles de los prompts ya no dicen "en español": el idioma es configuración y se agrega al final. El mismo prompt sirve para otro idioma.

### Probado (2026-10-14)

| Prueba | Resultado |
| --- | --- |
| `agent-prompt.md` y `report.md` | El prompt, en inglés ("You are working on the IDK Django dashboard…"); el reporte, en español |
| Cerrar la sesión y `start` de nuevo | "Esta tarea ya se trabajó…"; la nueva continúa (`continues`), con la conversación anterior y `agent_briefed: false` |
| `agent` en la sesión continuada | `--resume <id>` con el prompt nuevo; después `agent_briefed: true` |
| El análisis continuado | Empezó con "Cerrar lo que falta…" y listó solo los pendientes |
| Un `start` que se quedó esperando a ClickUp (una vez) | `session retry` lo retomó y el análisis terminó |

Tests: de 452 a 462.
