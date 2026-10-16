# Manual de comandos

Todos los comandos de Taskflow, qué hacen y cuándo usarlos. La ayuda corta de cada uno está en la terminal: `taskflow --help`, `taskflow <comando> --help` y `taskflow <grupo> <subcomando> --help`.

Estado: versión 0.1.0, fases 0 a 8 (2026-10-15).

## Contenido

1. [Primeros pasos](#1-primeros-pasos)
2. [El flujo de un día](#2-el-flujo-de-un-día)
3. [Tareas: `today`, `show`, `plan`, `history`, `close`](#3-tareas)
4. [Sesión de trabajo: de `start` a `done`](#4-sesión-de-trabajo)
5. [Código: `agent`, `commit`, `pr`](#5-código-agent-commit-pr)
6. [Reportes: `report`](#6-reportes-report)
7. [Proyectos: `project`](#7-proyectos-project)
8. [Contexto para la IA: `context`](#8-contexto-para-la-ia-context)
9. [Configuración: `config` y todas las claves](#9-configuración)
10. [Dónde se guarda cada cosa](#10-dónde-se-guarda-cada-cosa)
11. [Problemas frecuentes](#11-problemas-frecuentes)

Convenciones:

- `<ID>` es el ID de una tarea de ClickUp: lo que va después de `/t/` en su URL (`86bcjdn6v`).
- Opción global en todos los comandos: `--config <PATH>`, para usar otro `config.toml`.
- **Cuándo pregunta un comando:** solo en una terminal interactiva. Con `--yes`, o desde un script, no pregunta nada y usa lo sugerido.
- `--json` da la salida en JSON, para scripts.

---

## 1. Primeros pasos

Necesitas:

| Herramienta                                                                    | Para qué                                             | Comprobar                 |
| ------------------------------------------------------------------------------ | ----------------------------------------------------- | ------------------------- |
| Token de ClickUp                                                               | Leer y escribir tareas                                | `taskflow config check` |
| [Claude Code](https://claude.com/claude-code) (`claude`) con sesión iniciada | Análisis, plan, mensajes de commit, PR y comentarios | `claude --version`      |
| `git`                                                                        | Commit y push                                         | `git --version`         |
| [GitHub CLI](https://cli.github.com) (`gh`) con `gh auth login`             | Abrir PR (solo para `taskflow pr`)                  | `gh auth status`        |

```sh
taskflow config init     # asistente: proveedor, token (va al llavero) y valores por defecto
taskflow config check    # valida el archivo y prueba la conexión con ClickUp
taskflow project add     # agrupa tareas por proyecto y le asigna su repo
taskflow today           # tus tareas, por prioridad
```

El token se guarda en el llavero del sistema (servicio `taskflow`, cuenta `clickup_token`), nunca en `config.toml`. También se puede pasar en la variable `TASKFLOW_CLICKUP_TOKEN`.

---

## 2. El flujo de un día

```text
taskflow plan                 ¿por dónde empiezo?
taskflow start                eliges la tarea → contador, análisis con IA, estado sugerido
  └ revisas el análisis       aprobar y abrir el agente · aprobar · editar · rechazar
      └ taskflow agent        Claude Code en el repo, con el prompt de la sesión
          └ al salir          hacer commit → push + PR · publicar lo hecho y terminar · …
taskflow done                 estado final, comentario y contador detenido
```

En una terminal interactiva, **cada paso ofrece el siguiente**: después de `taskflow start` no necesitas escribir otro comando. Cada uno existe también por separado, para hacerlo a mano o desde un script.

---

## 3. Tareas

### `taskflow today`

Tus tareas abiertas, ordenadas por prioridad: la de ClickUp, el vencimiento y los comentarios sin responder (pesos en `policy.*`).

| Opción              | Qué hace                                     |
| -------------------- | --------------------------------------------- |
| `--limit <N>`      | Máximo de tareas                             |
| `--by-project`     | Agrupar por proyecto (ignora `ui.group_by`) |
| `--project <SLUG>` | Solo las de ese proyecto                      |
| `--json`           | JSON                                          |

```sh
taskflow today --by-project --limit 15
```

### `taskflow show <ID>`

Detalle de una tarea: descripción, subtareas, checklists y comentarios.

| Opción          | Qué hace                                                                                                                  |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `--ai`         | Agrega el análisis de la IA: qué se pide, criterios de aceptación, pasos, riesgos, preguntas y un prompt para el agente |
| `--out <FILE>` | Guarda el prompt para el agente en ese archivo (requiere `--ai`)                                                         |
| `--save`       | Guarda el análisis en el historial (requiere `--ai`)                                                                    |
| `--json`       | JSON                                                                                                                       |

```sh
taskflow show 86bcjdn6v --ai --save
```

### `taskflow plan`

Plan del día con IA: en qué orden trabajar, cuánto dedicar a cada tarea y qué desbloquear primero.

| Opción              | Qué hace                                         |
| -------------------- | ------------------------------------------------- |
| `--max <N>`        | Máximo de tareas (por defecto, `ai.max_tasks`) |
| `--project <SLUG>` | Solo ese proyecto, con su contexto                |
| `--save`           | Lo guarda en el historial                         |
| `--json`           | JSON                                              |

### `taskflow history [ID]`

Los análisis y planes guardados con `--save`, del más reciente al más antiguo.

| Opción              | Qué hace                                                 |
| -------------------- | --------------------------------------------------------- |
| `[ID]`             | Solo los análisis de esa tarea                           |
| `--project <SLUG>` | Solo los de ese proyecto                                  |
| `--plans`          | Solo los planes                                           |
| `--limit <N>`      | Máximo de registros                                      |
| `--last`           | Muestra el contenido del más reciente en vez de la lista |
| `--json`           | JSON                                                      |

### `taskflow close <ID>`

Cierra una tarea **sin sesión**: estado final y comentario opcional, con confirmación.

| Opción               | Qué hace                                                                                     |
| --------------------- | --------------------------------------------------------------------------------------------- |
| `--status <NOMBRE>` | Estado final (sin distinguir mayúsculas). Sin él, eliges de la lista de estados de la tarea |
| `--comment <TEXTO>` | Comentario de cierre                                                                          |
| `--yes`             | No pregunta: usa el estado sugerido (`provider.status_map.done`)                            |

---

## 4. Sesión de trabajo

Una sesión acompaña una tarea de principio a fin. Hay **una sola activa a la vez**. Estados:

```text
analizando → esperando aprobación → en progreso ⇄ pausada → cerrada
                     │ reject: otro análisis                (o abortada)
```

### `taskflow start [ID]`

1. Sin `ID`, eliges de tu lista priorizada.
2. Inicia el contador de ClickUp (o uno local, con `session.track_time = false`).
3. La IA analiza la tarea con el contexto del proyecto y deja el reporte en `report.md`.
4. Pasa la tarea al estado "preparado" (`provider.status_map.ready` o `[statuses]` del proyecto). Si ese estado no existe en la lista, solo avisa.
5. Prepara la carpeta de la tarea: un **worktree** en su rama `CU-<id>` (ver [Worktrees](#worktrees-una-carpeta-por-tarea)).
6. Te muestra el análisis: **aprobar y abrir el agente**, aprobar, editarlo, rechazarlo con una nota o decidir después.

| Opción     | Qué hace                                                                                  |
| ----------- | ------------------------------------------------------------------------------------------ |
| `--yes`   | No pregunta nada: detiene el contador de otra tarea, continúa la sesión anterior (si la hubo) y no ofrece la revisión del reporte |
| `--fresh` | Si la tarea ya tuvo una sesión (te la devolvieron), empieza de cero en vez de continuarla |

**Tareas devueltas.** Si la tarea ya se trabajó, `start` lo dice y ofrece continuar. Al continuar, el análisis sabe qué se hizo antes y se centra en los comentarios nuevos (qué faltó o qué hay que corregir). Al abrir el agente, Taskflow mide la conversación anterior:

- **Conversación nueva** con el análisis y el resumen de lo que se entregó. Es la opción por defecto si la anterior pasa de `session.resume_max_tokens` (100 000). Sale más barata, porque el agente solo relee los archivos que necesita.
- **Retomarla:** recuerda todo, pero carga ese historial en cada turno. Es la opción por defecto si la conversación es pequeña.

### Worktrees: una carpeta por tarea

Con `session.worktrees = true` (por defecto), **Taskflow crea el worktree, no el agente**:

- Al hacer `start` (o, si no se pudo, al abrir el agente), crea `<repo>-worktrees/CU-<id>` junto al repo, en la rama `CU-<id>`. La rama sale de la base al día (`git fetch` + `origin/master`); si ya existe, en local o en `origin`, la usa.
- Si algún worktree ya tiene la rama de la tarea (incluido el repo principal), usa ese y no crea otro.
- El agente se abre ahí, y el prompt le dice que trabaje solo en esa carpeta: que no cree otros worktrees, no cambie de rama y no haga commit.
- `commit`, `pr` y `report publish` usan esa misma carpeta. El repo principal no cambia de rama, así que puedes llevar varias tareas a la vez.
- Si el agente creó **otro** worktree por su cuenta (por ejemplo, en `.claude/worktrees/`) y dejó cambios ahí, al salir Taskflow te lo muestra y pregunta dónde hacer el commit y el PR.
- Al terminar (`done` o `abort`), si el worktree no tiene cambios sin commit, pregunta si quitarlo. La rama se queda con sus commits. Con `--yes` no quita nada: te dice el comando.
- `taskflow session` muestra la carpeta en "Carpeta de trabajo".
- **Siempre al día con `master`.** Cada vez que empieza o se retoma algo (`start`, abrir el agente, "Revisar los cambios"), Taskflow hace `git fetch` y une `origin/master` en la rama de la tarea con `git merge`, sin reescribir la historia: lo ya subido no necesita `--force`. Si la rama seguía a `origin/master`, deja de seguirla, para que un `git push` suelto no suba a `master`. Si hay cambios sin commit no toca nada y avisa; si hay conflicto, deshace la unión, avisa y te dice el comando para resolverlo a mano.

**Ojo:** un worktree solo trae lo que está en git. Lo ignorado (`.env`, `node_modules`, entornos virtuales) no aparece: cópialo o instálalo ahí si el agente lo necesita para correr el proyecto.

Para trabajar como antes, en el repo del proyecto: `taskflow config set session.worktrees false`.

### Vista previa: probar el worktree en el navegador

Si el proyecto tiene `[preview]` en su `project.toml`, el prompt del agente le dice cómo levantar la app **de ese worktree** y en qué URL. Así puede probar los cambios de interfaz con el MCP de Playwright: recorre las páginas, revisa la consola y las peticiones que fallan y toma una captura. Taskflow la detiene al terminar la sesión (`done` o `abort`).

```toml
[preview]
start = "bash '/ruta/preview.sh' start '{repo}' '{worktree}' {name} {port}"
stop = "bash '/ruta/preview.sh' stop {name}"
url = "http://localhost:{port}"
port = 8100        # cada tarea usa un puerto fijo entre 8100 y 8199
```

Las plantillas pueden usar `{repo}` (el repo principal), `{worktree}`, `{branch}` (`CU-86b1`), `{name}` (la rama en minúsculas, válida como nombre de contenedor) y `{port}`. Solo se usa en un worktree: en el repo principal ya tienes la app de siempre. `taskflow project show <slug>` la muestra.

**En IDK** (`projects/idk/preview.sh`) todo corre en Docker:

1. Arranca Postgres, Redis y Mailhog del stack local. Si ya existen, aunque sean de otro proyecto de compose, los arranca tal cual y los une a la red `idk_default`.
2. Compila el frontend del worktree en un contenedor `node:20`, con el `node_modules` del repo principal. La salida queda en el worktree, y git la ignora.
3. Levanta Django en un contenedor propio, `idk-preview-<rama>`, con la imagen `idk`, el worktree en `/app` y el `.env` del repo principal.

Comparte la base de datos con el stack local. Celery no corre en la vista previa.

### `taskflow approve`

Aprueba el reporte: la tarea pasa a "en progreso" (`provider.status_map.in_progress`).

| Opción               | Qué hace                       |
| --------------------- | ------------------------------- |
| `--status <NOMBRE>` | Otro estado en vez del sugerido |

### `taskflow reject "<nota>"`

Rechaza el análisis y pide otro a la IA con tu nota: qué falta o qué corregir.

```sh
taskflow reject "solo cambia el login, no las demás pantallas"
```

### `taskflow pause` · `taskflow resume`

Pausa la sesión y detiene el contador, o la reanuda. `resume` también reanuda un contador que detuviste a mano en ClickUp: `taskflow session` te avisa cuando pasa.

### `taskflow done`

Termina la sesión: estado final, comentario opcional y contador detenido. Archiva la sesión y, si el worktree de la tarea quedó sin cambios, ofrece quitarlo.

| Opción               | Qué hace                                                                                                                    |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `--status <NOMBRE>` | Estado final. Sin él, eliges de la lista (sugerido: `provider.status_map.done` o `done` de `[statuses]` del proyecto) |
| `--comment <TEXTO>` | Comentario de cierre                                                                                                         |
| `--yes`             | No pregunta: usa el estado sugerido                                                                                          |

### `taskflow abort`

Cancela la sesión y detiene el contador.

| Opción              | Qué hace                                         |
| -------------------- | ------------------------------------------------- |
| `--restore-status` | Devuelve la tarea al estado que tenía al empezar |
| `--yes`            | Sin confirmación                                 |

Como `done`, al final ofrece quitar el worktree de la tarea si quedó limpio.

### `taskflow session [show | status | retry]`

| Subcomando                              | Qué hace                                                                                 |
| --------------------------------------- | ----------------------------------------------------------------------------------------- |
| `session` o `session show [--json]` | Estado, tiempo, reporte, lo pendiente y el siguiente paso                                 |
| `session status <NOMBRE>`             | Cambia el estado de la tarea en ClickUp sin cambiar la sesión (por ejemplo, `blocked`) |
| `session retry`                       | Reintenta lo que quedó pendiente tras un error (o el análisis, si se cortó)            |

---

## 5. Código: `agent`, `commit`, `pr`

**El agente escribe el código; Taskflow hace el commit, el push y el PR.** La IA propone los textos y tú los revisas antes.

### `taskflow agent`

Abre Claude Code (`session.agent_command`) en la carpeta de la tarea: su worktree o, sin él, el repo del proyecto (`repo_path`). Le pasa el prompt de la sesión aprobada: el prompt de la IA, el reporte con tus ediciones, tus reglas y la tarea.

- Si ya lo abriste antes en esta sesión, **retoma la misma conversación**.
- El agente trabaja en inglés (código, comentarios y commits: `ai.agent_language`) y habla contigo en español (`ai.report_language`).
- Al salir, si dejó cambios en otro worktree, pregunta cuál usar. Luego el menú ofrece: hacer commit (si hay cambios), subir y crear el PR (si todo está commiteado y no hay PR), publicar lo hecho y terminar, terminar, volver al agente o dejar la sesión abierta.
- **Revisar los cambios** (primera opción si hay cambios y aún no se revisó): retoma la conversación del agente con un pedido de revisión. Revisa su diff con el subagente revisor del repo (o la skill `code-review`) contra el plan y tus reglas, corrige lo real, corre tests y linter, y te cuenta qué encontró. Después, el commit pasa a ser la primera opción.

| Opción     | Qué hace                                       |
| ----------- | ----------------------------------------------- |
| `--print` | Solo imprime el prompt completo, sin abrir nada |

### `taskflow commit`

1. Muestra los archivos con cambios. Vienen marcados los que git ya conoce; **los nuevos, sin marcar** (pueden ser tuyos y no de la tarea).
2. La IA propone el mensaje con la convención de los últimos commits del repo (`CU-<id> - feat: …`).
3. Puedes hacer el commit, editar el título, editar el mensaje completo en tu editor o cancelar.
4. `git add` + `git commit` **solo de esos archivos**. Los hooks del repo se ejecutan; si uno falla, ves su salida.
5. Después, lo que diga `session.after_commit` (ver abajo).

| Opción   | Qué hace                                                            |
| --------- | -------------------------------------------------------------------- |
| `--yes` | Sin preguntar: los archivos que git ya conoce y el mensaje propuesto |

### `taskflow pr`

Sube la rama y abre el pull request en GitHub.

- **Título:** `CU-<id> - <título de la tarea>`, sin IA. Se puede editar en el menú.
- **Descripción:** la escribe la IA (el mismo agente, si hay conversación). Sigue la plantilla del repo (`.github/pull_request_template.md`), en inglés y con el enlace a la tarea.
- **Si la rama ya tiene PR,** solo hace el push, que lo actualiza.
- **No abre PR** desde la rama base (`master`/`main`) ni si la rama no tiene commits nuevos.
- Menú: crear el PR, crearlo como borrador, editar el título, editar la descripción en tu editor, solo push o cancelar.

| Opción     | Qué hace                                                  |
| ----------- | ---------------------------------------------------------- |
| `--yes`   | Sin preguntar: lo abre con la descripción propuesta       |
| `--draft` | Como borrador (o siempre, con `session.pr_draft = true`) |

**Push y PR automáticos después del commit** (`session.after_commit`):

| Valor                 | Qué pasa después de `taskflow commit`                                                |
| --------------------- | ---------------------------------------------------------------------------------------- |
| `none`              | Nada                                                                                     |
| `ask` (por defecto) | Pregunta: push y PR, solo push o nada                                                    |
| `push`              | Solo `git push -u origin HEAD`                                                         |
| `pr`                | Push y, si la rama no tiene PR, lo abre (ves la descripción antes, salvo con `--yes`) |

```sh
taskflow config set session.after_commit pr
```

Si el push o el PR fallan, el commit queda hecho. Reinténtalo con `taskflow pr`.

---

## 6. Reportes: `report`

Cada sesión tiene dos reportes:

| Archivo            | Qué es                                                                                                                        | Quién lo lee   |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------ | --------------- |
| `report.md`      | El **plan**: el análisis de la IA, con tus ediciones                                                                    | Tú y el agente |
| `work-report.md` | **Lo que se hizo**: el comentario de entrega, en tono natural, con los pasos para probarlo y el enlace a la rama y al PR | QA, en ClickUp  |

| Comando                  | Qué hace                                                                                                                                                                                           |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `report show [--work]` | Muestra el plan, o con `--work` lo que se hizo                                                                                                                                                    |
| `report edit [--work]` | Lo abre en tu editor (`$VISUAL`, `$EDITOR` o `vi`)                                                                                                                                            |
| `report publish`       | Publica lo que se hizo como comentario en la tarea, con formato. Si no existe, lo escribe la IA (el mismo agente, desde su conversación). Te lo muestra y pregunta: publicar, editar o no publicar |

Opciones de `report publish`:

| Opción          | Qué hace                                     |
| ---------------- | --------------------------------------------- |
| `--regenerate` | Lo vuelve a escribir aunque ya exista         |
| `--dry-run`    | Solo lo escribe y lo muestra: no publica nada |
| `--yes`        | Publica sin preguntar                         |

---

## 7. Proyectos: `project`

Un proyecto agrupa tareas por reglas (espacio, carpeta, lista o etiqueta de ClickUp). Además tiene su repo (`repo_path`), su contexto para la IA y sus estados sugeridos. Vive en `projects/<slug>/project.toml`.

| Comando                            | Qué hace                                                                         |
| ---------------------------------- | --------------------------------------------------------------------------------- |
| `project list [--json]`          | Todos los proyectos con sus reglas                                                |
| `project show [SLUG] [--json]`   | Un proyecto. Sin `SLUG`, el de `projects.default`                             |
| `project add`                    | Asistente: eliges espacios, carpetas o listas de ClickUp y crea el proyecto       |
| `project rename <VIEJO> <NUEVO>` | Cambia el slug: mueve la carpeta, sus secciones, su historial y la sesión activa |

Ejemplo de `project.toml`:

```toml
slug = "idk"
name = "IDK - Dashboard"
kind = "internal"
repo_path = "~/swapps/idk"     # donde se abren el agente, el commit y el PR

[[match]]                      # una tarea es del proyecto si cumple alguna regla
level = "folder"
id = "1116732"

[[match]]
tag = "idk-todo"

[context]
exclude = ["aprendizaje-rust"] # secciones globales que no aplican aquí

[preview]                      # opcional: la app de cada worktree, para probar en el navegador
start = "…"                    # ver "Vista previa" en la sección 4
url = "http://localhost:{port}"

[statuses]                     # opcional: estados sugeridos propios del proyecto
ready = ["needs approval"]     # tras el análisis (también: in_progress, done)
done = ["code review", "qa"]
```

---

## 8. Contexto para la IA: `context`

Las **secciones** son archivos Markdown con lo que la IA debe saber (conocimiento) o cómo debe responder (instrucciones). Las hay globales y por proyecto. La guía completa está en [contexto-por-proyecto.md](contexto-por-proyecto.md).

| Comando                                             | Qué hace                                                                                                                                       |
| --------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `context list [--project SLUG] [--json]`          | Las secciones globales, o las que aplican a un proyecto (marca las globales excluidas)                                                          |
| `context preview <ID> [--usage USO] [--json]`     | Exactamente lo que recibiría la IA para esa tarea, sin llamarla. `USO`: `summary` (por defecto), `plan`, `agent_prompt`, `playbook` o `commit` |
| `context edit <CLAVE> [--project SLUG]`           | La abre en tu editor; si no existe, la crea con una plantilla. Al guardar, la valida                                                            |
| `context remove <CLAVE> [--project SLUG] [--yes]` | La borra, con confirmación                                                                                                                     |
| `context exclude <CLAVE> --project SLUG`          | Una sección global deja de aplicar a ese proyecto                                                                                              |
| `context include <CLAVE> --project SLUG`          | Deshace `exclude`                                                                                                                             |

La `<CLAVE>` es el nombre del archivo sin `.md` (`negocio`, `aprendizaje-rust`). Sin `--project`, se trabaja con una sección global.

---

## 9. Configuración

| Comando                        | Qué hace                                                                                                    |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------ |
| `config init`                | Asistente: proveedor, token y valores por defecto                                                            |
| `config path`                | Dónde está `config.toml` (en macOS, `~/Library/Application Support/taskflow/config.toml`)              |
| `config show`                | Todas las claves con su valor (`(por defecto)` si no está en el archivo)                                  |
| `config get <CLAVE>`         | El valor de una clave                                                                                        |
| `config set <CLAVE> <VALOR>` | La cambia conservando los comentarios del archivo. Valida antes de escribir: un valor inválido no se guarda |
| `config check`               | Valida la configuración y prueba la conexión con ClickUp                                                   |

Los valores de `config set` pueden ser texto, números, `true`/`false` o listas:

```sh
taskflow config set ai.effort high
taskflow config set provider.status_map.in_review '["code review", "qa"]'
```

### Todas las claves

| Clave                               | Por defecto                | Qué controla                                                      |
| ----------------------------------- | -------------------------- | ------------------------------------------------------------------ |
| `provider.kind`                   | `"clickup"`              | El gestor de tareas                                                |
| `provider.clickup.team_id`        | —                         | El Workspace de ClickUp                                            |
| `provider.status_map.todo`        | `["to do", "open"]`      | Qué estados cuentan como "por hacer"                              |
| `provider.status_map.ready`       | `["preparado"]`          | Estado sugerido tras el análisis de `start`                     |
| `provider.status_map.in_progress` | `["in progress"]`        | Estado sugerido al aprobar                                         |
| `provider.status_map.in_review`   | `["review", "qa"]`       | Estados de revisión (suman `policy.in_review_bonus`)            |
| `provider.status_map.done`        | `["complete", "closed"]` | Estados finales (los sugiere `done` y `close`)                 |
| `policy.priority_weight`          | `10`                     | Peso de la prioridad de ClickUp en el orden                        |
| `policy.overdue_bonus`            | `50`                     | Puntos extra si está vencida                                      |
| `policy.due_today_bonus`          | `30`                     | … si vence hoy                                                    |
| `policy.due_this_week_bonus`      | `10`                     | … si vence esta semana                                            |
| `policy.pending_comment_weight`   | `15`                     | Por cada comentario sin responder                                  |
| `policy.in_review_bonus`          | `5`                      | Si está en revisión                                              |
| `projects.dir`                    | `"projects"`             | Carpeta de proyectos (relativa a `config.toml`, o absoluta)      |
| `projects.default`                | —                         | El proyecto de `project show` sin slug                           |
| `context.global_dir`              | `"context"`              | Carpeta de las secciones globales                                  |
| `context.budget_chars`            | `12000`                  | Caracteres máximos de contexto por llamada a la IA                |
| `ai.kind`                         | `"claude_code"`          | El proveedor de IA                                                 |
| `ai.model`                        | — (el de Claude Code)     | Modelo: `"sonnet"`, `"opus"`…                                 |
| `ai.effort`                       | —                         | Esfuerzo: `low`, `medium`, `high`, `xhigh` o `max`       |
| `ai.timeout_secs`                 | `300`                    | Espera máxima por respuesta                                       |
| `ai.max_tasks`                    | `10`                     | Tareas que recibe `plan`                                         |
| `ai.max_description_chars`        | `4000`                   | Caracteres de la descripción que se envían                       |
| `ai.max_comments`                 | `10`                     | Comentarios recientes que se envían                               |
| `ai.read_repo`                    | `true`                   | Si la IA puede leer el repo del proyecto en el análisis           |
| `ai.report_language`              | `"español"`             | Idioma de lo que lees tú: análisis, plan, comentarios en ClickUp |
| `ai.agent_language`               | `"English"`              | Idioma de lo que queda en el repo: código, commits, PR            |
| `history.dir`                     | `"history"`              | Carpeta del historial (`--save`)                                 |
| `session.dir`                     | `"sessions"`             | Carpeta de la sesión activa, sus reportes y el archivo            |
| `session.track_time`              | `true`                   | Usar el contador de ClickUp (`false`: solo en la sesión)        |
| `session.notify`                  | `true`                   | Notificaciones de escritorio                                       |
| `session.agent_command`           | `"claude"`               | Lo que abre `taskflow agent`                                     |
| `session.after_commit`            | `"ask"`                  | Después del commit: `none`, `ask`, `push` o `pr`          |
| `session.pr_draft`                | `false`                  | Abrir los PR como borrador                                         |
| `session.worktrees`               | `true`                   | Un worktree por tarea, en su rama `CU-<id>` (lo crea Taskflow)    |
| `session.worktree_dir`            | `""`                     | Dónde crearlos (absoluta o relativa al repo). Vacía: `<repo>-worktrees` junto al repo |
| `session.resume_max_tokens`       | `100000`                 | Tarea devuelta: desde cuántos tokens de la conversación anterior se sugiere una nueva |
| `ui.group_by`                     | `"none"`                 | Agrupar `today`: `none` o `project`                          |

---

## 10. Dónde se guarda cada cosa

Todo cuelga de la carpeta de `config.toml` (`taskflow config path`):

```text
config.toml
projects/<slug>/project.toml         el proyecto
projects/<slug>/context/*.md         sus secciones de contexto
context/*.md                         secciones globales
history/<slug>/analyses/<id>/…md     análisis guardados (show --ai --save)
history/<slug>/plans/…md             planes guardados (plan --save)
sessions/active.json                 la sesión activa
sessions/<id>/report.md              el plan de la sesión
sessions/<id>/agent-prompt.md        el prompt para el agente
sessions/<id>/work-report.md         lo que se hizo (el comentario de entrega)
sessions/archive/<fecha>_<id>.json   sesiones terminadas
```

Los worktrees no van ahí: están junto al repo, en `<repo>-worktrees/CU-<id>` (o en `session.worktree_dir`).

---

## 11. Problemas frecuentes

| Síntoma                                      | Qué hacer                                                                                                    |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| "⚠ El contador no corre en el proveedor"     | Lo detuviste en ClickUp: `taskflow resume` lo reanuda                                                       |
| Un comando se cortó a mitad (red, IA)        | `taskflow session retry`                                                                                    |
| "429" o lentitud de ClickUp                   | Límite de 100 peticiones por minuto; Taskflow espera lo que indica ClickUp. Vuelve a intentarlo en un minuto |
| El estado sugerido "no existe en la lista"    | Ajusta `provider.status_map.*` o `[statuses]` del proyecto con los nombres reales de ClickUp              |
| `AI provider unavailable: claude not found` | Instala Claude Code e inicia sesión                                                                          |
| `could not run gh … gh auth login`         | Instala la GitHub CLI y ejecuta `gh auth login`                                                             |
| `git push failed`                           | Revisa el mensaje de git (permisos, rama protegida, hook de pre-push) y reintenta con `taskflow pr`         |
| La IA usa contexto que no aplica al proyecto  | `taskflow context exclude <clave> --project <slug>`, y compruébalo con `context preview`                 |
| "no se pudo preparar el worktree"           | Lee el mensaje de git (la carpeta ya existe, la rama está en otro worktree…). La sesión sigue en el repo del proyecto |
| En el worktree falta `.env` o las dependencias | Un worktree solo trae lo que está en git: cópialos o instálalos ahí, o usa una vista previa (`[preview]`) que los tome del repo principal |
| La vista previa no responde                   | `docker logs idk-preview-<rama>`; se vuelve a levantar con el comando `start` del prompt |
| macOS pide permiso del llavero en cada build  | Ya no debería: Taskflow lee el token con `/usr/bin/security`. Dale "Permitir siempre" una vez a `security` |
| Quieres ver qué recibe la IA                 | `taskflow context preview <ID> --usage agent_prompt` o `taskflow agent --print`                           |
