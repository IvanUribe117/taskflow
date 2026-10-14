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
