# Fase 6B: IA con Claude Code

Objetivo: que Taskflow use IA con el contexto de la fase 6A. El primer proveedor es **Claude Code** (`claude -p`), que usa tu sesión de Claude: no hace falta API key ni pago aparte (ADR-027).

```sh
taskflow show 86bcfn2dc --ai                    # detalle + resumen, pasos, preguntas y prompt para el agente
taskflow show 86bcfn2dc --ai --out prompt.md    # además guarda el prompt en un archivo
taskflow plan                                   # plan del día con tus tareas priorizadas
taskflow plan --project idk-base --max 5
```

Cuando la tarea es de un proyecto con `repo_path`, **Claude Code lee el repositorio** (solo lectura) y su análisis cita archivos y líneas reales. Probado con IDK: tardó 17 segundos con el modelo pequeño, encontró `config/urls.py`, las vistas de salud que ya existían y los middlewares que había que tocar, y no modificó nada.

Esta fase tiene **6 pasos**, todos con conceptos nuevos. Fallan **30 tests** que dependen de ellos; los comprobé con una implementación de referencia que después borré.

---

## Cómo funciona

```text
show --ai 86bcfn2dc
  │
  ▼
SummarizeTask (application)                                      paso 5
  ├─ tarea + comentarios + proyecto (ClickUp, archivos)
  ├─ BuildContext: secciones de "summary" Y "agent_prompt" (6A)
  ├─ SummaryInput: solo lo necesario, recortado (NFR-05)
  └─ ctx.ai.summarize_task(&input) ─────────────────┐
                                                    ▼
ClaudeCodeSummarizer (adapter-claude-code)
  ├─ prompts: system prompt + mensaje + esquema JSON              paso 1
  ├─ command: argumentos de `claude` y carpeta de trabajo         paso 2
  ├─ process: ejecutar `claude`, stdin → stdout, tiempo límite    paso 3
  └─ output: leer el JSON y convertirlo en TaskSummary            paso 4
```

`plan` sigue el mismo camino con `PlanMyDay` (paso 6), que además **no falla si falla la IA**: muestra tus tareas y el motivo.

### Lo que se le permite a Claude Code

| Flag | Para qué |
| --- | --- |
| `-p` | Responder y terminar, sin sesión interactiva |
| `--output-format json` + `--json-schema` | La respuesta llega como JSON validado contra el esquema, en `structured_output` |
| `--system-prompt` | El rol y las secciones de *instrucción* de la 6A |
| `--tools "Read,Grep,Glob"` + `--allowedTools` (igual) | **Solo lectura**: puede leer y buscar en el repo; no puede ejecutar comandos ni editar |
| `--tools ""` | Sin repositorio: ninguna herramienta |
| `--restricted` | Confina la lectura a la carpeta de trabajo e ignora la configuración de la carpeta (sus hooks) |
| `--strict-mcp-config` | No conecta servidores MCP del repositorio |
| `--permission-mode dontAsk` | Todo lo que necesitaría permiso se niega, sin preguntar (nadie está mirando) |
| `--no-session-persistence` | No guarda la sesión en tu historial de Claude Code |

`--restricted` y `--strict-mcp-config` importan porque `claude -p` dentro de un repositorio **ejecuta sus hooks y servidores MCP** aunque nunca hayas confiado en esa carpeta. Aquí se ejecuta dentro de repos de clientes.

---

## Lo que ya hice yo

| Qué | Archivo |
| --- | --- |
| Puerto `Summarizer`, `AiError`, `SummaryInput`, `PlanInput`, `TaskSummary`, `Complexity` | `application/src/ports/summarizer.rs` |
| `ctx.ai` en `AppContext`, `AppError::Ai`, `FakeSummarizer` | `context.rs`, `error.rs`, `testing.rs` |
| Sección `[ai]` (`kind`, `model`, `effort`, `timeout_secs`, `max_tasks`, `max_description_chars`, `max_comments`, `read_repo`) y su validación | `settings/mod.rs`, `settings/key.rs` |
| `select` y `ContextRequest` con **varios usos** a la vez | `domain/src/context/mod.rs`, `sections/` |
| Conversiones a lo que recibe la IA: `task_brief`, `recent_comments`, `plan_task`, `truncate_chars` | `application/src/ai/brief.rs` |
| `TaskInsight`, `DailyPlan` | `dto.rs` |
| `ClaudeCodeConfig`, el *system prompt*, `plan_message`, los esquemas JSON y `ClaudeCodeSummarizer` (une los 4 pasos del adaptador) | `adapters/claude-code/src/` |
| Registro de proveedores de IA (`AiProviderDescriptor`, `ai_providers`, `find_ai`) y su construcción | `bootstrap/src/registry.rs`, `claude_code.rs`, `build.rs` |
| `show --ai --out`, `plan`, el presentador y la pregunta "Proveedor de IA" en `config init` | `apps/cli/src/` |

Tu `config.toml` no necesita cambios: si no tiene `[ai]`, se usan los valores por defecto (Claude Code, 300 s de tiempo límite, leer el repositorio).

---

## Pasos

| # | Archivo | Qué implementar | Concepto nuevo | Verificar con |
| - | ------- | --------------- | -------------- | ------------- |
| 1 | `adapters/claude-code/src/prompts.rs` | `summary_message` | Diseñar un prompt: datos delimitados con etiquetas | `cargo test -p adapter-claude-code --lib prompts` |
| 2 | `adapters/claude-code/src/command.rs` | `invocation` | Argumentos de un programa; mínimo privilegio | `cargo test -p adapter-claude-code --lib command` |
| 3 | `adapters/claude-code/src/process.rs` | `run` | `tokio::process`: pipes, stdin, tiempo límite, código de salida | `cargo test -p adapter-claude-code --test fake_claude` |
| 4 | `adapters/claude-code/src/output.rs` | `parse` | Un "sobre" JSON genérico: `Value` → `T` | `cargo test -p adapter-claude-code --lib output` |
| 5 | `application/src/ai/summarize_task.rs` | `SummarizeTask::execute` | Minimizar lo que se envía; usos combinados con `\|` | `cargo test -p application --lib ai::summarize` |
| 6 | `application/src/ai/plan_my_day.rs` | `PlanMyDay::execute` | Degradación elegante: `Result` → dato | `cargo test -p application --lib ai::plan` |
| 7 | — | Probarlo con tus tareas | — | Ver "Paso 7" |

Los tests de `fake_claude` (paso 3) necesitan también los pasos 1, 2 y 4, porque prueban el adaptador entero.

Al terminar, **borra los bloques `#![allow(...)]` con `TODO(fase 6B)`** en `crates/application/src/lib.rs` y `crates/adapters/claude-code/src/lib.rs`.

---

## Conceptos nuevos

### Un proceso hijo con `tokio::process`

```rust
let mut child = Command::new("claude")
    .args(&args)
    .current_dir(repo)              // la carpeta donde se ejecuta
    .stdin(Stdio::piped())          // vamos a escribirle
    .stdout(Stdio::piped())         // vamos a leer lo que imprime
    .stderr(Stdio::piped())         // y sus errores
    .kill_on_drop(true)             // si soltamos `child`, se mata el proceso
    .spawn()?;                      // arranca y NO espera

let mut stdin = child.stdin.take().expect("stdin is piped");
stdin.write_all(prompt.as_bytes()).await?;
drop(stdin);                        // cerrar: `claude` recibe "fin de la entrada" y empieza

let output = child.wait_with_output().await?;   // espera a que termine; junta stdout y stderr
```

| Pieza | Por qué |
| --- | --- |
| `Stdio::piped()` | Conecta un "tubo" entre los dos procesos. Sin esto, el hijo heredaría tu terminal |
| `spawn` contra `output` | `spawn` arranca y devuelve el control; así se puede escribir en su entrada antes de esperarlo |
| `child.stdin.take()` | `take()` saca el `ChildStdin` del `Option` y te lo da: a partir de ahí es tuyo |
| `drop(stdin)` | Hasta que se cierra la entrada, `claude` sigue esperando más texto. Sin el `drop`, los dos procesos se esperarían para siempre |
| El prompt por stdin, no como argumento | Un argumento tiene un límite de tamaño del sistema operativo y aparece en la lista de procesos (`ps`). La entrada estándar no |
| `kill_on_drop(true)` | Si se agota el tiempo y se suelta el `child`, el proceso muere. Sin esto quedaría corriendo en segundo plano |

### Tiempo límite: `tokio::time::timeout`

```rust
let output = tokio::time::timeout(Duration::from_secs(300), child.wait_with_output())
    .await                                   // Result<Result<Output, io::Error>, Elapsed>
    .map_err(|_| AiError::Timeout(...))?     // el de afuera: ¿llegó a tiempo?
    .map_err(|e| AiError::Failed(...))?;     // el de adentro: ¿funcionó la espera?
```

`timeout` envuelve cualquier future: si termina a tiempo, da su resultado; si no, da `Err(Elapsed)` y **suelta** el future. Al soltarse, se suelta también el `child` que contenía, y `kill_on_drop` mata el proceso. Dos `Result` anidados, dos `?`.

### Un programa falso para probar procesos

Los tests de `tests/fake_claude.rs` no usan Claude: escriben un script de shell llamado `claude` en una carpeta temporal, que guarda sus argumentos, su entrada y su carpeta de trabajo en archivos, y responde un JSON fijo. Así se prueba el proceso **real** (argumentos, stdin, `current_dir`, tiempo límite, código de salida) sin red y sin gastar llamadas.

Es la misma idea que wiremock en la fase 3, pero para programas en lugar de HTTP. `#![cfg(unix)]` hace que esos tests solo se compilen en sistemas con `sh` (macOS y Linux).

### Mínimo privilegio

Claude Code puede hacer mucho: ejecutar comandos, editar archivos, conectarse a servidores MCP. Para resumir una tarea solo necesita **leer**. La regla es darle solo eso: `--tools "Read,Grep,Glob"` y nada más, y ninguna herramienta si no hay repositorio. Si algún día un prompt de un comentario de ClickUp intentara convencer a la IA de "borrar archivos", no tendría con qué hacerlo.

### Datos delimitados en el prompt

```text
<task>
id: 86bcfn2dc
título: Record and show the client a dev/preview environment
...
</task>

<comments>
- Client PM (2026-10-09): ¿se puede ver antes de producción?
</comments>
```

Las etiquetas tipo XML le dicen a la IA dónde empieza y termina cada dato, y que lo de adentro es **material de trabajo, no instrucciones**. Un comentario que diga "ignora todo lo anterior" queda dentro de `<comments>`, como dato.

### Minimizar lo que sale (NFR-05)

`SummaryInput` no es la tarea entera: es un `TaskBrief` con la descripción recortada (`ai.max_description_chars`), los últimos N comentarios (`ai.max_comments`) y las secciones de contexto que entran en el presupuesto. Menos datos significa menos costo, respuestas más rápidas y menos información de clientes fuera de tu máquina.

### Varios usos a la vez: `Usage::Summary | Usage::AgentPrompt`

El resumen y el prompt para el agente salen de la **misma** llamada, así que necesita las secciones de los dos usos. `|` entre dos `Usage` da un `EnumSet<Usage>` (lo genera `#[derive(EnumSetType)]`), y `select` ahora acepta `impl Into<EnumSet<Usage>>`: un solo uso (`Usage::Plan`) o varios. `!section.usage.is_disjoint(usages)` responde "¿comparten algún uso?" con una operación de bits.

### Degradación elegante

```rust
let (plan, ai_error) = match self.ctx.ai.plan_day(&input).await {
    Ok(plan) => (Some(plan), None),
    Err(error) => (None, Some(error.to_string())),
};
```

En `plan`, un fallo de la IA **no es un error del comando**: es un dato del resultado. La lista priorizada sigue siendo útil sin el plan, así que `PlanMyDay` devuelve las tareas y el motivo del fallo (UC-02). En `show --ai`, en cambio, el análisis es lo que pediste, y un fallo sí es un error.

---

## Especificación por paso

### Paso 1: `summary_message` (`adapters/claude-code/src/prompts.rs`)

Arma el mensaje con estas partes, **en este orden**, separadas por una línea en blanco (`"\n\n"`). El test `summary_message_has_every_part` compara el texto completo.

| Parte | Cuándo | Texto |
| --- | --- | --- |
| `<task>` | Siempre | Ver abajo |
| `<description>` | Siempre | `<description>\n{descripción}\n</description>`; sin descripción: `(sin descripción)` |
| `<comments>` | Solo si hay comentarios | `<comments>\n` + una línea `- {comentario}\n` por cada uno + `</comments>` |
| `<project>` | Solo si hay proyecto | `<project>\nnombre: {nombre}\n` + `repositorio: {ruta}\n` (solo si tiene `repo_path`) + `</project>` |
| `<project_context>` | Solo si hay secciones de conocimiento | `<project_context>\n` + `push_blocks(&mut message, &input.context.knowledge)` + `</project_context>` |

`<task>`, una línea por campo:

```text
<task>
id: {id}
título: {title}
estado: {status}
prioridad: {priority}
vence: {due, o "sin fecha"}
etiquetas: {tags unidas con ", ", o "(ninguna)"}
url: {url}
</task>
```

Herramientas:

- `format!` para el bloque `<task>` completo; `push_str` para agregar partes.
- `writeln!(message, "- {comment}")` escribe en un `String` con salto de línea. Necesita `use std::fmt::Write` (ya está) y devuelve un `Result` que se puede ignorar con `let _ =` (escribir en un `String` nunca falla).
- `task.due.as_deref().unwrap_or("sin fecha")`: de `Option<String>` a `&str`.
- `repo.display()` muestra un `PathBuf`.
- `push_blocks` (ya está) agrega los bloques de conocimiento con el mismo formato que las instrucciones.

### Paso 2: `invocation` (`adapters/claude-code/src/command.rs`)

| # | Argumentos | Cuándo |
| --- | --- | --- |
| 1 | `-p`, `--output-format`, `json`, `--no-session-persistence`, `--restricted`, `--strict-mcp-config`, `--permission-mode`, `dontAsk` | Siempre, en ese orden (`-p` primero: lo comprueba un test) |
| 2 | `--json-schema`, `schema.to_string()` | Siempre. `Value::to_string()` da el JSON como texto |
| 3 | `--system-prompt`, `system` | Siempre |
| 4 | `--tools`, `READ_ONLY_TOOLS`, `--allowedTools`, `READ_ONLY_TOOLS` | Si hay `repo` **y** `config.read_repo` |
| 4' | `--tools`, `""` | Si no (texto vacío = ninguna herramienta) |
| 5 | `--model`, `{model}` | Solo si `config.model` es `Some` |
| 6 | `--effort`, `{effort}` | Solo si `config.effort` es `Some` |

Y el resto de `Invocation`:

| Campo | Valor |
| --- | --- |
| `cwd` | El `repo` como `PathBuf`, **solo** en el caso 4; si no, `None` |
| `stdin` | `message` (se mueve) |

Idea práctica: calcula primero `let repo = repo.filter(|_| config.read_repo);` y usa ese `Option` para los puntos 4/4' y para `cwd`. Para agregar pares, `args.extend(["--model".to_owned(), model.clone()])`.

### Paso 3: `run` (`adapters/claude-code/src/process.rs`)

| # | Qué | Si falla |
| --- | --- | --- |
| 1 | `Command::new(program)` con `.args(&invocation.args)`, las tres salidas en `Stdio::piped()` y `.kill_on_drop(true)` | — |
| 2 | Si `invocation.cwd` es `Some(cwd)` → `command.current_dir(cwd)` | — |
| 3 | `command.spawn()` | `io::ErrorKind::NotFound` → `AiError::Unavailable("`{program}` not found: install Claude Code and sign in")`; otro → `AiError::Unavailable(e.to_string())` |
| 4 | `let mut stdin = child.stdin.take().expect("stdin is piped");` → `stdin.write_all(invocation.stdin.as_bytes()).await` → `drop(stdin)` | `AiError::Failed("cannot write the prompt: …")` |
| 5 | `tokio::time::timeout(timeout, child.wait_with_output()).await` | `Elapsed` → `AiError::Timeout(timeout)`; el `io::Error` de adentro → `AiError::Failed(e.to_string())` |
| 6 | Decidir con la salida (tabla de abajo) | — |

| `stdout` | Código de salida | Resultado |
| --- | --- | --- |
| Con texto | cualquiera | `Ok(stdout)`: lo interpreta `parse` (aunque falle, `claude` imprime el error como JSON) |
| Vacío | éxito | `Err(AiError::InvalidResponse("empty output"))` |
| Vacío | error | `Err(AiError::Failed(stderr))`; si `stderr` también está vacío, `"exited with {status}"` |

- `String::from_utf8_lossy(&output.stdout)` convierte los bytes a texto (reemplaza lo que no sea UTF-8 válido en lugar de fallar). `.into_owned()` para tener un `String`.
- `output.status.success()` dice si terminó con código 0.

### Paso 4: `parse` (`adapters/claude-code/src/output.rs`)

| # | Qué | Si falla |
| --- | --- | --- |
| 1 | `serde_json::from_str::<Envelope>(stdout.trim())` | `AiError::InvalidResponse("not the JSON of `claude -p`: {e}")` |
| 2 | Si `envelope.is_error` | `AiError::Failed(...)` con `result`, o si no hay, `subtype`, o si tampoco, `"unknown error"` |
| 3 | `envelope.structured_output` | `None` → `AiError::InvalidResponse("the response has no structured_output")` |
| 4 | `serde_json::from_value::<T>(structured)` | `AiError::InvalidResponse(e.to_string())` |

Para el punto 2: `envelope.result.or(envelope.subtype).unwrap_or_else(|| "unknown error".to_owned())`. `or` se queda con el primer `Some`.

`parse` es **genérica**: el mismo código lee un `TaskSummary` o el `{ "plan": ... }` del plan. `T` lo decide quien llama, como `get<T>` en la fase 3. `structured_output` llega como `serde_json::Value` (JSON sin tipo), y `from_value` lo convierte al tipo que se pidió.

### Paso 5: `SummarizeTask::execute(id, limits)` (`application/src/ai/summarize_task.rs`)

| # | Qué | Código clave |
| --- | --- | --- |
| 1 | Usuario, tarea y comentarios **a la vez** | `futures::try_join!(self.ctx.tasks.current_user(), self.ctx.tasks.task(id), self.ctx.tasks.comments(id))?`, como `InspectTask` |
| 2 | Poner los comentarios en la tarea | `task.comments = comments;` |
| 3 | El proyecto | `ProjectResolver::new(self.ctx.projects.list().await?)` y `resolver.resolve(&task)` |
| 4 | El contexto, con **los dos usos** | `BuildContext` con `usage: Usage::Summary \| Usage::AgentPrompt` y `limits.budget_chars` |
| 5 | Lo que se envía | `SummaryInput` (tabla de abajo), con `let offset = self.ctx.clock.offset();` |
| 6 | La IA | `let summary = self.ctx.ai.summarize_task(&input).await?;` |
| 7 | El detalle | `TaskDetail::new(&task, project, &me, self.ctx.clock.now())` |
| 8 | Devolver | `Ok(TaskInsight { detail, summary })` |

| Campo de `SummaryInput` | Valor |
| --- | --- |
| `task` | `task_brief(&task, limits.max_description_chars, offset)` (ya está) |
| `comments` | `recent_comments(&task.comments, limits.max_comments, offset)` (ya está) |
| `project` | `project.map(\|p\| ProjectBrief { name: p.name.clone(), repo_path: p.repo_path.clone() })` |
| `context` | El del punto 4 |

`resolver` debe vivir mientras uses `project`, porque `project` es una referencia **dentro** del resolver (la lección del paso 4 de la 6A).

### Paso 6: `PlanMyDay::execute(query)` (`application/src/ai/plan_my_day.rs`)

| # | Qué | Código clave |
| --- | --- | --- |
| 1 | Las tareas, agrupadas por proyecto | `GetTodayTasks` con `TodayQuery { limit: Some(query.max_tasks), project: query.project.clone(), group_by: GroupBy::Project }` |
| 2 | Aplanarlas con el nombre de su proyecto | Ver abajo |
| 3 | El contexto | `BuildContext` con `project: query.project`, `usage: Usage::Plan.into()`, `query.budget_chars`; guarda `context.omitted.clone()` |
| 4 | Sin tareas → devolver **sin llamar a la IA** | `DailyPlan { plan: None, ai_error: None, … }` |
| 5 | La IA, con degradación | `match self.ctx.ai.plan_day(&input).await { Ok(p) => (Some(p), None), Err(e) => (None, Some(e.to_string())) }` |
| 6 | Devolver | `DailyPlan` con `generated_at: report.generated_at`, las tareas, `plan`, `ai_error` y las omitidas |

`PlanInput`: `date` es el día local (`self.ctx.clock.now().with_timezone(&offset).date_naive()`), `tasks` y `context`.

El punto 2, con `flat_map`: cada grupo se convierte en sus tareas, cada una con el nombre del proyecto del grupo.

```rust
let tasks: Vec<PlanTask> = report
    .groups
    .iter()
    .flat_map(|group| {
        let project = group.project.as_ref().map(|p| p.name.as_str());
        group.tasks.iter().map(move |task| plan_task(task, project, offset))
    })
    .collect();
```

`move` hace que la closure interna se lleve **una copia** de `project` (un `Option<&str>`, que es `Copy`) y de `offset`. Sin `move`, intentaría tomarlos prestados de la closure externa, que ya terminó cuando `collect` la ejecuta.

`tasks` va dentro de `PlanInput` y también en `DailyPlan`. Muévelo a `input` y después recupéralo con `input.tasks`, en lugar de clonarlo.

---

## Paso 7: probarlo con tus tareas

```sh
cargo run -q -p taskflow -- show <id de una tarea de IDK> --ai
cargo run -q -p taskflow -- show <id> --ai --out /tmp/prompt.md && cat /tmp/prompt.md
cargo run -q -p taskflow -- plan
cargo run -q -p taskflow -- plan --project idk-base --max 5
```

Mientras la IA trabaja, verás `Analizando la tarea con claude_code…` en la salida de errores. Con el modelo por defecto de tu Claude Code y leyendo el repo, puede tardar de 20 segundos a un par de minutos.

**Ajustes** (`taskflow config set ...`):

| Clave | Para qué | Ejemplo |
| --- | --- | --- |
| `ai.model` | Otro modelo: más rápido o más capaz | `sonnet`, `opus`, `haiku` |
| `ai.effort` | Cuánto razona: menos es más rápido | `low`, `medium`, `high` |
| `ai.timeout_secs` | Tiempo máximo | `600` |
| `ai.read_repo` | `false` para que no lea el repositorio | `false` |
| `ai.max_tasks` | Tareas que entran en `plan` | `15` |

**Si algo falla:**

| Mensaje | Qué hacer |
| --- | --- |
| `AI provider unavailable: `claude` not found` | Instala Claude Code o revisa que `claude` esté en tu `PATH` |
| `AI provider failed: Not logged in…` | Abre `claude` una vez e inicia sesión |
| `AI provider timed out after 300 s` | Sube `ai.timeout_secs` o baja `ai.effort` |

---

## Limitaciones conocidas

- **El plan usa el contexto global y, con `--project`, el de ese proyecto.** Sin `--project`, el conocimiento de cada proyecto no entra (repartir el presupuesto entre varios proyectos queda para después).
- **`show --ai` y `plan` no guardan caché:** cada ejecución es una llamada nueva (fase 12).
- **Un solo proveedor de IA.** El registro ya admite más: la API de Anthropic, OpenAI y Gemini son un descriptor y un adaptador cada uno (ADR-027).
- **Sin portapapeles:** el prompt se guarda con `--out` (FR-09, decisión abierta en §26).

---

## Qué observar mientras implementas

- **Los casos de uso no saben que existe Claude Code.** `SummarizeTask` llama a `ctx.ai.summarize_task`. Mañana, con Gemini, no cambia ni una línea.
- **El mismo puerto, implementaciones muy distintas.** ClickUp hace HTTP; Claude Code ejecuta un programa. Los dos se prueban sin red: uno con wiremock, el otro con un `claude` falso.
- **El contexto de la 6A llega solo.** Las secciones de IDK que generó el agente son las que verás reflejadas en el análisis.

Cuando termines, avísame y reviso. Las dudas las sumamos al final de este documento.

---

# Conceptos aplicados

La fase quedó completa. El paso 1 lo empezaste tú; lo terminé junto con los pasos 2 a 6.

## Paso 1: el prompt, línea por línea

Tu inicio usaba un solo `format!` para el bloque `<task>`, que es la forma correcta. Los ajustes:

| Tenías | Quedó | Por qué |
| --- | --- | --- |
| `\n\n` entre campos | `\n` | Los `\n\n` separan **partes** (`<task>`, `<description>`...); los campos de una parte van en líneas seguidas |
| `id:{}` | `id: {}` | Un espacio después de los dos puntos |
| `title`, `status`, `priotity`, `due`, `tags` | `título`, `estado`, `prioridad`, `vence`, `etiquetas` | El prompt está en español; `priotity` además tenía un error de tipeo |
| `tags.join(",")` | `", "`, o `(ninguna)` | Legible, y explícito cuando no hay |
| Sin `</task>` | `\n</task>` | Toda etiqueta que se abre se cierra: la IA sabe dónde termina cada dato |

- **`let mut message`:** el mensaje empieza con el `format!` y después crece con `push_str` y `writeln!`.
- **`writeln!(message, "- {comment}")`** escribe en un `String` igual que `println!` en la terminal, porque `String` implementa `fmt::Write`. El `Result` que devuelve nunca es error en un `String`: `let _ =`.
- **Qué se omite y qué no.** La descripción aparece siempre (`(sin descripción)` es información útil para la IA); los comentarios, el proyecto y el contexto solo si hay.

Un detalle de la limpieza: había un `use core::task;` que no se usaba, probablemente agregado por un autocompletado del editor. Clippy lo encontró al borrar el bloque `#![allow]`.

## Paso 2: `invocation`, los argumentos como datos

```rust
let mut args: Vec<String> = ["-p", "--output-format", "json", /* ... */]
    .into_iter()
    .map(str::to_owned)
    .collect();
args.extend(["--json-schema".to_owned(), schema.to_string()]);
```

- **Un array de `&str` convertido a `Vec<String>`** escribe la parte fija en una línea por flag.
- **`extend` con un array de dos** agrega pares `flag valor` de una vez.
- **`let repo = repo.filter(|_| config.read_repo);`** junta las dos condiciones ("hay repositorio" y "se permite leerlo") en un solo `Option`. Después ese mismo `Option` decide las herramientas y la carpeta de trabajo: no pueden quedar desincronizadas.
- **`Invocation` es solo datos**: no ejecuta nada. Por eso se prueba con tests unitarios comunes, sin procesos.

## Paso 3: `run`, un proceso hijo

```rust
let mut child = command.spawn()?;                       // arranca y no espera
let mut stdin = child.stdin.take().expect("stdin is piped");
stdin.write_all(prompt.as_bytes()).await?;
drop(stdin);                                            // fin de la entrada
let output = tokio::time::timeout(timeout, child.wait_with_output()).await??;
```

- **`take()` y `drop()`.** `take` saca la entrada estándar del `Option` y te la entrega. `drop` la cierra: hasta ese momento, `claude` seguiría esperando más texto, y los dos procesos se esperarían para siempre.
- **Dos `Result` anidados:** el de `timeout` ("¿llegó a tiempo?") y el de `wait_with_output` ("¿funcionó la espera?"). Cada uno se convierte en su propio `AiError`.
- **`kill_on_drop(true)` + `timeout`:** si se agota el tiempo, `timeout` suelta el future, con él se suelta el `child`, y el proceso muere. Sin `kill_on_drop` seguiría corriendo en segundo plano.
- **`ErrorKind::NotFound` en `spawn`** distingue "no está instalado" (`Unavailable`, con un mensaje que dice qué hacer) de otros fallos.
- **Por qué la salida estándar manda.** Cuando `claude` falla dentro de la ejecución (por ejemplo, sin sesión iniciada), imprime el error como JSON en la salida estándar y termina con código distinto de 0. Por eso, si hay salida, se devuelve y `parse` decide; `stderr` solo se usa cuando no hay nada más.

## Paso 4: `parse`, un sobre genérico

```rust
let envelope: Envelope = serde_json::from_str(stdout.trim())?;     // el sobre
let structured = envelope.structured_output.ok_or_else(...)?;     // serde_json::Value
serde_json::from_value::<T>(structured)                           // el tipo pedido
```

El sobre siempre tiene la misma forma; su contenido (`structured_output`) depende de la llamada. `Value` es JSON sin tipo, y `from_value::<T>` lo convierte al tipo que pidió quien llama: `TaskSummary` en el resumen, `PlanOutput` en el plan. Es el mismo patrón genérico que `get<T>` de la fase 3.

`envelope.result.or(envelope.subtype)` elige el primer `Some`: el texto del error, o si no hay, su categoría.

## Paso 5: `SummarizeTask`, minimizar lo que sale

- **`Usage::Summary | Usage::AgentPrompt`:** el resumen y el prompt para el agente salen de la misma llamada, así que el contexto incluye las secciones de los dos usos. Con IDK, eso suma `desarrollo`, `despliegue` y `estilo-codigo` a las de resumen.
- **`resolver` vive mientras se usa `project`.** `resolve` devuelve una referencia **dentro** del resolver; si el resolver se destruyera antes, no compilaría (la lección del paso 4 de la 6A).
- **`SummaryInput` es un resumen de la tarea, no la tarea.** La descripción recortada, los últimos N comentarios y las secciones que entran en el presupuesto. Menos costo, más velocidad y menos datos de clientes fuera de tu máquina (NFR-05).

## Paso 6: `PlanMyDay`, degradación elegante

```rust
let (plan, ai_error) = match self.ctx.ai.plan_day(&input).await {
    Ok(plan) => (Some(plan), None),
    Err(error) => (None, Some(error.to_string())),
};
```

Un error de la IA se convierte en **datos del resultado**, no en un error del comando: sin plan, la lista priorizada sigue siendo útil. En cambio, si fallan ClickUp o las secciones (`?`), sí es un error, porque sin tareas no hay nada que mostrar.

Otros detalles:

- **Sin tareas, no se llama a la IA**: no tiene sentido gastar una llamada para planificar nada.
- **`flat_map` + `move`:** cada grupo se convierte en sus tareas. La closure interna usa `move` para llevarse **copias** de `project` (`Option<&str>`, que es `Copy`) y de `offset`, porque se ejecuta después, durante el `collect`.
- **`tasks` se mueve a `input` y se recupera con `input.tasks`** para ponerlo en `DailyPlan`, en lugar de clonarlo.

## Al cerrar la fase

- Se borraron los bloques `#![allow(...)]` de `application` y `adapter-claude-code`.
- `cargo test --workspace`: 335 tests. Clippy y `cargo fmt` sin avisos.
- Falta el **paso 7**: `show --ai` y `plan` con tus tareas reales.
