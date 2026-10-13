# Taskflow — Documento de arquitectura de software

*Versión 0.5 · 2026-10-10 · Estado: borrador*

> **Cambios en 0.3:** configuración persistente con asistente interactivo, proyectos (cliente/proyecto) y agrupación de tareas, contexto por proyecto en secciones abiertas, automatizaciones con **Playbooks** y patrones de diseño de Rust asociados.
>
> **Cambios en 0.4:** sesión de trabajo (`taskflow start`): contador de tiempo, análisis con IA, reporte de trabajo, estados por etapa, luz verde, notificaciones y cierre. Modelada como máquina de estados con enums que devuelve efectos.
>
> **Cambios en 0.5:** estados del flujo variables (sugerencias + elección y validación por tarea), cierre siempre explícito del usuario, reporte como borrador local con publicación explícita.

## 1. Propósito del documento

Este documento describe qué debe hacer Taskflow, cómo se organiza internamente y en qué orden se construye. Sirve como guía de implementación y como material de estudio de Rust: cada decisión técnica indica qué concepto del lenguaje aplica.

**Convención de idioma:** la explicación y los requisitos están en español. Comandos, código, nombres de tipos, módulos y crates están en inglés.

## 2. Contexto y problema

Hoy la revisión diaria de tareas es manual y repetitiva:

1. Entrar a ClickUp y revisar tareas asignadas, su estado y fechas.
2. Revisar comentarios asignados o menciones pendientes.
3. Decidir la prioridad de cada tarea.
4. Leer cada tarea para entender qué se solicita y qué se puede resolver.
5. Trabajar en VS Code con ayuda de un agente de IA.
6. Revisar el resultado del agente y cerrar la tarea en ClickUp.
7. Ejecutar procesos operativos repetitivos. Ejemplo: "revisa y despliega la nueva release" implica desplegar, revisar commits y PRs, comentar las tareas incluidas y cerrarlas.

Además, las tareas pertenecen a distintos **proyectos o clientes** (en ClickUp, un espacio, carpeta o lista), y cada proyecto tiene su propio contexto: modelo de negocio, código, despliegue. Hoy ese contexto vive en la cabeza del desarrollador y la IA no lo conoce.

Taskflow automatiza los pasos 1 a 4, prepara el contexto del paso 5 con el conocimiento del proyecto, ejecuta el paso 6 desde la herramienta y convierte los procesos del paso 7 en **Playbooks** reproducibles.

Cuando el usuario decide trabajar una tarea concreta, los pasos 4 a 6 se unen en una **sesión de trabajo** (`taskflow start`): se inicia el contador de tiempo, la IA analiza la tarea y deja un reporte de lo que hay que hacer, la tarea pasa a "preparado", el usuario da luz verde, se trabaja y, cuando el usuario indica explícitamente que terminó, elige el estado final, se detiene el contador y la sesión se cierra.

## 3. Objetivos

| Tipo         | Objetivo                                                                                                                     |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| Producto     | Reducir la revisión matutina de tareas a un comando                                                                         |
| Producto     | Ver el trabajo organizado por proyecto o cliente                                                                             |
| Producto     | Entregar al agente de VS Code un contexto claro de cada tarea, incluido el contexto del proyecto                             |
| Producto     | Convertir procesos operativos repetitivos en Playbooks explícitos y supervisados                                            |
| Producto     | Iniciar el trabajo en una tarea con un comando que registra el tiempo, prepara el análisis y guía la tarea hasta su cierre |
| Producto     | Configurar la herramienta de forma interactiva, sin editar archivos a mano                                                   |
| Arquitectura | Independencia total del proveedor de tareas (ClickUp hoy, otro mañana)                                                      |
| Arquitectura | Independencia total de la interfaz (CLI hoy, escritorio o web mañana)                                                       |
| Aprendizaje  | Aplicar de forma deliberada los conceptos centrales de Rust y patrones de proyectos profesionales                            |

**No objetivos (v1):** interfaz gráfica, ejecución automática del agente, sincronización entre varios proveedores a la vez, uso multiusuario, Playbooks programados o sin supervisión (cron), lenguaje de scripting embebido.

## 4. Requisitos funcionales

Prioridad según MoSCoW: **Debe** (imprescindible en v1), **Debería** (importante, no bloqueante), **Podría** (deseable si hay tiempo).

### 4.1 Núcleo: tareas, IA y cierre

| ID    | Requisito                                                                                                                                          | Prioridad |
| ----- | -------------------------------------------------------------------------------------------------------------------------------------------------- | --------- |
| FR-01 | El sistema debe obtener todas las tareas abiertas asignadas al usuario actual desde el proveedor de tareas configurado.                            | Debe      |
| FR-02 | El sistema debe obtener, para cada tarea, su estado, prioridad, fecha de vencimiento, descripción, URL, ubicación en el proveedor y comentarios. | Debe      |
| FR-03 | El sistema debe identificar los comentarios asignados al usuario actual que aún no están resueltos.                                              | Debe      |
| FR-04 | El sistema debe ordenar las tareas usando una política de priorización configurable.                                                             | Debe      |
| FR-05 | El sistema debe mostrar la lista priorizada con estado, prioridad, fecha de vencimiento y cantidad de comentarios pendientes.                      | Debe      |
| FR-06 | El sistema debe generar, mediante un proveedor de IA, un resumen por tarea: qué se solicita, pasos sugeridos y preguntas abiertas.                | Debe      |
| FR-07 | El sistema debe generar un plan del día a partir de la lista priorizada.                                                                          | Debe      |
| FR-08 | El sistema debe generar, para una tarea, un prompt listo para el agente de IA.                                                                     | Debe      |
| FR-09 | El sistema debería exportar el prompt del agente al portapapeles o a un archivo.                                                                  | Debería  |
| FR-10 | El sistema debe cambiar el estado de una tarea al estado "done" configurado.                                                                       | Debe      |
| FR-11 | El sistema debería publicar un comentario de cierre en la tarea.                                                                                  | Debería  |
| FR-12 | El sistema debe pedir confirmación explícita antes de cualquier operación de escritura.                                                         | Debe      |
| FR-13 | El sistema debe permitir elegir el proveedor de tareas y el proveedor de IA solo mediante configuración.                                          | Debe      |
| FR-14 | El sistema debe mapear los estados propios del proveedor a los estados del dominio mediante configuración.                                        | Debe      |
| FR-15 | El sistema debería exportar el resultado de cualquier consulta en JSON.                                                                           | Debería  |
| FR-16 | El sistema podría guardar el plan del día como archivo Markdown.                                                                                 | Podría   |
| FR-17 | El sistema podría guardar en caché local los datos de tareas y los resúmenes de IA para evitar llamadas repetidas.                              | Podría   |

### 4.2 Configuración persistente

| ID    | Requisito                                                                                                                                                                                                                                   | Prioridad |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------- |
| FR-18 | El sistema debe almacenar la configuración en un archivo TOML persistente, en la ubicación estándar del sistema operativo, organizado en secciones (`provider`, `ai`, `policy`, `projects`, `context`, `playbooks`, `ui`). | Debe      |
| FR-19 | El sistema debe ofrecer un asistente interactivo (`config init`) que permita elegir el proveedor de tareas y el de IA de una lista de proveedores disponibles, y que solicite los datos que cada proveedor requiere.                      | Debe      |
| FR-20 | El sistema debe permitir cambiar de proveedor de forma interactiva (`config provider`) sin reconfigurar el resto de secciones.                                                                                                            | Debe      |
| FR-21 | El sistema debe permitir leer y actualizar una clave o sección individual desde el CLI (`config get`, `config set`), preservando los comentarios y el formato del resto del archivo.                                                   | Debe      |
| FR-22 | El sistema debe validar la configuración antes de guardarla, rechazar valores inválidos indicando la clave y el motivo, y nunca dejar en disco un archivo inválido.                                                                      | Debe      |
| FR-23 | El asistente debería guardar los secretos en el llavero del sistema operativo; las variables de entorno tienen precedencia sobre el llavero.                                                                                               | Debería  |
| FR-24 | El sistema debería abrir la configuración en el editor del sistema (`$EDITOR`) y validarla al cerrarlo.                                                                                                                                 | Debería  |
| FR-25 | El sistema podría versionar el esquema de configuración y migrar automáticamente archivos de versiones anteriores.                                                                                                                       | Podría   |

### 4.3 Proyectos y agrupación

Un **proyecto** representa un cliente o proyecto de trabajo. En ClickUp se identifica por un espacio, una carpeta o una lista; en otro proveedor, por su equivalente (por ejemplo, un repositorio en GitHub).

| ID    | Requisito                                                                                                                                                                                                               | Prioridad |
| ----- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------- |
| FR-26 | El sistema debe permitir definir proyectos con un identificador corto (`slug`), un nombre, un tipo (cliente o interno) y reglas que lo asocian a ubicaciones del proveedor (espacio, carpeta, lista) o a etiquetas.   | Debe      |
| FR-27 | El sistema debe asignar cada tarea a un proyecto según esas reglas; si varias reglas coinciden, gana la más específica (lista > carpeta > espacio > etiqueta). Las tareas sin coincidencia quedan en "Sin proyecto". | Debe      |
| FR-28 | El sistema debe mostrar las tareas agrupadas por proyecto y permitir filtrar cualquier consulta por proyecto.                                                                                                           | Debe      |
| FR-29 | El sistema debería permitir crear un proyecto de forma interactiva, eligiendo sus ubicaciones del árbol obtenido del propio proveedor.                                                                                | Debería  |
| FR-30 | El sistema debería permitir asociar un repositorio local a cada proyecto, usado como directorio de trabajo de los Playbooks y como referencia en el prompt del agente.                                                 | Debería  |
| FR-31 | El sistema podría aplicar una bonificación de prioridad por proyecto dentro de la política de priorización.                                                                                                         | Podría   |

### 4.4 Contexto por proyecto

El contexto de un proyecto se compone de **secciones**: archivos independientes que describen un aspecto del proyecto o dan instrucciones a la IA. Ejemplos: modelo de negocio, código, despliegue, aprendizaje ("estoy aprendiendo traits").

| ID    | Requisito                                                                                                                                                                                                    | Prioridad |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------- |
| FR-32 | Cada proyecto debe tener un contexto compuesto por secciones, cada una en su propio archivo Markdown.                                                                                                        | Debe      |
| FR-33 | Las secciones deben ser abiertas: el usuario puede crear tantas como quiera y con cualquier nombre.                                                                                                          | Debe      |
| FR-34 | Cada sección debe declarar su rol:**conocimiento** (describe el proyecto) o **instrucción** (indica a la IA cómo responder, por ejemplo adaptar las propuestas a un objetivo de aprendizaje). | Debe      |
| FR-35 | Cada sección debe declarar en qué usos se incluye: resumen, plan del día, prompt del agente o pasos de IA de los Playbooks.                                                                               | Debe      |
| FR-36 | El sistema debe admitir secciones globales que aplican a todos los proyectos; una sección del proyecto con la misma clave reemplaza a la global.                                                            | Debe      |
| FR-37 | El sistema debe incluir automáticamente las secciones correspondientes al generar resúmenes, planes, prompts del agente y pasos de IA.                                                                     | Debe      |
| FR-38 | El sistema debe respetar un presupuesto de tamaño de contexto, incluir las secciones por orden de prioridad e informar cuáles se omitieron.                                                                | Debe      |
| FR-39 | Las secciones marcadas como privadas nunca deben enviarse a un proveedor de IA.                                                                                                                              | Debe      |
| FR-40 | El sistema debería permitir listar, crear desde plantilla y editar secciones, y previsualizar el contexto exacto que recibirá la IA para una tarea.                                                        | Debería  |

### 4.5 Playbooks (automatizaciones)

Un **Playbook** es una secuencia con nombre de pasos explícitos, definida por el usuario dentro de un proyecto, que automatiza un proceso operativo: ejecutar comandos de shell, consultar tareas, comentar, cambiar estados, pedir un análisis a la IA y solicitar confirmaciones.

| ID    | Requisito                                                                                                                                                                                               | Prioridad |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------- |
| FR-41 | El sistema debe permitir definir Playbooks por proyecto en archivos TOML dentro del directorio del proyecto, con nombre, descripción, entradas y pasos.                                                | Debe      |
| FR-42 | El sistema debe soportar estos tipos de paso:`shell`, `select_tasks`, `extract_task_ids`, `comment`, `set_status`, `ai` y `confirm`.                                                      | Debe      |
| FR-43 | Los pasos deben poder usar plantillas con variables: entradas del Playbook, salidas de pasos anteriores y datos del proyecto.                                                                           | Debe      |
| FR-44 | Un paso debe poder repetirse por cada elemento de una colección (`for_each`) y ejecutarse de forma condicional (`when`).                                                                           | Debe      |
| FR-45 | El sistema debe validar el Playbook completo al cargarlo (identificadores únicos, referencias existentes, plantillas válidas) e informar todos los errores antes de ejecutar nada.                    | Debe      |
| FR-46 | El sistema debe mostrar el plan de ejecución y pedir confirmación antes de ejecutarlo; los pasos marcados con`confirm` piden además confirmación individual. Amplía FR-12.                       | Debe      |
| FR-47 | El sistema debe ofrecer un modo de simulación (`--dry-run`) que muestre los comandos y operaciones resueltos sin ejecutar nada.                                                                      | Debe      |
| FR-48 | El sistema debe mostrar la salida de cada paso en vivo, detenerse ante un error salvo que el paso indique`on_error = "continue"`, y terminar con un resumen de pasos ejecutados, omitidos y fallidos. | Debe      |
| FR-49 | El sistema debería guardar un registro de cada ejecución (entradas, pasos, salidas, resultado) en un archivo.                                                                                         | Debería  |
| FR-50 | El sistema debería permitir listar, ver, validar y crear Playbooks desde plantillas.                                                                                                                   | Debería  |
| FR-51 | El sistema podría permitir acciones nativas escritas en Rust (trait`Action`) como tipo de paso `custom`, para lógica que no cabe en pasos declarativos.                                           | Podría   |

### 4.6 Sesión de trabajo (`start`)

Una **sesión de trabajo** acompaña una tarea desde que el usuario decide trabajarla hasta que la cierra:

```text
start ─► contador ON ─► datos (caché o API) ─► análisis IA ─► reporte (borrador local) ─► estado "preparado" ─► notificación
      ─► luz verde (approve) ─► estado de trabajo ─► trabajo (usuario + agente)
      ─► [opcional y explícito] editar y publicar el reporte como comentario
      ─► el usuario indica que terminó (done) ─► elige el estado final ─► contador OFF ─► notificación
```

Los nombres de los estados **no son fijos**: dependen de la lista de ClickUp de cada tarea (por ejemplo, "Code Review" en una y "Ready For Testing" en otra) y pueden cambiar con el tiempo. Por eso la configuración solo guarda **sugerencias**, y el estado real se elige y se valida en cada transición.

| ID    | Requisito                                                                                                                                                                                                                                                                                                      | Prioridad |
| ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------- |
| FR-52 | El sistema debe iniciar una sesión de trabajo sobre una tarea con`taskflow start <TASK_ID>`, o permitir elegirla de forma interactiva de la lista priorizada si no se indica el ID.                                                                                                                         | Debe      |
| FR-53 | Al iniciar la sesión, el sistema debe activar el contador de tiempo del proveedor para esa tarea.                                                                                                                                                                                                             | Debe      |
| FR-54 | El sistema debe obtener la tarea completa (descripción, comentarios y, si el proveedor los ofrece, subtareas y listas de verificación) y resolver su proyecto.                                                                                                                                               | Debe      |
| FR-55 | El sistema debería reutilizar los datos y el análisis en caché si la tarea no cambió desde el último análisis (misma fecha de actualización y mismo contexto).                                                                                                                                          | Debería  |
| FR-56 | El sistema debe analizar la tarea con IA usando el contexto del proyecto y producir un**reporte de trabajo**: qué se solicita, criterios de aceptación, pasos propuestos, riesgos, preguntas abiertas, complejidad y prompt para el agente.                                                            | Debe      |
| FR-57 | El sistema debe mostrar el reporte y guardarlo como Markdown en el directorio de la sesión. El reporte queda como borrador local y nunca se publica automáticamente.                                                                                                                                         | Debe      |
| FR-58 | Con el reporte listo, el sistema debe cambiar la tarea al estado sugerido para "preparado" (si existe en la lista de la tarea; si no, avisar y mantener el estado actual) y notificar al usuario que está lista para revisión.                                                                               | Debe      |
| FR-59 | La sesión debe esperar la luz verde explícita del usuario (`taskflow approve`); solo entonces debe cambiar la tarea al estado de trabajo (el sugerido o el indicado con `--status`) y entregar el prompt del agente (pantalla, portapapeles o archivo).                                                  | Debe      |
| FR-60 | El usuario debería poder rechazar el reporte con indicaciones (`taskflow reject --note`) para que el análisis se repita teniéndolas en cuenta.                                                                                                                                                            | Debería  |
| FR-61 | Solo el usuario debe poder indicar que la tarea terminó, de forma explícita con`taskflow done`; ninguna herramienta externa (por ejemplo, el agente) puede cerrar la sesión ni marcarla como terminada.                                                                                                   | Debe      |
| FR-62 | Al finalizar (`taskflow done`), el sistema debe pedir el estado final (con `--status` o eligiéndolo de los estados disponibles en la lista de la tarea, con las sugerencias primero), permitir un comentario de cierre opcional, pedir confirmación, cambiar el estado, detener el contador y notificar. | Debe      |
| FR-63 | El sistema debería permitir pausar y reanudar la sesión, deteniendo y reanudando el contador.                                                                                                                                                                                                                | Debería  |
| FR-64 | El sistema debería permitir abortar la sesión, deteniendo el contador y, opcionalmente, restaurando el estado original de la tarea.                                                                                                                                                                          | Debería  |
| FR-65 | La sesión debe persistir en disco para continuar entre ejecuciones y reinicios;`taskflow session` debe mostrar el estado actual, el tiempo acumulado y las acciones posibles.                                                                                                                               | Debe      |
| FR-66 | Solo debe existir una sesión activa a la vez; al iniciar otra, el sistema debe ofrecer pausar o terminar la actual.                                                                                                                                                                                           | Debe      |
| FR-67 | Los estados del proveedor no deben estar fijos: la configuración global y la de cada proyecto solo guardan**sugerencias** por etapa (preparado, trabajo, final), modificables en cualquier momento, y cada transición puede usar otro estado indicado por el usuario.                                  | Debe      |
| FR-68 | El sistema debe notificar al menos con notificación de escritorio y en la terminal; los canales deben ser configurables.                                                                                                                                                                                      | Debe      |
| FR-69 | El sistema podría ejecutar un Playbook del proyecto en etapas de la sesión (`on_start`, `on_approved`, `on_done`), por ejemplo para crear la rama de la tarea.                                                                                                                                         | Podría   |
| FR-70 | El sistema debe validar cualquier estado destino contra los estados disponibles en la lista de la tarea en ese momento e informar las opciones válidas si no existe.                                                                                                                                          | Debe      |
| FR-71 | El sistema debe permitir publicar el reporte de trabajo como comentario en la tarea solo de forma explícita (`taskflow report publish`), mientras la sesión esté activa.                                                                                                                                  | Debe      |
| FR-72 | El sistema debería permitir editar el reporte antes de publicarlo (`taskflow report edit`); se publica la versión editada.                                                                                                                                                                                 | Debería  |
| FR-73 | Al ejecutar`done` con el reporte sin publicar, el sistema debería preguntar si se desea publicarlo antes de cerrar (por defecto, no).                                                                                                                                                                       | Debería  |
| FR-74 | El sistema debería permitir cambiar el estado de la tarea en cualquier momento de la sesión (`taskflow session status [STATUS]`).                                                                                                                                                                          | Debería  |

## 5. Requisitos no funcionales

| ID     | Requisito                                                                                                                                                                                                                                                 | Categoría     |
| ------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- |
| NFR-01 | La lógica de negocio no debe depender de ningún proveedor de tareas, proveedor de IA ni interfaz de usuario específicos.                                                                                                                               | Mantenibilidad |
| NFR-02 | Agregar un nuevo proveedor de tareas debe requerir solo un nuevo crate adaptador y una entrada de configuración, sin cambios en`domain` ni `application`.                                                                                            | Extensibilidad |
| NFR-03 | Agregar una nueva interfaz de usuario (escritorio, web) debe reutilizar`application` y `bootstrap` sin modificarlos.                                                                                                                                  | Extensibilidad |
| NFR-04 | Los secretos nunca deben guardarse en el código fuente, en el archivo de configuración ni subirse al repositorio.                                                                                                                                       | Seguridad      |
| NFR-05 | El proveedor de IA debe recibir solo los campos de la tarea y las secciones de contexto necesarias para cada uso.                                                                                                                                         | Privacidad     |
| NFR-06 | El caso de uso`today` debe completarse en menos de 5 segundos para hasta 50 tareas en condiciones normales de red.                                                                                                                                      | Rendimiento    |
| NFR-07 | Las peticiones independientes a las APIs deben ejecutarse de forma concurrente, respetando los límites de peticiones del proveedor.                                                                                                                      | Rendimiento    |
| NFR-08 | El sistema debe compilar y ejecutarse en Windows, Linux y macOS con un único código fuente.                                                                                                                                                             | Portabilidad   |
| NFR-09 | `domain` y `application` deben poder probarse por completo sin acceso a red ni ejecución de procesos reales.                                                                                                                                         | Testeabilidad  |
| NFR-10 | Todo error mostrado al usuario debe indicar qué falló y sugerir una acción correctiva.                                                                                                                                                                 | Usabilidad     |
| NFR-11 | El código debe pasar`cargo fmt --check` y `cargo clippy -- -D warnings`.                                                                                                                                                                             | Calidad        |
| NFR-12 | Los Playbooks solo deben ejecutar comandos definidos en archivos locales del usuario; un texto generado por la IA nunca debe ejecutarse como comando.                                                                                                     | Seguridad      |
| NFR-13 | Las entradas y variables marcadas como secretas deben ocultarse en pantalla, en los logs y en los registros de ejecución.                                                                                                                                | Seguridad      |
| NFR-14 | Toda escritura de configuración, contexto o Playbooks debe ser atómica (archivo temporal y renombrado) para no dejar archivos corruptos.                                                                                                                | Fiabilidad     |
| NFR-15 | Cada comando de shell debe tener un tiempo máximo configurable y poder cancelarse con Ctrl+C sin dejar procesos huérfanos.                                                                                                                              | Robustez       |
| NFR-16 | La configuración, el contexto y los Playbooks deben ser texto plano legible y versionable con git.                                                                                                                                                       | Mantenibilidad |
| NFR-17 | Un nuevo proveedor debe aparecer en el asistente interactivo con solo registrar su descriptor, sin cambiar el código del asistente.                                                                                                                      | Extensibilidad |
| NFR-18 | Agregar una acción nativa de Playbook debe requerir solo implementar el trait`Action` y registrarla, sin modificar el motor.                                                                                                                           | Extensibilidad |
| NFR-19 | Los Playbooks deben usar una shell configurable por sistema operativo (`sh`/`bash`/`zsh` en Unix, `pwsh` o `cmd` en Windows).                                                                                                                   | Portabilidad   |
| NFR-20 | Las transiciones de la sesión deben validarse: una acción no permitida en el estado actual se rechaza indicando el estado y las acciones válidas.                                                                                                      | Fiabilidad     |
| NFR-21 | Si una operación externa falla durante una transición, la sesión debe registrar qué efectos se aplicaron y permitir reintentar solo los pendientes (`taskflow session retry`); nunca debe quedar un contador activo sin una sesión que lo refleje. | Fiabilidad     |
| NFR-22 | Un fallo al notificar o al ejecutar un gancho no debe interrumpir la sesión; se registra como advertencia.                                                                                                                                               | Robustez       |
| NFR-23 | `start` debe mostrar el reporte en menos de 30 segundos para una tarea típica, y en menos de 3 segundos si el análisis está en caché.                                                                                                               | Rendimiento    |

## 6. Casos de uso

Cada caso de uso es una pieza de la capa `application`. El CLI, y en el futuro la app de escritorio o web, solo los invocan.

### UC-01 `GetTodayTasks`

- **Actor:** usuario.
- **Entrada:** `TodayQuery { limit: Option<usize>, project: Option<ProjectSlug>, group_by: GroupBy }`. (El campo `include_in_review` de v0.2 se quitó en la fase 2: las tareas en revisión siempre se incluyen y reciben `in_review_bonus`.)
- **Flujo:** obtener usuario actual → obtener tareas asignadas → resolver el proyecto de cada tarea → filtrar por proyecto si aplica → obtener comentarios en paralelo → descartar tareas cerradas → aplicar política de prioridad → agrupar por proyecto si aplica.
- **Salida:** `TodayReport { groups: Vec<TaskGroup>, generated_at }`.
- **Errores:** credenciales inválidas, límite de peticiones, proveedor no disponible.

### UC-02 `PlanMyDay`

- **Entrada:** `PlanQuery { max_tasks: usize, project: Option<ProjectSlug> }`.
- **Flujo:** ejecutar UC-01 → construir el contexto de cada proyecto involucrado (uso `Plan`) → enviar las primeras `max_tasks` al `Summarizer` → construir plan agrupado por proyecto.
- **Salida:** `DailyPlan { summaries: Vec<TaskSummary>, plan: String, omitted_sections: Vec<SectionKey> }`.
- **Regla:** si la IA falla, devolver la lista priorizada sin resumen y avisar (degradación elegante).

### UC-03 `InspectTask`

- **Entrada:** `TaskId`.
- **Flujo:** obtener tarea con comentarios → resolver proyecto → construir contexto (usos `Summary` y `AgentPrompt`) → generar `TaskSummary` → construir `AgentPrompt`.
- **Salida:** `TaskInspection { task: TaskView, project: Option<ProjectView>, summary: TaskSummary, agent_prompt: String }`.

### UC-04 `CloseTask`

- **Entrada:** `CloseCommand { task_id, comment: Option<String> }`.
- **Precondición:** confirmación del usuario, obtenida por la interfaz, no por la capa de aplicación.
- **Flujo:** publicar comentario (si existe) → cambiar estado al estado "done" configurado.
- **Salida:** `CloseResult { task_id, final_status }`.

### UC-05 `UpdateSettings`

- **Entrada:** `SettingsChange` (`Set { key, value }`, `SwitchProvider { slot, kind, fields }`, `ReplaceAll(SettingsDraft)`).
- **Flujo:** cargar documento actual → aplicar el cambio sobre el documento editable → deserializar y validar el resultado completo → guardar secretos en `SecretStore` → escribir de forma atómica.
- **Salida:** `SettingsUpdated { path, changed_keys }`.
- **Regla:** las preguntas del asistente las hace la interfaz; este caso de uso solo recibe las respuestas y garantiza que el archivo resultante sea válido.

### UC-06 `ManageProjects`

- `ListProjects` → `Vec<ProjectView>`.
- `BrowseLocations` → árbol de ubicaciones del proveedor (`Vec<LocationNode>`), usado por el asistente de proyectos.
- `CreateProject { slug, name, kind, rules, repo_path }` → valida y crea `projects/<slug>/` con `project.toml` y plantillas de contexto.

### UC-07 `BuildContext`

- **Entrada:** `ContextRequest { project: Option<ProjectSlug>, usage: Usage, budget_chars: usize }`.
- **Flujo:** cargar secciones globales → cargar secciones del proyecto → fusionar por clave (el proyecto reemplaza a la global) → descartar privadas → filtrar por uso → aplicar presupuesto por prioridad.
- **Salida:** `BuiltContext { knowledge: Vec<ContextBlock>, instructions: Vec<ContextBlock>, omitted: Vec<SectionKey> }`.
- Lo usan UC-02, UC-03 y el paso `ai` de UC-08. También lo expone `taskflow context preview`.

### UC-08 `RunPlaybook`

Se divide en dos fases separadas por tipos (ver *typestate* en §11.4):

1. **`PlanPlaybook`** — entrada: `RunRequest { project, playbook, inputs: Vec<(String, String)> }`. Carga y valida el Playbook, resuelve entradas (pide las faltantes mediante el puerto `Interaction`) y produce `PlaybookRun<Planned>` con una vista previa. `--dry-run` termina aquí.
2. **`ExecutePlaybook`** — solo acepta `PlaybookRun<Approved>`. Ejecuta paso a paso, emite eventos de progreso, pide confirmaciones de los pasos marcados y produce `RunReport`.

### UC-09 `ManagePlaybooks`

- `ListPlaybooks { project }`, `ShowPlaybook`, `CheckPlaybook` (validación sin ejecución), `NewPlaybook { from_template }`.

### UC-10 `WorkSession`

Todas las acciones de la sesión pasan por un único servicio, `SessionService` (§11.5):

1. Cargar la sesión activa (`SessionStore`).
2. Pedir al dominio la decisión: `session.handle(command, now)` devuelve el nuevo estado y la lista de efectos, o un error de transición.
3. Ejecutar los efectos en orden a través de los puertos (`TimeTracker`, `TaskProvider`, `Summarizer`, `Notifier`, `PlaybookEngine`), anotando cada uno en el diario de la sesión.
4. Guardar la sesión de forma atómica; si llegó a un estado terminal, archivarla.

| Acción                        | Comando                                  | Entrada                                                                           |
| ------------------------------ | ---------------------------------------- | --------------------------------------------------------------------------------- |
| `StartWork`                  | `taskflow start`                       | `TaskId` o selección interactiva                                               |
| `ApproveWork`                | `taskflow approve`                     | Destino del prompt del agente                                                     |
| `RejectWork`                 | `taskflow reject`                      | Nota para el nuevo análisis                                                      |
| `PauseWork` / `ResumeWork` | `taskflow pause` / `taskflow resume` | —                                                                                |
| `PublishReport`              | `taskflow report publish`              | — (usa la versión de`report.md` en disco, con las ediciones del usuario)      |
| `ChangeStatus`               | `taskflow session status`              | Nombre del estado o selección interactiva                                        |
| `FinishWork`                 | `taskflow done`                        | Estado final elegido, comentario opcional; confirmación obtenida por la interfaz |
| `AbortWork`                  | `taskflow abort`                       | Restaurar o no el estado original                                                 |
| `RetryEffects`               | `taskflow session retry`               | —                                                                                |

Consulta auxiliar: `StatusOptions` devuelve los estados disponibles en la lista de la tarea, con las sugerencias del proyecto primero. La usan las interfaces para `approve`, `session status` y `done`.

- **Errores:** no hay sesión activa, acción no válida en el estado actual, contador ya activo en otra tarea, estado no disponible en la lista, reporte inexistente o ya publicado, fallos del proveedor o de la IA.
- **Regla:** si la IA falla, la sesión pasa igualmente a "esperando aprobación" con el reporte marcado como no disponible (degradación elegante); el usuario puede aprobar, reintentar o abortar.

## 7. Comandos del CLI

| Command                                                                           | Use case | Description                                                                         |
| --------------------------------------------------------------------------------- | -------- | ----------------------------------------------------------------------------------- |
| `taskflow today [--json] [--limit N] [--by-project] [--project SLUG]`           | UC-01    | Ranked list of assigned tasks, optionally grouped or filtered by project            |
| `taskflow plan [--max N] [--project SLUG] [--save]`                             | UC-02    | AI daily plan, optionally saved as Markdown                                         |
| `taskflow show <TASK_ID> [--copy] [--out FILE]`                                 | UC-03    | Task detail, AI summary and agent prompt                                            |
| `taskflow close <TASK_ID> [--comment TEXT] [--yes]`                             | UC-04    | Close task, with confirmation unless`--yes`                                       |
| `taskflow config init`                                                          | UC-05    | Interactive wizard: providers, credentials and defaults                             |
| `taskflow config provider [tasks\|ai]`                                           | UC-05    | Interactively switch a provider                                                     |
| `taskflow config show [SECTION]`                                                | —       | Print the current configuration (secrets masked)                                    |
| `taskflow config get <KEY>`                                                     | —       | Read one key, e.g.`ai.model`                                                      |
| `taskflow config set <KEY> <VALUE>`                                             | UC-05    | Update one key, preserving comments                                                 |
| `taskflow config edit`                                                          | UC-05    | Open in`$EDITOR`, validate on close                                               |
| `taskflow config check`                                                         | —       | Validate configuration and credentials                                              |
| `taskflow config path`                                                          | —       | Print the configuration directory                                                   |
| `taskflow project list`                                                         | UC-06    | List projects and their rules                                                       |
| `taskflow project add`                                                          | UC-06    | Interactive: pick spaces, folders or lists from the provider                        |
| `taskflow project show <SLUG>`                                                  | UC-06    | Project detail, context sections and playbooks                                      |
| `taskflow context list [--project SLUG]`                                        | UC-07    | List global and project sections                                                    |
| `taskflow context new <KEY> [--project SLUG] [--role knowledge\|instruction]`    | UC-07    | Create a section from a template and open it in the editor                          |
| `taskflow context edit <KEY> [--project SLUG]`                                  | UC-07    | Edit a section                                                                      |
| `taskflow context preview <TASK_ID> [--usage summary\|plan\|agent]`               | UC-07    | Show exactly what context the AI would receive                                      |
| `taskflow playbook list [--project SLUG]`                                       | UC-09    | List playbooks                                                                      |
| `taskflow playbook show <NAME> [--project SLUG]`                                | UC-09    | Show inputs and steps                                                               |
| `taskflow playbook check <NAME> [--project SLUG]`                               | UC-09    | Validate without running                                                            |
| `taskflow playbook new <NAME> [--project SLUG] [--from TEMPLATE]`               | UC-09    | Create from a template                                                              |
| `taskflow run <NAME> [--project SLUG] [--set KEY=VALUE]... [--dry-run] [--yes]` | UC-08    | Plan, confirm and execute a playbook                                                |
| `taskflow start [TASK_ID]`                                                      | UC-10    | Start a work session: timer, AI analysis, work report, "ready" status, notification |
| `taskflow approve [--status NAME] [--copy] [--out FILE]`                        | UC-10    | Green light: working status and agent prompt                                        |
| `taskflow reject --note TEXT`                                                   | UC-10    | Re-run the analysis with extra guidance                                             |
| `taskflow pause` / `taskflow resume`                                          | UC-10    | Stop / restart the timer                                                            |
| `taskflow session [--json]`                                                     | UC-10    | Current session: state, elapsed time, allowed actions                               |
| `taskflow session status [STATUS]`                                              | UC-10    | Change the task status now (interactive list if omitted)                            |
| `taskflow report show`                                                          | UC-10    | Show the work report draft                                                          |
| `taskflow report edit`                                                          | UC-10    | Edit the report in`$EDITOR` before publishing                                     |
| `taskflow report publish [--yes]`                                               | UC-10    | Explicitly post the report as a task comment                                        |
| `taskflow session retry`                                                        | UC-10    | Retry pending effects after a failure                                               |
| `taskflow done [--status NAME] [--comment TEXT] [--yes]`                        | UC-10    | You declare the task finished: final status, optional comment, stop timer, notify   |
| `taskflow abort [--keep-status]`                                                | UC-10    | Cancel the session and stop the timer                                               |

Opciones globales: `-v/--verbose` (logs detallados), `--config <PATH>`.

Si un comando necesita un proyecto y no se indica `--project`, se usa `projects.default` o, si el directorio actual está dentro del `repo_path` de un proyecto, ese proyecto.

## 8. Arquitectura

### 8.1 Estilo: arquitectura hexagonal organizada por capacidades

El sistema tiene un núcleo (dominio + aplicación) rodeado de adaptadores. Hay dos tipos de puertos:

- **Puertos de salida (driven):** lo que el núcleo necesita del exterior. Ejemplos: `TaskProvider`, `Summarizer`, `ConfigStore`, `CommandRunner`, `Interaction`. Los implementan los adaptadores de ClickUp, Anthropic, sistema de archivos, shell y la propia interfaz.
- **Puertos de entrada (driving):** cómo el exterior usa el núcleo. Son los casos de uso. Los invocan el CLI, una app de escritorio o una API web.

La regla de dependencias es estricta: **todas las flechas apuntan hacia adentro.** El dominio no conoce a nadie; la aplicación solo conoce al dominio; los adaptadores e interfaces conocen al núcleo, nunca al revés.

Dentro de `domain` y `application` el código se organiza **por capacidad** (tareas, proyectos, contexto, Playbooks, configuración) y no por tipo técnico. Cada capacidad es un módulo con su modelo, sus puertos y sus casos de uso, lo que permite entender o modificar una funcionalidad sin recorrer todo el crate.

### 8.2 Patrones de diseño y la herramienta de Rust que los implementa

| Patrón                                | Dónde                                                                 | Herramienta de Rust                                                        | Qué resuelve                                                                  |
| -------------------------------------- | ---------------------------------------------------------------------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| Puertos y adaptadores                  | Todos los puertos                                                      | Traits +`Arc<dyn Trait>`                                                 | Independencia de proveedor e interfaz                                          |
| Composition root                       | `bootstrap`                                                          | Funciones constructoras,`cfg(feature)`                                   | Un único lugar donde se conectan las piezas                                   |
| Registro de proveedores                | `bootstrap::registry`                                                | `static` de descriptores con punteros a función                         | El asistente lista proveedores sin conocerlos (NFR-17)                         |
| Newtype +*parse, don't validate*     | `TaskId`, `ProjectSlug`, `SectionKey`, `StepId`, `ConfigKey` | Campo privado +`FromStr`/`TryFrom`                                     | Un valor inválido no puede existir                                            |
| Spec → Modelo                         | Archivos TOML → dominio                                               | `serde` para la *spec*, `TryFrom` hacia el modelo                    | El formato de archivo no contamina el dominio                                  |
| Estrategia                             | `PrioritizationPolicy`, reglas de proyecto                           | Traits con métodos por defecto, enums                                     | Cambiar comportamiento por configuración                                      |
| Repositorio                            | Configuración, proyectos, contexto, Playbooks                         | Traits async en`application::ports`                                      | El núcleo no sabe que existen archivos                                        |
| Comando +*enum dispatch*             | Pasos de Playbook (`StepKind`)                                       | `enum` + `match` exhaustivo                                            | El compilador obliga a manejar cada tipo de paso                               |
| Extensión por trait object            | Paso`custom`                                                         | `trait Action` + `HashMap<&str, Arc<dyn Action>>`                      | Agregar acciones sin tocar el motor (NFR-18)                                   |
| Intérprete                            | `PlaybookEngine`                                                     | Bucle sobre pasos +`Scope` de variables                                  | Ejecutar Playbooks declarativos                                                |
| *Typestate*                          | `PlaybookRun<Planned>` → `PlaybookRun<Approved>`                  | Genéricos con tipos de estado                                             | Imposible ejecutar sin aprobación: lo verifica el compilador                  |
| Observador por canales                 | Progreso de Playbooks                                                  | `tokio::sync::mpsc` + `enum RunEvent`                                  | La aplicación informa sin imprimir                                            |
| Cancelación cooperativa               | Playbooks, comandos largos                                             | `CancellationToken` + `tokio::select!`                                 | Ctrl+C limpio (NFR-15)                                                         |
| Errores tipados por capa               | Todas las capas                                                        | `thiserror` + `#[from]`, `anyhow` en binarios                        | Errores precisos y mensajes útiles                                            |
| API estable                            | Enums públicos de errores y DTOs                                      | `#[non_exhaustive]`                                                      | Agregar variantes sin romper a quien las usa                                   |
| Máquina de estados con enums          | `SessionState` + `SessionCommand`                                  | `match (estado, comando)` exhaustivo, guardas `if`, patrones `A \| B` | Solo existen las transiciones permitidas (NFR-20)                              |
| Núcleo funcional, efectos en el borde | `WorkSession::handle` → `Vec<Effect>`                             | Enums con datos y funciones puras                                          | La lógica de la sesión se prueba sin red; los efectos se reintentan (NFR-21) |
| Decorador                              | `CachedTaskProvider<P, C>`                                           | Genéricos +`impl<P: TaskProvider, C: Cache> TaskProvider for ...`       | Caché transparente sin tocar adaptadores ni casos de uso                      |
| Segregación de interfaces             | `TimeTracker` separado de `TaskProvider`                           | Traits pequeños                                                           | Un proveedor sin contador (GitHub) no implementa métodos vacíos              |
| Objeto nulo                            | `LocalTimeTracker`                                                   | Otra implementación del mismo trait                                       | Sin casos especiales cuando el proveedor no mide tiempo                        |
| Composite                              | `MultiNotifier`                                                      | `Vec<Box<dyn Notifier>>` que también implementa `Notifier`            | Varios canales de notificación como si fueran uno                             |

### 8.3 Capas y crates

```text
 INTERFACES (driving)       ┌─────────┐   ┌──────────────┐   ┌──────────┐
                            │ cli     │   │ desktop      │   │ web      │
                            │ (v1)    │   │ (futuro,     │   │ (futuro, │
                            │ clap    │   │  Tauri)      │   │  axum)   │
                            └────┬────┘   └──────┬───────┘   └────┬─────┘
                                 │ implementa Interaction          │
                                 └───────────────┼────────────────┘
                                                 ▼
 COMPOSITION ROOT                  ┌──────────────────────────┐
                                   │ bootstrap                │
                                   │ registry · Config → Ctx  │
                                   └─────┬──────────────┬─────┘
                                         │              │
 ADAPTERS (driven)   ┌──────────────────▼──┐   ┌───────▼────────────┐   ┌────────────────────┐
                     │ adapter-clickup     │   │ adapter-anthropic  │   │ adapter-fs         │
                     │ adapter-github (+)  │   │ adapter-ollama (+) │   │ adapter-shell      │
                     └──────────┬──────────┘   └─────────┬──────────┘   │ adapter-keyring    │
                                │                        │              │ adapter-notify     │
                                │                        │              └─────────┬──────────┘
                                │ implementan puertos    │                        │
                                ▼                        ▼                        ▼
 CORE                ┌──────────────────────────────────────────────────────────────────┐
                     │ application                                                      │
                     │ ports (traits) · use cases · DTOs · PlaybookEngine · AppContext  │
                     ├──────────────────────────────────────────────────────────────────┤
                     │ domain                                                           │
                     │ task · project · context · playbook · session · policy           │
                     └──────────────────────────────────────────────────────────────────┘
```

| Crate                 | Tipo         | Responsabilidad                                                                                                                                                                                                 | Depende de                                                                                                                           |
| --------------------- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `domain`            | lib          | Entidades, value objects, política de prioridad, resolución de proyectos, selección de contexto, modelo validado de Playbooks, máquina de estados de la sesión de trabajo. Código puro, sin async ni I/O. | `chrono`, `thiserror`, `enumset`, `indexmap`, `serde` (solo derive)                                                        |
| `application`       | lib          | Puertos (traits), casos de uso, DTOs serializables,`AppContext`, motor de Playbooks, servicio de sesiones de trabajo, decorador de caché.                                                                    | `domain`, `async-trait`, `serde`, `serde_json`, `futures`, `tokio` (`sync`, `macros`), `tokio-util`, `minijinja` |
| `adapter-clickup`   | lib          | Implementa`TaskProvider` y `TimeTracker` para la API v2 de ClickUp.                                                                                                                                         | `application`, `reqwest`, `serde`                                                                                              |
| `adapter-anthropic` | lib          | Implementa`Summarizer` usando la API de Messages.                                                                                                                                                             | `application`, `reqwest`, `serde`                                                                                              |
| `adapter-fs`        | lib          | Implementa`ConfigStore`, `ProjectRepository`, `ContextRepository`, `PlaybookRepository`, `RunLog`, `SessionStore` y `Cache` sobre archivos.                                                       | `application`, `toml`, `toml_edit`, `tempfile`, `directories`                                                              |
| `adapter-shell`     | lib          | Implementa`CommandRunner` con `tokio::process`, tiempo máximo y cancelación.                                                                                                                              | `application`, `tokio` (`process`)                                                                                             |
| `adapter-keyring`   | lib          | Implementa`SecretStore` con el llavero del sistema y fallback a variables de entorno.                                                                                                                         | `application`, `keyring`                                                                                                         |
| `adapter-notify`    | lib          | Implementa`Notifier` para escritorio y terminal, y `MultiNotifier` para combinarlos.                                                                                                                        | `application`, `notify-rust`                                                                                                     |
| `bootstrap`         | lib          | Registro de proveedores, carga de configuración y armado del`AppContext`. Compartido por todas las interfaces.                                                                                               | `application`, todos los adaptadores                                                                                               |
| `cli`               | bin          | Parseo de argumentos, asistentes interactivos, implementación de`Interaction`, presentación en terminal.                                                                                                    | `bootstrap`, `application`, `clap`, `dialoguer`, `indicatif`, `comfy-table`, `arboard`                                 |
| `desktop`           | bin (futuro) | App Tauri; cada comando Tauri invoca un caso de uso.                                                                                                                                                            | `bootstrap`, `application`, `tauri`                                                                                            |
| `web`               | bin (futuro) | API HTTP; cada handler invoca un caso de uso.                                                                                                                                                                   | `bootstrap`, `application`, `axum`                                                                                             |

> **Por qué existe `bootstrap`:** si el armado de dependencias viviera en el CLI, la app de escritorio tendría que copiarlo. Al separarlo, cualquier interfaz nueva obtiene la lógica completa con una sola línea: `bootstrap::build(config).await?`.

> **Por qué `minijinja` puede vivir en `application`:** es una librería pura (sin I/O ni red), igual que `serde`. Las plantillas son parte de la lógica del motor de Playbooks, no un detalle de infraestructura.

### 8.4 Estructura de carpetas del código

```text
taskflow/
├── Cargo.toml                    # [workspace] + [workspace.dependencies] + [workspace.lints]
├── rust-toolchain.toml           # pinned stable toolchain
├── .env.example                  # documented env vars, no real values
├── docs/
│   ├── ARCHITECTURE.md           # este documento
│   └── adr/                      # Architecture Decision Records
├── crates/
│   ├── domain/src/
│   │   ├── lib.rs
│   │   ├── task/                 # mod.rs (Task, TaskId, Comment), status.rs, location.rs
│   │   ├── project/              # mod.rs (Project, ProjectSlug), rules.rs, resolver.rs
│   │   ├── context/              # mod.rs (ContextSection, SectionKey, Usage), selection.rs
│   │   ├── playbook/             # mod.rs (Playbook), step.rs, input.rs, validate.rs
│   │   ├── session/              # mod.rs (WorkSession), state.rs, command.rs, effect.rs, report.rs
│   │   ├── policy.rs             # PrioritizationPolicy, WeightedPolicy
│   │   └── error.rs              # DomainError, PlaybookIssue
│   ├── application/src/
│   │   ├── lib.rs
│   │   ├── ports/                # task_provider, summarizer, clock, cache, config_store,
│   │   │                         # secret_store, project_repository, context_repository,
│   │   │                         # playbook_repository, command_runner, interaction, run_log,
│   │   │                         # time_tracker, notifier, session_store
│   │   ├── tasks/                # get_today_tasks, plan_my_day, inspect_task, close_task
│   │   ├── settings/             # update_settings, settings (AppSettings), keys (ConfigKey)
│   │   ├── projects/             # list, browse_locations, create
│   │   ├── context/              # build_context
│   │   ├── playbooks/            # plan, run (typestate), engine, scope, events, actions
│   │   ├── session/              # service (dispatch + efectos), cached_provider, views
│   │   ├── dto.rs                # TaskView, TodayReport, TaskGroup, DailyPlan, ...
│   │   ├── context.rs            # AppContext
│   │   └── error.rs              # AppError
│   ├── adapters/
│   │   ├── clickup/src/          # client.rs, api_types.rs, mapper.rs, provider.rs, time_tracker.rs, descriptor.rs
│   │   ├── anthropic/src/        # client.rs, prompts.rs, summarizer.rs, descriptor.rs
│   │   ├── fs/src/               # config_store.rs, projects.rs, context.rs, playbooks.rs,
│   │   │                         # front_matter.rs, atomic.rs, run_log.rs, specs.rs, session_store.rs, cache.rs
│   │   ├── shell/src/            # runner.rs, shell_kind.rs
│   │   ├── keyring/src/          # secret_store.rs
│   │   └── notify/src/           # desktop.rs, terminal.rs, multi.rs
│   └── bootstrap/src/            # registry.rs, config.rs, build.rs
├── apps/
│   └── cli/src/                  # main.rs, args.rs, commands/, wizards/, interaction.rs, presenter/
├── templates/                    # plantillas de contexto y Playbooks (incluidas con include_str!)
└── tests/
    ├── fixtures/                 # JSON samples per provider, sample projects
    └── contract/                 # shared test suite every TaskProvider must pass
```

### 8.5 Estructura de datos del usuario

Separada del código. Ubicación con el crate `directories` (`~/.config/taskflow/` en Linux, `~/Library/Application Support/taskflow/` en macOS, `%APPDATA%\taskflow\` en Windows).

```text
<config_dir>/taskflow/
├── config.toml                       # configuración general (§13)
├── context/                          # secciones globales, aplican a todos los proyectos
│   └── 90-aprendizaje-rust.md
├── projects/
│   ├── acme/
│   │   ├── project.toml              # slug, nombre, reglas, repo_path
│   │   ├── context/
│   │   │   ├── 10-negocio.md
│   │   │   ├── 20-codigo.md
│   │   │   └── 30-despliegue.md
│   │   └── playbooks/
│   │       └── release.toml
│   └── interno/
│       └── project.toml
├── runs/                             # registros de ejecución de Playbooks (JSON Lines)
│   └── 2026-10-10T14-02-11_acme_release.jsonl
└── sessions/                         # sesiones de trabajo (§9.6)
    ├── active.json                   # sesión activa: estado, intervalos de tiempo, diario de efectos
    ├── 86b2x1abc/
    │   ├── report.md                 # reporte de trabajo generado por la IA
    │   └── agent-prompt.md
    └── history/                      # sesiones cerradas o abortadas
```

Todo es texto plano, por lo que el directorio completo (sin `runs/` ni `sessions/`) puede versionarse con git (NFR-16).

## 9. Modelo de dominio

El dominio representa tareas, proyectos, contexto y Playbooks de forma genérica. No contiene ningún concepto propio de ClickUp.

### 9.1 Tarea

```rust
// crates/domain/src/task/mod.rs
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(String);

impl TaskId {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let raw = raw.into();
        if raw.trim().is_empty() {
            return Err(DomainError::EmptyTaskId);
        }
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserId(String);

#[derive(Debug, Clone)]
pub struct Comment {
    pub author: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub assignee: Option<UserId>,
    pub resolved: bool,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub description: Option<String>,
    pub status: Status,
    pub priority: Priority,
    pub due: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub url: String,
    pub location: TaskLocation,
    pub tags: Vec<String>,
    pub comments: Vec<Comment>,
}

impl Task {
    pub fn pending_comments_for<'a>(
        &'a self,
        user: &'a UserId,
    ) -> impl Iterator<Item = &'a Comment> + 'a {
        self.comments
            .iter()
            .filter(move |c| !c.resolved && c.assignee.as_ref() == Some(user))
    }

    pub fn is_overdue(&self, now: DateTime<Utc>) -> bool {
        matches!(self.due, Some(due) if due < now)
    }
}
```

```rust
// crates/domain/src/task/location.rs
/// Nivel jerárquico genérico. ClickUp: Space > Folder > List. GitHub: Owner > Repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LocationLevel {
    Workspace,
    Space,
    Folder,
    List,
    Repository,
    Other(String),
}

impl LocationLevel {
    /// Mayor profundidad = más específico. Lo usa la resolución de proyectos.
    pub fn depth(&self) -> u8 {
        match self {
            Self::Workspace => 0,
            Self::Space => 1,
            Self::Folder => 2,
            Self::List | Self::Repository => 3,
            Self::Other(_) => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationSegment {
    pub level: LocationLevel,
    pub id: String,
    pub name: String,
}

/// Ruta desde la raíz hasta el contenedor de la tarea.
#[derive(Debug, Clone, Default)]
pub struct TaskLocation(Vec<LocationSegment>);

impl TaskLocation {
    pub fn segments(&self) -> &[LocationSegment] {
        &self.0
    }
}

/// Árbol de ubicaciones para el asistente de proyectos. `Vec` ya vive en el heap,
/// por eso el tipo recursivo no necesita `Box`.
#[derive(Debug, Clone)]
pub struct LocationNode {
    pub segment: LocationSegment,
    pub children: Vec<LocationNode>,
}
```

```rust
// crates/domain/src/task/status.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Todo,
    Ready, // "preparado": analizado y esperando luz verde
    InProgress,
    InReview,
    Done,
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Urgent,
    High,
    Normal,
    Low,
    None,
}
```

Decisiones:

- `TaskId::new` valida al construir y el campo interno es privado: no puede existir un `TaskId` vacío (*parse, don't validate*).
- `Status::Other(String)` conserva estados personalizados del proveedor en lugar de perderlos.
- `pending_comments_for` usa lifetimes explícitos porque el iterador devuelto toma prestados `self` y `user` a la vez.
- `TaskLocation` es una ruta genérica: el mismo modelo sirve para ClickUp y para GitHub.

### 9.2 Proyecto

```rust
// crates/domain/src/project/mod.rs
use std::{path::PathBuf, str::FromStr};

/// Identificador corto: minúsculas, dígitos y guiones, de 1 a 40 caracteres.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectSlug(String);

impl FromStr for ProjectSlug {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let valid = (1..=40).contains(&s.len())
            && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !s.starts_with('-');
        if valid {
            Ok(Self(s.to_owned()))
        } else {
            Err(DomainError::InvalidSlug(s.to_owned()))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    Client,
    Internal,
}

#[derive(Debug, Clone)]
pub enum MatchRule {
    Location { level: LocationLevel, id: String },
    Tag(String),
}

impl MatchRule {
    /// Devuelve la especificidad de la coincidencia, o `None` si no coincide.
    pub fn specificity(&self, task: &Task) -> Option<u8> {
        match self {
            Self::Location { level, id } => task
                .location
                .segments()
                .iter()
                .any(|s| &s.level == level && &s.id == id)
                .then(|| level.depth() + 1),
            Self::Tag(tag) => task.tags.iter().any(|t| t == tag).then_some(0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub slug: ProjectSlug,
    pub name: String,
    pub kind: ProjectKind,
    pub rules: Vec<MatchRule>,
    pub repo_path: Option<PathBuf>,
    pub priority_boost: i64,
}
```

```rust
// crates/domain/src/project/resolver.rs
use indexmap::IndexMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProjectKey {
    Project(ProjectSlug),
    Unassigned,
}

pub struct ProjectResolver {
    projects: Vec<Project>,
}

impl ProjectResolver {
    pub fn new(projects: Vec<Project>) -> Self {
        Self { projects }
    }

    /// La regla más específica gana (FR-27).
    pub fn resolve(&self, task: &Task) -> Option<&Project> {
        self.projects
            .iter()
            .filter_map(|p| {
                p.rules
                    .iter()
                    .filter_map(|r| r.specificity(task))
                    .max()
                    .map(|score| (score, p))
            })
            .max_by_key(|(score, _)| *score)
            .map(|(_, p)| p)
    }

    /// Agrupa tareas ya ordenadas. `IndexMap` conserva el orden de inserción, así que
    /// el primer grupo es el del proyecto con la tarea más prioritaria.
    pub fn group<'t>(&self, tasks: &'t [Task]) -> IndexMap<ProjectKey, Vec<&'t Task>> {
        elet mut groups: IndexMap<ProjectKey, Vec<&Task>> = IndexMap::new();
        for task in tasks {
            let key = self
                .resolve(task)
                .map_or(ProjectKey::Unassigned, |p| ProjectKey::Project(p.slug.clone()));
            groups.entry(key).or_default().push(task);
        }
        groups
    }
}
```

### 9.3 Contexto

```rust
// crates/domain/src/context/mod.rs
use enumset::{EnumSet, EnumSetType};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SectionKey(String); // slug, igual que ProjectSlug

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionRole {
    /// Describe el proyecto: negocio, código, despliegue...
    Knowledge,
    /// Indica a la IA cómo responder: aprendizaje, estilo, restricciones...
    Instruction,
}

/// `EnumSet` guarda un conjunto de variantes en un entero (bitset): barato de copiar y comparar.
#[derive(Debug, EnumSetType)]
pub enum Usage {
    Summary,
    Plan,
    AgentPrompt,
    Playbook,
}

#[derive(Debug, Clone)]
pub struct ContextSection {
    pub key: SectionKey,
    pub title: String,
    pub role: SectionRole,
    pub usage: EnumSet<Usage>,
    /// Menor número = más importante.
    pub priority: u8,
    pub private: bool,
    pub body: String,
}

#[derive(Debug, Default)]
pub struct ContextSelection<'a> {
    pub included: Vec<&'a ContextSection>,
    pub omitted: Vec<&'a SectionKey>,
}

pub struct ProjectContext {
    sections: Vec<ContextSection>,
}

impl ProjectContext {
    /// Fusiona globales y del proyecto: una sección del proyecto reemplaza a la global con la misma clave.
    pub fn merge(global: Vec<ContextSection>, project: Vec<ContextSection>) -> Self {
        let mut by_key: BTreeMap<SectionKey, ContextSection> =
            global.into_iter().map(|s| (s.key.clone(), s)).collect();
        by_key.extend(project.into_iter().map(|s| (s.key.clone(), s)));
        Self { sections: by_key.into_values().collect() }
    }

    /// Selección pura y determinista: excluye privadas, filtra por uso y respeta el presupuesto.
    pub fn select(&self, usage: Usage, budget_chars: usize) -> ContextSelection<'_> {
        let mut candidates: Vec<&ContextSection> = self
            .sections
            .iter()
            .filter(|s| !s.private && s.usage.contains(usage))
            .collect();
        candidates.sort_by_key(|s| (s.priority, &s.key));

        let mut used = 0;
        let mut selection = ContextSelection::default();
        for section in candidates {
            let len = section.body.chars().count();
            if used + len <= budget_chars {
                used += len;
                selection.included.push(section);
            } else {
                selection.omitted.push(&section.key);
            }
        }
        selection
    }
}
```

Decisiones:

- La selección vive en `domain` porque es una regla de negocio pura y se prueba sin archivos.
- `ContextSelection<'a>` toma prestadas las secciones en lugar de clonarlas: un buen ejercicio de lifetimes.
- Invariante verificable con *property testing*: la suma de `included` nunca supera `budget_chars`.

### 9.4 Playbook

```rust
// crates/domain/src/playbook/step.rs
use std::{collections::BTreeMap, time::Duration};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StepId(String);

/// Texto con variables `{{ ... }}`. Su sintaxis la valida `application` al cargar.
#[derive(Debug, Clone)]
pub struct Template(String);

/// Referencia a una colección producida por un paso anterior: `steps.<id>`.
#[derive(Debug, Clone)]
pub struct StepRef(pub StepId);

#[derive(Debug, Clone, Copy, Default)]
pub enum OnError {
    #[default]
    Stop,
    Continue,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum Capture {
    #[default]
    Text,
    Lines,
    Json,
    None,
}

#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub ids_from: Option<StepRef>,
    pub statuses: Vec<Status>,
    pub tags: Vec<String>,
    pub only_mine: bool,
}

/// Conjunto cerrado de pasos integrados: el `match` del motor es exhaustivo.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum StepKind {
    Shell { run: Template, cwd: Option<Template>, env: BTreeMap<String, Template>, capture: Capture },
    SelectTasks { filter: TaskFilter },
    ExtractTaskIds { from: Template, pattern: Option<String> },
    Comment { body: Template },
    SetStatus { status: Status },
    Ai { prompt: Template, sections: Vec<SectionKey> },
    Confirm { message: Template },
    Custom { action: String, args: BTreeMap<String, Template> },
}

#[derive(Debug, Clone)]
pub struct Step {
    pub id: StepId,
    pub title: Option<String>,
    pub kind: StepKind,
    pub when: Option<Template>,
    pub for_each: Option<StepRef>,
    pub confirm: bool,
    pub on_error: OnError,
    pub timeout: Option<Duration>,
}
```

```rust
// crates/domain/src/playbook/mod.rs
#[derive(Debug, Clone)]
pub struct InputSpec {
    pub name: String,
    pub prompt: String,
    pub required: bool,
    pub default: Option<Template>,
    pub secret: bool,
}

#[derive(Debug, Clone)]
pub struct Playbook {
    name: PlaybookName,
    description: String,
    inputs: Vec<InputSpec>,
    steps: Vec<Step>,
}

impl Playbook {
    /// Único constructor. Devuelve todos los problemas a la vez, no solo el primero (FR-45).
    pub fn new(
        name: PlaybookName,
        description: String,
        inputs: Vec<InputSpec>,
        steps: Vec<Step>,
    ) -> Result<Self, Vec<PlaybookIssue>> {
        let mut issues = Vec::new();
        let mut seen = HashSet::new();

        if steps.is_empty() {
            issues.push(PlaybookIssue::NoSteps);
        }
        for step in &steps {
            for target in step.referenced_steps() {
                if !seen.contains(target) {
                    issues.push(PlaybookIssue::UnknownOrLaterStep {
                        step: step.id.clone(),
                        target: target.clone(),
                    });
                }
            }
            if !seen.insert(&step.id) {
                issues.push(PlaybookIssue::DuplicateStep(step.id.clone()));
            }
        }

        if issues.is_empty() {
            Ok(Self { name, description, inputs, steps })
        } else {
            Err(issues)
        }
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
    }
}
```

Decisiones:

- **Enum dispatch para los pasos integrados:** el conjunto es conocido, el `match` es exhaustivo y el compilador avisa si un tipo de paso nuevo no se maneja en el motor, la vista previa o el registro.
- **`Custom` como puerta de extensión:** las acciones nativas (`trait Action`, §11.4) cubren lo que no cabe en pasos declarativos sin abrir el enum.
- **Campos privados en `Playbook`:** solo `Playbook::new` puede construirlo, así que todo `Playbook` en memoria ya está validado.
- **Una referencia solo puede apuntar a un paso anterior:** el Playbook es una secuencia, no un grafo, y se ejecuta de arriba abajo.

### 9.5 Política de prioridad

La política es lógica de negocio pura, por eso vive en `domain` y no en `application`.

```rust
// crates/domain/src/policy.rs
pub trait PrioritizationPolicy: Send + Sync {
    fn score(&self, task: &Task, ctx: &ScoringContext) -> i64;

    fn rank(&self, tasks: &mut [Task], ctx: &ScoringContext) {
        tasks.sort_by_cached_key(|t| std::cmp::Reverse(self.score(t, ctx)));
    }
}

pub struct ScoringContext<'a> {
    pub now: DateTime<Utc>,
    /// Zona horaria del usuario: "vence hoy" depende del día local, no del día UTC.
    pub offset: FixedOffset,
    pub me: &'a UserId,
    pub projects: &'a ProjectResolver,
}

pub struct WeightedPolicy {
    pub priority_weight: i64,
    pub overdue_bonus: i64,
    pub due_today_bonus: i64,
    pub due_this_week_bonus: i64,
    pub pending_comment_weight: i64,
    pub in_review_bonus: i64,
}
```

Fórmula de `WeightedPolicy`:

```text
score = priority_weight × priority_value        (Urgent=4, High=3, Normal=2, Low=1, None=0)
      + overdue_bonus        if due < now                     ┐
      + due_today_bonus      if due is today (día local)      ├ excluyentes: se aplica solo uno
      + due_this_week_bonus  if due within 7 days             ┘
      + pending_comment_weight × pending_comments_count
      + in_review_bonus      if status == InReview
      + project.priority_boost  if the task resolves to a project (FR-31)
```

Los pesos se leen de la configuración, de modo que la política se ajusta sin recompilar.

Precisiones de la implementación (fase 1):

- **Bonos de vencimiento excluyentes:** una tarea vencida no suma también "vence hoy" ni "esta semana". Internamente se modela con un enum privado `Urgency { Overdue, DueToday, DueThisWeek, Later, NoDueDate }`.
- **"Hoy" es el día local del usuario:** `ScoringContext.offset` evita que una tarea que vence a las 22:00 en Bogotá (03:00 UTC del día siguiente) se clasifique como "mañana".
- **Empates estables:** `rank` usa `sort_by_cached_key`, que es estable; las tareas con igual puntaje conservan el orden del proveedor.
- **Empate entre proyectos:** si dos proyectos coinciden con la misma especificidad, gana el definido primero.

### 9.6 Sesión de trabajo: máquina de estados con enums

```text
 start ─► Analyzing ──AnalysisReady / AnalysisFailed──► AwaitingApproval ──approve──► InProgress ──done──► Closed
              ▲                                               │                      │     ▲
              └──────────────────────reject───────────────────┘                 pause│     │resume
                                                                                     ▼     │
                                                                                    Paused ┘   (done también desde Paused)

 En cualquier estado no terminal:  report publish (explícito) · session status (cambio manual de estado)
 abort: desde cualquier estado no terminal ─► Aborted               Estados terminales: Closed, Aborted
```

#### Estados de la sesión y del reporte

```rust
// crates/domain/src/session/state.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SessionState {
    Analyzing { note: Option<String> },
    AwaitingApproval,
    InProgress,
    Paused { since: DateTime<Utc> },
    Closed,
    Aborted,
}

impl SessionState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Closed | Self::Aborted)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportOutcome {
    Ready(WorkReport),
    /// Degradación elegante: la IA falló, pero el usuario puede aprobar, reintentar o abortar.
    Unavailable { reason: String },
}

/// Un enum en lugar de `bool`: el estado "publicado" lleva consigo cuándo se publicó.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum Publication {
    #[default]
    Draft,
    Published { at: DateTime<Utc> },
}
```

#### Estados del proveedor: sugeridos o explícitos

```rust
// crates/domain/src/session/status.rs
/// Nombre de un estado del proveedor ("Code Review", "Ready For Testing"...).
/// No vacío; se compara sin distinguir mayúsculas, como hace ClickUp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusName(String);

/// Etapas que tienen un estado *sugerido* en la configuración.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    Ready,
    Working,
}

/// A qué estado llevar la tarea. Los nombres cambian por lista y con el tiempo,
/// así que el dominio no los fija: usa la sugerencia de una etapa o un nombre explícito.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StatusTarget {
    Suggested(Stage),
    Explicit(StatusName),
    Original,
}
```

No existe `Stage::Final`: el estado final siempre lo elige el usuario al ejecutar `done`, así que `Finish` exige un `StatusName`. El tipo hace imposible cerrar sin haber elegido un estado.

#### Comandos y efectos

```rust
// crates/domain/src/session/command.rs
#[derive(Debug, Clone)]
pub enum SessionCommand {
    AnalysisReady(WorkReport),
    AnalysisFailed { reason: String },
    Approve { status: Option<StatusName> },
    Reject { note: String },
    Pause,
    Resume,
    ChangeStatus { status: StatusName },
    PublishReport,
    Finish { status: StatusName, comment: Option<String> },
    Abort { restore_status: bool },
}
```

```rust
// crates/domain/src/session/effect.rs
/// Lo que el dominio pide hacer. No lo ejecuta: eso lo hace `application` mediante puertos.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Effect {
    StartTimer,
    StopTimer,
    Analyze { note: Option<String> },
    SaveReport,
    SetStatus(StatusTarget),
    PostComment(CommentKind), // Report | Closing(String)
    MarkReportPublished,
    Notify(NoticeKind),
    RunHook(HookPoint),
}

impl Effect {
    /// Si un efecto crítico falla, la transición se detiene y queda pendiente para `session retry`.
    /// Uno no crítico solo genera una advertencia (NFR-22).
    pub fn is_critical(&self) -> bool {
        !matches!(self, Self::Notify(_) | Self::RunHook(_))
    }
}
```

`MarkReportPublished` va siempre después de `PostComment(CommentKind::Report)`: como los efectos se ejecutan en orden y un fallo crítico detiene la cadena, el reporte solo se marca como publicado si el comentario realmente se creó.

#### La sesión y sus transiciones

```rust
// crates/domain/src/session/mod.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkSession {
    pub task_id: TaskId,
    pub project: Option<ProjectSlug>,
    pub original_status: Status,
    pub started_at: DateTime<Utc>,
    pub state: SessionState,
    pub report: Option<ReportOutcome>,
    pub publication: Publication,
    /// Tramos con el contador activo; el tiempo total se calcula a partir de ellos.
    pub intervals: Vec<Interval>,
    /// Efectos aplicados, pendientes o fallidos (NFR-21).
    pub journal: Journal,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("cannot `{command}` while the session is `{from}`")]
    InvalidTransition { from: &'static str, command: &'static str, allowed: Vec<&'static str> },
    #[error("there is no work report to publish")]
    NoReport,
    #[error("the work report was already published")]
    ReportAlreadyPublished,
}

impl WorkSession {
    pub fn start(task: &Task, project: Option<ProjectSlug>, now: DateTime<Utc>) -> (Self, Vec<Effect>) {
        let session = Self {
            task_id: task.id.clone(),
            project,
            original_status: task.status.clone(),
            started_at: now,
            state: SessionState::Analyzing { note: None },
            report: None,
            publication: Publication::Draft,
            intervals: Vec::new(),
            journal: Journal::default(),
        };
        let effects = vec![
            Effect::StartTimer,
            Effect::RunHook(HookPoint::OnStart),
            Effect::Analyze { note: None },
        ];
        (session, effects)
    }

    /// Sin I/O: decide el siguiente estado y los efectos. Solo modifica la sesión si el comando es válido.
    pub fn handle(&mut self, command: SessionCommand, now: DateTime<Utc>) -> Result<Vec<Effect>, SessionError> {
        use Effect::*;
        use SessionCommand as C;
        use SessionState as S;

        let (next, effects) = match (&self.state, command) {
            (S::Analyzing { .. }, C::AnalysisReady(report)) => {
                self.report = Some(ReportOutcome::Ready(report));
                self.publication = Publication::Draft;
                (
                    S::AwaitingApproval,
                    vec![SaveReport, SetStatus(StatusTarget::Suggested(Stage::Ready)), Notify(NoticeKind::ReadyForReview)],
                )
            }
            (S::Analyzing { .. }, C::AnalysisFailed { reason }) => {
                self.report = Some(ReportOutcome::Unavailable { reason });
                (S::AwaitingApproval, vec![Notify(NoticeKind::AnalysisFailed)])
            }
            (S::AwaitingApproval, C::Approve { status }) => {
                let target = status.map_or(StatusTarget::Suggested(Stage::Working), StatusTarget::Explicit);
                (S::InProgress, vec![SetStatus(target), RunHook(HookPoint::OnApproved)])
            }
            (S::AwaitingApproval, C::Reject { note }) => (
                S::Analyzing { note: Some(note.clone()) },
                vec![Analyze { note: Some(note) }],
            ),
            (S::InProgress, C::Pause) => (S::Paused { since: now }, vec![StopTimer]),
            (S::Paused { .. }, C::Resume) => (S::InProgress, vec![StartTimer]),
            (state, C::ChangeStatus { status }) if !state.is_terminal() => {
                (state.clone(), vec![SetStatus(StatusTarget::Explicit(status))])
            }
            (state, C::PublishReport) if !state.is_terminal() => match (&self.report, &self.publication) {
                (Some(ReportOutcome::Ready(_)), Publication::Draft) => {
                    (state.clone(), vec![PostComment(CommentKind::Report), MarkReportPublished])
                }
                (Some(ReportOutcome::Ready(_)), Publication::Published { .. }) => {
                    return Err(SessionError::ReportAlreadyPublished)
                }
                _ => return Err(SessionError::NoReport),
            },
            (S::InProgress | S::Paused { .. }, C::Finish { status, comment }) => {
                let mut effects = Vec::with_capacity(5);
                if let Some(text) = comment {
                    effects.push(PostComment(CommentKind::Closing(text)));
                }
                effects.extend([
                    SetStatus(StatusTarget::Explicit(status)),
                    StopTimer,
                    RunHook(HookPoint::OnDone),
                    Notify(NoticeKind::Closed),
                ]);
                (S::Closed, effects)
            }
            (state, C::Abort { restore_status }) if !state.is_terminal() => {
                let mut effects = vec![StopTimer];
                if restore_status {
                    effects.push(SetStatus(StatusTarget::Original));
                }
                (S::Aborted, effects)
            }
            (state, command) => {
                return Err(SessionError::InvalidTransition {
                    from: state.name(),
                    command: command.name(),
                    allowed: state.allowed_commands(),
                })
            }
        };

        self.state = next;
        Ok(effects)
    }

    pub fn elapsed(&self, now: DateTime<Utc>) -> Duration {
        self.intervals.iter().map(|i| i.duration(now)).sum()
    }
}
```

#### Reporte de trabajo

```rust
// crates/domain/src/session/report.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Complexity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkReport {
    pub what_is_requested: String,
    pub acceptance_criteria: Vec<String>,
    pub proposed_steps: Vec<String>,
    pub risks: Vec<String>,
    pub open_questions: Vec<String>,
    pub complexity: Complexity,
    pub agent_prompt: String,
}
```

Decisiones:

- **Un `match` sobre la tupla `(estado, comando)`** concentra todas las reglas del flujo en un solo lugar. Las guardas `if !state.is_terminal()` expresan "en cualquier estado activo", y el último brazo rechaza cualquier combinación no prevista con un error que lista las acciones válidas (NFR-20).
- **No existe un comando "trabajo completado" para herramientas externas.** El único camino a `Closed` es `Finish`, que envía la interfaz cuando el usuario ejecuta `done` (FR-61).
- **Publicar el reporte es un comando propio (`PublishReport`)**, nunca un efecto de otra transición. Ni `AnalysisReady` ni `Finish` lo publican (FR-71).
- **Los nombres de estado viven en los datos, no en los tipos:** `StatusTarget::Explicit(StatusName)` acepta cualquier estado que el proveedor tenga hoy; la validación contra la lista de la tarea ocurre en `application`, que sí puede consultar al proveedor (FR-70).
- **El dominio devuelve efectos en lugar de ejecutarlos** (*functional core, imperative shell*). `handle` recibe `&mut self`, pero no hace I/O: calcula el resultado y solo modifica la sesión si el comando es válido. Probarla no requiere red, reloj real ni ClickUp.
- **`Effect` se serializa**: el diario guarda qué efectos se aplicaron, así `session retry` continúa exactamente donde falló.
- **¿Typestate o enum?** Los Playbooks usan *typestate* (§11.4) porque viven dentro de una sola ejecución y el compilador puede seguir su estado. La sesión se guarda en disco y se recupera en otra ejecución (`start` hoy, `approve` en una hora), así que su estado solo se conoce en tiempo de ejecución. Por eso es un enum con transiciones validadas.

|                                | Typestate (`PlaybookRun<S>`) | Enum (`SessionState`)                  |
| ------------------------------ | ------------------------------ | ---------------------------------------- |
| ¿Cuándo se conoce el estado? | Al compilar                    | Al ejecutar (se lee de disco)            |
| Transición inválida          | No compila                     | `Err(SessionError::InvalidTransition)` |
| Persistencia                   | No necesaria                   | `serde`                                |
| Ideal para                     | Flujos dentro de un proceso    | Flujos largos entre ejecuciones          |

## 10. Puertos (traits)

### 10.1 Tareas e IA

```rust
// crates/application/src/ports/task_provider.rs
#[async_trait]
pub trait TaskProvider: Send + Sync {
    fn name(&self) -> &'static str;
    /// Regex que reconoce referencias a tareas en texto libre (commits, ramas, PRs).
    fn task_ref_pattern(&self) -> &str;
    async fn current_user(&self) -> Result<UserId, ProviderError>;
    async fn search_tasks(&self, query: &TaskQuery) -> Result<Vec<Task>, ProviderError>;
    async fn task(&self, id: &TaskId) -> Result<Task, ProviderError>;
    async fn comments(&self, id: &TaskId) -> Result<Vec<Comment>, ProviderError>;
    async fn locations(&self) -> Result<Vec<LocationNode>, ProviderError>;
    /// Estados válidos para la tarea según su lista; cambian por lista y con el tiempo.
    async fn available_statuses(&self, id: &TaskId) -> Result<Vec<StatusOption>, ProviderError>;
    async fn set_status(&self, id: &TaskId, status: &Status) -> Result<(), ProviderError>;
    async fn add_comment(&self, id: &TaskId, body: &str) -> Result<(), ProviderError>;
}

#[derive(Debug, Clone, Default)]
pub struct TaskQuery {
    pub assignee: Option<UserId>,
    pub ids: Vec<TaskId>,
    pub locations: Vec<(LocationLevel, String)>,
    pub statuses: Vec<Status>,
    pub tags: Vec<String>,
    pub include_closed: bool,
}
```

`assigned_tasks` (v0.2) se reemplaza por `search_tasks` con `assignee: Some(me)`. Los Playbooks necesitan consultar tareas del proyecto que no siempre son del usuario.

```rust
#[derive(Debug, Clone)]
pub struct StatusOption {
    pub name: StatusName,
    pub kind: StatusKind,
}

/// Clasificación que da el propio proveedor (en ClickUp, el campo `type` de cada estado).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Open,
    Custom,
    Done,
    Closed,
}
```

```rust
// crates/application/src/ports/summarizer.rs
#[async_trait]
pub trait Summarizer: Send + Sync {
    async fn summarize_task(&self, input: &SummaryInput) -> Result<TaskSummary, AiError>;
    async fn plan_day(&self, inputs: &[SummaryInput], context: &BuiltContext) -> Result<String, AiError>;
    async fn complete(&self, prompt: &str, context: &BuiltContext) -> Result<String, AiError>;
    async fn analyze_work(&self, input: &SummaryInput, note: Option<&str>) -> Result<WorkReport, AiError>;
}

pub struct SummaryInput {
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub recent_comments: Vec<String>,
    pub context: BuiltContext,
}
```

`complete` es una llamada genérica que usa el paso `ai` de los Playbooks. `analyze_work` produce el `WorkReport` de la sesión de trabajo (§9.6); `note` lleva las indicaciones de `taskflow reject`.

### 10.2 Tiempo y caché

```rust
// crates/application/src/ports/clock.rs
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
    /// Desfase horario del usuario; define qué es "hoy" para la política de prioridad.
    fn offset(&self) -> FixedOffset;
}
```

```rust
// crates/application/src/ports/cache.rs  (FR-17, FR-55, fase 12)
#[async_trait]
pub trait Cache: Send + Sync {
    async fn get_task(&self, id: &TaskId, max_age: Duration) -> Option<Task>;
    async fn put_task(&self, task: &Task);
    async fn get_summary(&self, id: &TaskId, updated_at: DateTime<Utc>) -> Option<TaskSummary>;
    async fn put_summary(&self, id: &TaskId, updated_at: DateTime<Utc>, summary: &TaskSummary);
    async fn get_report(&self, key: &ReportKey) -> Option<WorkReport>;
    async fn put_report(&self, key: &ReportKey, report: &WorkReport);
}

/// Un análisis sirve mientras no cambien la tarea ni el contexto enviado a la IA.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReportKey {
    pub task: TaskId,
    pub updated_at: DateTime<Utc>,
    pub context_hash: [u8; 32], // hash estable (blake3) del contexto seleccionado
}
```

#### Caché como decorador

```rust
// crates/application/src/session/cached_provider.rs
pub struct CachedTaskProvider<P, C> {
    inner: P,
    cache: C,
    max_age: Duration,
}

#[async_trait]
impl<P, C> TaskProvider for CachedTaskProvider<P, C>
where
    P: TaskProvider,
    C: Cache,
{
    async fn task(&self, id: &TaskId) -> Result<Task, ProviderError> {
        if let Some(task) = self.cache.get_task(id, self.max_age).await {
            return Ok(task);
        }
        let task = self.inner.task(id).await?;
        self.cache.put_task(&task).await;
        Ok(task)
    }

    // El resto de métodos delega en `self.inner`; las escrituras invalidan la entrada en caché.
}
```

`bootstrap` decide si envolver el adaptador: `Arc::new(CachedTaskProvider::new(clickup, fs_cache, ttl)) as Arc<dyn TaskProvider>`. Ni el adaptador de ClickUp ni los casos de uso saben que existe la caché.

```

### 10.3 Configuración y secretos

```rust
// crates/application/src/ports/config_store.rs
pub trait ConfigStore: Send + Sync {
    fn path(&self) -> &Path;
    fn load(&self) -> Result<AppSettings, ConfigError>;
    /// Aplica el cambio sobre el documento editable, valida el resultado completo
    /// y solo entonces escribe de forma atómica (FR-22, NFR-14).
    fn apply(&self, change: &SettingsChange) -> Result<Vec<ConfigKey>, ConfigError>;
    fn get_raw(&self, key: &ConfigKey) -> Result<Option<String>, ConfigError>;
}

pub trait SecretStore: Send + Sync {
    fn get(&self, name: &SecretName) -> Result<Option<SecretString>, SecretError>;
    fn set(&self, name: &SecretName, value: SecretString) -> Result<(), SecretError>;
}
```

`ConfigStore` es síncrono: lee y escribe un archivo pequeño una vez por ejecución, y lo necesita `bootstrap` antes de crear el runtime de los casos de uso.

### 10.4 Proyectos, contexto y Playbooks

```rust
#[async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<Project>, RepoError>;
    async fn get(&self, slug: &ProjectSlug) -> Result<Project, RepoError>;
    async fn create(&self, project: &Project) -> Result<(), RepoError>;
}

#[async_trait]
pub trait ContextRepository: Send + Sync {
    async fn global_sections(&self) -> Result<Vec<ContextSection>, RepoError>;
    async fn project_sections(&self, slug: &ProjectSlug) -> Result<Vec<ContextSection>, RepoError>;
    async fn create_section(&self, scope: SectionScope, draft: &SectionDraft) -> Result<PathBuf, RepoError>;
}

#[async_trait]
pub trait PlaybookRepository: Send + Sync {
    async fn list(&self, slug: &ProjectSlug) -> Result<Vec<PlaybookSummary>, RepoError>;
    /// Devuelve el Playbook ya validado o la lista completa de problemas.
    async fn load(&self, slug: &ProjectSlug, name: &PlaybookName) -> Result<Playbook, LoadError>;
}

#[async_trait]
pub trait RunLog: Send + Sync {
    async fn append(&self, run: &RunId, event: &RunEvent) -> Result<(), RepoError>;
}
```

### 10.5 Ejecución de comandos e interacción

```rust
// crates/application/src/ports/command_runner.rs
pub struct ShellCommand {
    pub line: String,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub timeout: Duration,
}

pub struct CommandOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}

#[async_trait]
pub trait CommandRunner: Send + Sync {
    /// Envía cada línea de salida por `lines` mientras el comando corre.
    async fn run(
        &self,
        cmd: &ShellCommand,
        lines: mpsc::Sender<OutputLine>,
        cancel: CancellationToken,
    ) -> Result<CommandOutput, RunnerError>;
}
```

```rust
// crates/application/src/ports/interaction.rs
#[async_trait]
pub trait Interaction: Send + Sync {
    async fn confirm(&self, request: &ConfirmRequest) -> Result<bool, InteractionError>;
    async fn input(&self, spec: &InputSpec) -> Result<String, InteractionError>;
}
```

### 10.6 Sesión de trabajo: tiempo, notificaciones y persistencia

```rust
// crates/application/src/ports/time_tracker.rs
#[async_trait]
pub trait TimeTracker: Send + Sync {
    async fn start(&self, task: &TaskId) -> Result<TimeEntryId, ProviderError>;
    /// Idempotente: si no hay contador activo devuelve `Ok(None)`.
    async fn stop(&self) -> Result<Option<TimeEntry>, ProviderError>;
    async fn current(&self) -> Result<Option<TimeEntry>, ProviderError>;
}
```

`TimeTracker` es un trait separado de `TaskProvider` (segregación de interfaces): GitHub Issues no tiene contador de tiempo, y en ese caso `bootstrap` usa `LocalTimeTracker`, que solo registra los intervalos en la sesión (objeto nulo).

```rust
// crates/application/src/ports/notifier.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoticeKind {
    ReadyForReview,
    AnalysisFailed,
    Closed,
    ActionFailed,
}

pub struct Notice {
    pub kind: NoticeKind,
    pub title: String,
    pub body: String,
    pub url: Option<String>,
}

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn notify(&self, notice: &Notice) -> Result<(), NotifyError>;
}

/// Composite: varios canales se comportan como uno solo.
pub struct MultiNotifier(Vec<Box<dyn Notifier>>);

#[async_trait]
impl Notifier for MultiNotifier {
    async fn notify(&self, notice: &Notice) -> Result<(), NotifyError> {
        let results = futures::future::join_all(self.0.iter().map(|n| n.notify(notice))).await;
        results.into_iter().find_map(Result::err).map_or(Ok(()), Err)
    }
}
```

```rust
// crates/application/src/ports/session_store.rs
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn active(&self) -> Result<Option<WorkSession>, RepoError>;
    async fn save(&self, session: &WorkSession) -> Result<(), RepoError>;
    async fn save_report(&self, task: &TaskId, report: &WorkReport) -> Result<PathBuf, RepoError>;
    /// Lee `report.md` tal como quedó en disco, incluidas las ediciones del usuario.
    async fn load_report_markdown(&self, task: &TaskId) -> Result<String, RepoError>;
    /// Recibe la sesión por valor: después de archivarla ya no se puede usar.
    async fn archive(&self, session: WorkSession) -> Result<(), RepoError>;
}
```

`archive` toma `WorkSession` por valor a propósito: el *ownership* expresa que la sesión termina su vida ahí, y el compilador impide seguir usándola después.

Detalles importantes:

- **`Clock` como puerto:** sin él, los tests que dependen de "hoy" fallarían según el día en que se ejecuten. En producción se usa `SystemClock`; en tests, `FixedClock`.
- **`SummaryInput` en lugar de `Task`:** el `Summarizer` recibe solo título, descripción, estado, comentarios recientes y el contexto seleccionado. Así se cumple NFR-05.
- **`Send + Sync`:** obligatorios porque los adaptadores se comparten entre tareas async y, en el futuro, entre peticiones concurrentes de una API web.
- **`Interaction` es un puerto que implementa la interfaz.** Refina la regla "la aplicación nunca pregunta al usuario": la aplicación nunca habla con la terminal directamente, pero puede *solicitar* una decisión a través de este puerto. El CLI lo implementa con `dialoguer`, Tauri con un diálogo y los tests con `ScriptedInteraction`.
- **`CommandRunner` como puerto:** el motor de Playbooks se prueba con `FakeCommandRunner` sin ejecutar procesos reales (NFR-09).

## 11. Capa de aplicación

### 11.1 `AppContext`

```rust
// crates/application/src/context.rs
use std::sync::Arc;

#[derive(Clone)]
pub struct AppContext {
    pub tasks: Arc<dyn TaskProvider>,
    pub ai: Arc<dyn Summarizer>,
    pub policy: Arc<dyn PrioritizationPolicy>,
    pub clock: Arc<dyn Clock>,
    pub projects: Arc<dyn ProjectRepository>,
    pub context: Arc<dyn ContextRepository>,
    pub playbooks: Arc<dyn PlaybookRepository>,
    pub runner: Arc<dyn CommandRunner>,
    pub run_log: Arc<dyn RunLog>,
    pub actions: Arc<ActionRegistry>,
    pub time: Arc<dyn TimeTracker>,
    pub notifier: Arc<dyn Notifier>,
    pub sessions: Arc<dyn SessionStore>,
    pub settings: AppSettings,
}
```

Se usa `Arc<dyn ...>` en lugar de `Box<dyn ...>` porque una app de escritorio o un servidor web necesitan compartir el contexto entre muchos hilos y peticiones. `Arc` permite clonarlo barato (solo incrementa un contador).

`Interaction` **no** está en `AppContext`: depende de la interfaz y de cada ejecución (por ejemplo, `--yes` usa `AssumeYesInteraction`), así que se pasa como argumento a los casos de uso que lo necesitan. `ConfigStore` y `SecretStore` tampoco: los usa `bootstrap` antes de que exista el contexto, y los casos de uso de configuración los reciben directamente.

En tests, `TestContext::default()` arma un `AppContext` con fakes de todos los puertos y permite reemplazar solo el que interesa.

### 11.2 Ejemplo de caso de uso

```rust
// crates/application/src/tasks/get_today_tasks.rs
use futures::future::try_join_all;

pub struct GetTodayTasks<'a> {
    ctx: &'a AppContext,
}

impl<'a> GetTodayTasks<'a> {
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    pub async fn execute(&self, query: TodayQuery) -> Result<TodayReport, AppError> {
        let me = self.ctx.tasks.current_user().await?;
        let projects = ProjectResolver::new(self.ctx.projects.list().await?);

        let mut tasks = self
            .ctx
            .tasks
            .search_tasks(&TaskQuery { assignee: Some(me.clone()), ..Default::default() })
            .await?;

        tasks.retain(|t| t.status != Status::Done);
        if let Some(slug) = &query.project {
            tasks.retain(|t| projects.resolve(t).is_some_and(|p| &p.slug == slug));
        }

        let comments = try_join_all(
            tasks.iter().map(|t| self.ctx.tasks.comments(&t.id)),
        )
        .await?;

        for (task, c) in tasks.iter_mut().zip(comments) {
            task.comments = c;
        }

        let now = self.ctx.clock.now();
        let scoring = ScoringContext { now, offset: self.ctx.clock.offset(), me: &me, projects: &projects };
        self.ctx.policy.rank(&mut tasks, &scoring);

        if let Some(limit) = query.limit {
            tasks.truncate(limit);
        }

        let groups = match query.group_by {
            GroupBy::None => vec![TaskGroup::flat(&tasks, &me, now)],
            GroupBy::Project => projects
                .group(&tasks)
                .into_iter()
                .map(|(key, group)| TaskGroup::for_project(&key, &projects, &group, &me, now))
                .collect(),
        };

        Ok(TodayReport { generated_at: now, groups })
    }
}
```

### 11.3 DTOs (lo que reciben las interfaces)

Las interfaces nunca reciben entidades del dominio, sino DTOs serializables. Eso permite que el mismo resultado se imprima en la terminal, se devuelva como JSON en una API web o se envíe al frontend de Tauri sin cambios.

```rust
// crates/application/src/dto.rs
#[derive(Debug, Clone, Serialize)]
pub struct TaskView {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub due: Option<DateTime<Utc>>,
    pub overdue: bool,
    pub pending_comments: usize,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectView {
    pub slug: String,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskGroup {
    /// `None` = "Sin proyecto" o lista sin agrupar.
    pub project: Option<ProjectView>,
    pub tasks: Vec<TaskView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TodayReport {
    pub generated_at: DateTime<Utc>,
    pub groups: Vec<TaskGroup>,
}
```

**Regla clave:** la capa de aplicación nunca imprime, nunca lee argumentos y nunca habla con la terminal. Cuando necesita una decisión del usuario, la pide a través del puerto `Interaction`.

### 11.4 Motor de Playbooks

#### Typestate: aprobar antes de ejecutar

FR-46 exige confirmación antes de ejecutar. En lugar de confiar en que cada interfaz recuerde preguntar, el tipo lo impone: `execute` solo existe para `PlaybookRun<Approved>`, y la única forma de obtenerlo es llamar a `approve`.

```rust
// crates/application/src/playbooks/run.rs
pub struct Planned;
pub struct Approved(Approval);

#[derive(Debug, Clone, Copy, Serialize)]
pub enum Approval {
    Interactive,
    AssumeYes, // --yes, queda registrado en el log de ejecución
}

pub struct PlaybookRun<S> {
    plan: RunPlan, // Playbook + proyecto + entradas resueltas + vista previa
    state: S,
}

impl PlaybookRun<Planned> {
    pub fn preview(&self) -> &RunPreview {
        &self.plan.preview
    }

    pub fn approve(self, approval: Approval) -> PlaybookRun<Approved> {
        PlaybookRun { plan: self.plan, state: Approved(approval) }
    }
}

impl PlaybookRun<Approved> {
    pub async fn execute(self, engine: &PlaybookEngine<'_>, io: RunIo<'_>) -> Result<RunReport, AppError> {
        engine.execute(self.plan, self.state.0, io).await
    }
}

pub struct RunIo<'a> {
    pub interaction: &'a dyn Interaction,
    pub events: mpsc::Sender<RunEvent>,
    pub cancel: CancellationToken,
}
```

`--dry-run` es simplemente: planificar, mostrar `preview()` y no llamar a `approve`. Las salidas de pasos aún no ejecutados aparecen como marcadores (`⟨steps.commits⟩`).

#### Bucle de ejecución

```rust
// crates/application/src/playbooks/engine.rs
impl PlaybookEngine<'_> {
    pub async fn execute(&self, plan: RunPlan, approval: Approval, io: RunIo<'_>) -> Result<RunReport, AppError> {
        let mut scope = Scope::new(&plan);
        let mut report = RunReport::start(&plan, approval);

        for step in plan.playbook.steps() {
            if io.cancel.is_cancelled() {
                report.cancelled();
                break;
            }
            if !scope.eval_when(step)? {
                report.skipped(&step.id);
                continue;
            }
            if step.confirm && !io.interaction.confirm(&scope.confirm_request(step)?).await? {
                report.declined(&step.id);
                break;
            }

            self.emit(&io, RunEvent::StepStarted { id: step.id.clone() }).await;
            let result = tokio::select! {
                r = self.run_step(step, &mut scope, &io) => r,
                _ = io.cancel.cancelled() => Err(StepError::Cancelled),
            };

            match (result, step.on_error) {
                (Ok(output), _) => {
                    self.emit(&io, RunEvent::StepSucceeded { id: step.id.clone() }).await;
                    scope.record(&step.id, output);
                }
                (Err(e), OnError::Continue) => report.failed_and_continued(&step.id, e),
                (Err(e), OnError::Stop) => {
                    report.failed(&step.id, e);
                    break;
                }
            }
        }

        Ok(report.finish())
    }

    async fn run_step(&self, step: &Step, scope: &mut Scope, io: &RunIo<'_>) -> Result<StepOutput, StepError> {
        match &step.kind {
            StepKind::Shell { run, cwd, env, capture } => {
                let cmd = scope.shell_command(run, cwd.as_ref(), env, step.timeout)?;
                let output = self.ctx.runner.run(&cmd, self.line_forwarder(io, &step.id), io.cancel.clone()).await?;
                StepOutput::from_command(output, *capture)
            }
            StepKind::SelectTasks { filter } => {
                let query = scope.task_query(filter)?;
                Ok(StepOutput::Tasks(self.ctx.tasks.search_tasks(&query).await?))
            }
            StepKind::ExtractTaskIds { from, pattern } => {
                let pattern = pattern.as_deref().unwrap_or(self.ctx.tasks.task_ref_pattern());
                Ok(StepOutput::Ids(extract_ids(&scope.render(from)?, pattern)?))
            }
            StepKind::Comment { body } => self.for_each_task(step, scope, |task, text| async move {
                self.ctx.tasks.add_comment(&task.id, &text).await
            }, body).await,
            StepKind::SetStatus { status } => self.for_each_task_status(step, scope, status).await,
            StepKind::Ai { prompt, sections } => {
                let context = self.build_context(scope.project(), Usage::Playbook, sections).await?;
                Ok(StepOutput::Text(self.ctx.ai.complete(&scope.render(prompt)?, &context).await?))
            }
            StepKind::Confirm { message } => {
                let approved = io.interaction.confirm(&ConfirmRequest::new(scope.render(message)?)).await?;
                if approved { Ok(StepOutput::Unit) } else { Err(StepError::Declined) }
            }
            StepKind::Custom { action, args } => {
                let action = self.ctx.actions.get(action).ok_or(StepError::UnknownAction(action.clone()))?;
                action.run(&scope.render_args(args)?, scope, io).await
            }
        }
    }
}
```

#### Salidas y variables

```rust
#[derive(Debug, Clone)]
pub enum StepOutput {
    Text(String),
    Lines(Vec<String>),
    Json(serde_json::Value),
    Tasks(Vec<Task>),
    Ids(Vec<TaskId>),
    Unit,
}
```

`Scope` expone a las plantillas (`minijinja`):

| Variable                                                   | Contenido                                                                        |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `inputs.<name>`                                          | Entradas del Playbook                                                            |
| `project.slug`, `project.name`, `project.repo_path`  | Datos del proyecto                                                               |
| `steps.<id>.text`, `.lines`, `.json`, `.exit_code` | Salida de un paso anterior                                                       |
| `steps.<id>.tasks`, `.ids`                             | Colecciones para`for_each` o `select_tasks`                                  |
| `item`                                                   | Elemento actual dentro de`for_each` (por ejemplo `item.title`, `item.url`) |
| `now`                                                    | Fecha y hora de ejecución                                                       |

#### Eventos de progreso

```rust
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub enum RunEvent {
    StepStarted { id: StepId },
    Output { id: StepId, line: String, stream: Stream },
    StepSucceeded { id: StepId },
    StepFailed { id: StepId, error: String },
    StepSkipped { id: StepId },
    Finished { summary: RunSummary },
}
```

El CLI consume el canal y dibuja con `indicatif`; Tauri los reenvía al frontend con `emit`; `RunLog` los guarda como JSON Lines (FR-49). La aplicación no sabe quién escucha.

#### Acciones nativas (FR-51)

```rust
// crates/application/src/playbooks/actions.rs
#[async_trait]
pub trait Action: Send + Sync {
    fn name(&self) -> &'static str;
    fn validate(&self, args: &BTreeMap<String, Template>) -> Result<(), String>;
    async fn run(&self, args: &BTreeMap<String, String>, scope: &Scope, io: &RunIo<'_>) -> Result<StepOutput, StepError>;
}

#[derive(Default)]
pub struct ActionRegistry(HashMap<&'static str, Arc<dyn Action>>);

impl ActionRegistry {
    pub fn register(&mut self, action: impl Action + 'static) -> &mut Self {
        self.0.insert(action.name(), Arc::new(action));
        self
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Action>> {
        self.0.get(name)
    }
}
```

Es la "alternativa en código" para procesos que no se expresan bien como pasos declarativos: se escribe en Rust, se registra en `bootstrap` y se usa desde cualquier Playbook con `kind = "custom"`.

### 11.5 Servicio de sesiones de trabajo

```rust
// crates/application/src/session/service.rs
pub struct SessionService<'a> {
    ctx: &'a AppContext,
}

impl SessionService<'_> {
    pub async fn start(&self, task_id: TaskId, on_conflict: Conflict) -> Result<SessionView, AppError> {
        if let Some(active) = self.ctx.sessions.active().await? {
            self.resolve_conflict(active, on_conflict).await?; // pausar o terminar la actual (FR-66)
        }
        let task = self.ctx.tasks.task(&task_id).await?;
        let project = self.resolve_project(&task).await?;
        let (session, effects) = WorkSession::start(&task, project, self.ctx.clock.now());
        self.apply(session, effects).await
    }

    pub async fn dispatch(&self, command: SessionCommand) -> Result<SessionView, AppError> {
        let mut session = self.ctx.sessions.active().await?.ok_or(AppError::NoActiveSession)?;
        let effects = session.handle(command, self.ctx.clock.now())?;
        self.apply(session, effects).await
    }

    /// Estados disponibles para la tarea activa, con las sugerencias primero (FR-62, FR-70).
    pub async fn status_options(&self, stage: Option<Stage>) -> Result<Vec<StatusChoice>, AppError> {
        let session = self.ctx.sessions.active().await?.ok_or(AppError::NoActiveSession)?;
        let available = self.ctx.tasks.available_statuses(&session.task_id).await?;
        let suggested = self.ctx.settings.session.suggestions(session.project.as_ref(), stage);
        Ok(StatusChoice::rank(available, &suggested))
    }

    async fn apply(&self, mut session: WorkSession, effects: Vec<Effect>) -> Result<SessionView, AppError> {
        session.journal.enqueue(effects);
        self.ctx.sessions.save(&session).await?; // primero se persiste la intención

        while let Some(effect) = session.journal.next_pending().cloned() {
            match self.run_effect(&effect, &mut session).await {
                Ok(follow_up) => {
                    session.journal.mark_done(&effect);
                    // `Analyze` devuelve AnalysisReady o AnalysisFailed: una nueva transición.
                    if let Some(command) = follow_up {
                        let effects = session.handle(command, self.ctx.clock.now())?;
                        session.journal.enqueue(effects);
                    }
                    self.ctx.sessions.save(&session).await?;
                }
                Err(error) if effect.is_critical() => {
                    session.journal.mark_failed(&effect, &error);
                    self.ctx.sessions.save(&session).await?;
                    return Err(error); // `taskflow session retry` continúa desde aquí
                }
                Err(error) => {
                    tracing::warn!(?effect, %error, "non-critical effect failed");
                    session.journal.mark_skipped(&effect);
                }
            }
        }

        let view = SessionView::from_session(&session, self.ctx.clock.now());
        if session.state.is_terminal() {
            self.ctx.sessions.archive(session).await?; // `session` se mueve aquí
        } else {
            self.ctx.sessions.save(&session).await?;
        }
        Ok(view)
    }

    async fn run_effect(&self, effect: &Effect, session: &mut WorkSession) -> Result<Option<SessionCommand>, AppError> {
        let now = self.ctx.clock.now();
        match effect {
            Effect::StartTimer => {
                let entry = self.ctx.time.start(&session.task_id).await?;
                session.open_interval(entry, now);
            }
            Effect::StopTimer => {
                self.ctx.time.stop().await?;
                session.close_interval(now);
            }
            Effect::Analyze { note } => return Ok(Some(self.analyze(session, note.as_deref()).await)),
            Effect::SaveReport => {
                if let Some(ReportOutcome::Ready(report)) = &session.report {
                    self.ctx.sessions.save_report(&session.task_id, report).await?;
                }
            }
            Effect::SetStatus(target) => match self.resolve_status(target, session).await? {
                Some(status) => self.ctx.tasks.set_status(&session.task_id, &status).await?,
                None => tracing::warn!(?target, "suggested status not available in this list; status unchanged"),
            },
            Effect::PostComment(CommentKind::Report) => {
                let body = self.ctx.sessions.load_report_markdown(&session.task_id).await?;
                self.ctx.tasks.add_comment(&session.task_id, &body).await?;
            }
            Effect::PostComment(CommentKind::Closing(text)) => {
                self.ctx.tasks.add_comment(&session.task_id, text).await?;
            }
            Effect::MarkReportPublished => session.publication = Publication::Published { at: now },
            Effect::Notify(kind) => self.ctx.notifier.notify(&Notice::for_session(*kind, session)).await?,
            Effect::RunHook(point) => self.run_hook(*point, session).await?,
        }
        Ok(None)
    }

    /// Traduce el destino a un estado real de la lista de la tarea, consultado en este momento.
    /// - `Suggested`: primera sugerencia (proyecto, luego global) que exista; si ninguna existe, `None`.
    /// - `Explicit`: debe existir; si no, error con las opciones válidas (FR-70).
    /// - `Original`: el estado que tenía la tarea al iniciar la sesión.
    async fn resolve_status(&self, target: &StatusTarget, session: &WorkSession) -> Result<Option<Status>, AppError> {
        let available = self.ctx.tasks.available_statuses(&session.task_id).await?;
        match target {
            StatusTarget::Suggested(stage) => Ok(self
                .ctx
                .settings
                .session
                .suggestions(session.project.as_ref(), Some(*stage))
                .iter()
                .find_map(|name| available.iter().find(|o| o.name.matches(name)))
                .map(|o| self.ctx.settings.status_map.to_domain(&o.name))),
            StatusTarget::Explicit(name) => available
                .iter()
                .find(|o| o.name.matches(name))
                .map(|o| Some(self.ctx.settings.status_map.to_domain(&o.name)))
                .ok_or_else(|| AppError::StatusNotAvailable { requested: name.clone(), available }),
            StatusTarget::Original => Ok(Some(session.original_status.clone())),
        }
    }

    /// Nunca falla: si la IA no responde, devuelve `AnalysisFailed` (degradación elegante).
    async fn analyze(&self, session: &WorkSession, note: Option<&str>) -> SessionCommand {
        match self.build_report(session, note).await {
            Ok(report) => SessionCommand::AnalysisReady(report),
            Err(error) => SessionCommand::AnalysisFailed { reason: error.to_string() },
        }
    }
}
```

Puntos clave:

- **El `match` sobre `Effect` es exhaustivo**, incluso con patrones anidados (`PostComment(CommentKind::Report)`): si se agrega un efecto o un tipo de comentario, el servicio no compila hasta manejarlo.
- **Los estados se consultan al proveedor en el momento de la transición**, no se guardan en caché: si alguien cambia los estados de la lista en ClickUp, Taskflow se adapta sin tocar la configuración.
- **Una sugerencia que no existe en la lista no rompe el flujo**: se avisa y la tarea conserva su estado. Un estado explícito que no existe sí es un error, porque el usuario pidió algo concreto.
- **`build_report`** busca primero en caché con `ReportKey` (FR-55); si no hay, construye el contexto (UC-07, usos `Summary` y `AgentPrompt`) y llama a `Summarizer::analyze_work`.
- **La sesión se guarda después de cada efecto:** si el proceso muere a mitad de camino, al volver `taskflow session` muestra exactamente qué quedó pendiente.
- **El cierre solo lo inicia el usuario:** `done` es la única vía hacia `Closed` y exige confirmación de la interfaz (FR-61, FR-62).

## 12. Interfaces (cómo se reutiliza la lógica)

### 12.1 CLI (v1)

```rust
// apps/cli/src/main.rs
#[derive(Parser)]
#[command(name = "taskflow", version)]
struct Cli {
    #[arg(short, long, global = true)]
    verbose: bool,

    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Today {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        by_project: bool,
        #[arg(long)]
        project: Option<ProjectSlug>,
    },
    Plan {
        #[arg(long, default_value_t = 10)]
        max: usize,
        #[arg(long)]
        project: Option<ProjectSlug>,
        #[arg(long)]
        save: bool,
    },
    Show {
        task_id: String,
        #[arg(long)]
        copy: bool,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    Close {
        task_id: String,
        #[arg(long)]
        comment: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    Run {
        name: String,
        #[arg(long)]
        project: Option<ProjectSlug>,
        #[arg(long = "set", value_parser = parse_key_value)]
        inputs: Vec<(String, String)>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
    Start {
        task_id: Option<String>,
    },
    Approve {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        copy: bool,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    Reject {
        #[arg(long)]
        note: String,
    },
    Pause,
    Resume,
    Done {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        comment: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    Abort {
        #[arg(long)]
        keep_status: bool,
    },
    #[command(subcommand)]
    Session(SessionArgs),
    #[command(subcommand)]
    Report(ReportArgs),
    #[command(subcommand)]
    Config(ConfigCommand),
    #[command(subcommand)]
    Project(ProjectCommand),
    #[command(subcommand)]
    Context(ContextCommand),
    #[command(subcommand)]
    Playbook(PlaybookCommand),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // `config init` debe funcionar aunque todavía no exista configuración.
    if let Command::Config(ConfigCommand::Init) = &cli.command {
        return wizards::config_init::run(cli.config.as_deref()).await;
    }

    let config = bootstrap::load_config(cli.config.as_deref())?;
    let ctx = bootstrap::build(config).await?;

    match cli.command {
        Command::Today { json, limit, by_project, project } => {
            let group_by = if by_project { GroupBy::Project } else { ctx.settings.ui.group_by };
            let report = GetTodayTasks::new(&ctx)
                .execute(TodayQuery { limit, project, group_by, ..Default::default() })
                .await?;
            if json {
                presenter::json(&report)?;
            } else {
                presenter::today_table(&report);
            }
        }
        Command::Run { name, project, inputs, dry_run, yes } => {
            commands::run::execute(&ctx, name, project, inputs, dry_run, yes).await?;
        }
        // Plan, Show, Close, Config, Project, Context, Playbook ...
        _ => todo!(),
    }

    Ok(())
}
```

`ProjectSlug` implementa `FromStr`, así que `clap` lo valida al parsear los argumentos: un slug inválido nunca llega a la aplicación.

#### Asistente interactivo de configuración

```rust
// apps/cli/src/wizards/config_init.rs
pub async fn run(path: Option<&Path>) -> anyhow::Result<()> {
    let store = bootstrap::config_store(path)?;
    let secrets = bootstrap::secret_store();

    let task_providers = bootstrap::registry::task_providers();
    let choice = Select::new()
        .with_prompt("Proveedor de tareas")
        .items(&task_providers.iter().map(|p| p.display_name).collect::<Vec<_>>())
        .default(0)
        .interact()?;
    let provider = &task_providers[choice];

    let mut draft = SettingsDraft::default();
    for field in provider.fields {
        let value = ask_field(field, &draft).await?; // texto, número, secreto u opción descubierta
        draft.set_provider_field(provider.kind, field, value);
    }
    // ... mismo flujo para el proveedor de IA, luego política con valores por defecto

    let changed = UpdateSettings::new(&*store, &*secrets).apply(SettingsChange::ReplaceAll(draft))?;
    println!("Configuración guardada en {} ({} claves)", store.path().display(), changed.len());
    Ok(())
}
```

#### Implementación de `Interaction`

```rust
// apps/cli/src/interaction.rs
pub struct TerminalInteraction;

#[async_trait]
impl Interaction for TerminalInteraction {
    async fn confirm(&self, request: &ConfirmRequest) -> Result<bool, InteractionError> {
        let prompt = request.message.clone();
        // dialoguer bloquea el hilo: se ejecuta fuera del runtime async.
        tokio::task::spawn_blocking(move || {
            dialoguer::Confirm::new().with_prompt(prompt).default(false).interact()
        })
        .await
        .map_err(|_| InteractionError::Aborted)?
        .map_err(|_| InteractionError::Aborted)
    }

    async fn input(&self, spec: &InputSpec) -> Result<String, InteractionError> {
        // Input o Password de dialoguer según `spec.secret`
        todo!()
    }
}

/// Usada con `--yes`: aprueba todo y no pregunta nada.
pub struct AssumeYesInteraction;
```

#### Ejemplos de salida

```text
$ taskflow today --by-project

ACME Corp (cliente)
  #  ID          Title                              Status       Priority  Due          Comments
  1  86b2x1abc   Fix invoice rounding               in review    urgent    hoy          2
  2  86b2x1abd   Add export to CSV                  in progress  high      mañana       0

Interno
  3  86b2x2aaa   Upgrade CI runners                 to do        normal    —            1

Sin proyecto
  4  86b2x9zzz   Revisar propuesta de arquitectura  to do        low       —            0
```

```text
$ taskflow run release --project acme --set version=v2.4.0

Playbook release · ACME Corp · ~/code/acme-backend
Entradas: version=v2.4.0, previous=v2.3.2 (por defecto)

Plan (9 pasos)
  1  commits   shell     git log --oneline v2.3.2..v2.4.0
  2  prs       shell     gh pr list --state merged --base main --limit 30 --json number,title,headRefName
  3  review    ai        secciones: despliegue, codigo
  4  gate      confirm   ¿Revisaste el análisis y quieres desplegar v2.4.0?
  5  deploy    shell     make deploy VERSION=v2.4.0                       [confirmación]
  6  task_ids  extract   de ⟨steps.commits⟩ y ⟨steps.prs⟩
  7  tasks     select    ids ← ⟨steps.task_ids⟩, estado: in_review
  8  comment   comment   × ⟨steps.tasks⟩
  9  close     status    × ⟨steps.tasks⟩ → done

¿Ejecutar el plan? [y/N]
```

```text
$ taskflow start 86b2x1abc

▶ Sesión iniciada · Fix invoice rounding · ACME Corp
  ⏱  Contador de ClickUp activo
  ↻  Tarea obtenida de la API · contexto: negocio, codigo, aprendizaje-rust
  ✦  Analizando con IA...

Reporte de trabajo
  Qué se solicita   Corregir el redondeo de impuestos en facturas con más de 20 ítems.
  Criterios         · El total coincide con el cálculo oficial al centavo
                    · Test de regresión con la factura adjunta al ticket
  Pasos             1. Reproducir con el JSON adjunto
                    2. Revisar `tax::round_line` y dónde se acumula el total
                    3. Redondear por línea y agregar el test
  Riesgos           Cambiar el redondeo afecta el recálculo de facturas ya emitidas
  Preguntas         ¿Se redondea por línea o sobre el total? (comentario pendiente del cliente)
  Complejidad       media

  Estado en ClickUp: to do → preparado
  🔔 Notificación enviada · reporte (borrador, no publicado) en ~/.config/taskflow/sessions/86b2x1abc/report.md

Siguiente: `taskflow approve` para dar luz verde · `taskflow reject --note "..."` para re-analizar
```

```text
$ taskflow session

Fix invoice rounding · ACME Corp
Estado: en progreso · tiempo: 1 h 42 min (2 tramos)
Reporte: borrador (no publicado)
Acciones posibles: pause, session status, report edit, report publish, done, abort
```

```text
$ taskflow done

Estado final para "Fix invoice rounding" (lista ACME – Sprint):
  > Code Review            (sugerido para ACME)
    Ready For Testing      (sugerido)
    in progress
    complete
El reporte de trabajo no se ha publicado. ¿Publicarlo como comentario antes de cerrar? [y/N] n
Comentario de cierre (opcional): Corregido el redondeo por línea; incluye test de regresión.

Resumen: estado in progress → Code Review · comentario de cierre · contador detenido · 2 h 05 min
¿Confirmar cierre? [y/N] y
✔ Sesión cerrada · 🔔 notificación enviada
```

La interfaz arma la conversación (`status_options`, pregunta por el reporte, comentario, confirmación) y al final envía los comandos al servicio: `PublishReport` solo si el usuario respondió que sí, y luego `Finish { status, comment }`.

### 12.2 Escritorio (futuro, Tauri)

Tauri permite una app de escritorio con backend en Rust y frontend web. El backend reutiliza todo sin cambios:

```rust
// apps/desktop/src/main.rs (future)
#[tauri::command]
async fn today(ctx: tauri::State<'_, AppContext>) -> Result<TodayReport, String> {
    GetTodayTasks::new(&ctx)
        .execute(TodayQuery::default())
        .await
        .map_err(|e| e.to_string())
}
```

Para Playbooks, el escritorio implementa `Interaction` con diálogos nativos y reenvía cada `RunEvent` al frontend.

### 12.3 Web (futuro, axum)

```rust
// apps/web/src/main.rs (future)
async fn today(State(ctx): State<AppContext>) -> Result<Json<TodayReport>, ApiError> {
    Ok(Json(GetTodayTasks::new(&ctx).execute(TodayQuery::default()).await?))
}
```

Las tres interfaces llaman a `GetTodayTasks::execute` y reciben el mismo `TodayReport`. Solo cambia cómo se presenta.

## 13. Configuración persistente

### 13.1 Archivo `config.toml`

```toml
version = 1                        # versión del esquema (FR-25)

[provider]
kind = "clickup"

[provider.clickup]
team_id = "123456"

[provider.status_map]
todo        = ["to do", "open"]
ready       = ["preparado"]
in_progress = ["in progress"]
in_review   = ["review", "qa"]
done        = ["complete", "closed"]
done_target = "complete"          # status used by `close`

[ai]
kind = "anthropic"
model = "<model-id>"
max_tasks = 10
max_description_chars = 4000

[policy]
kind = "weighted"
priority_weight = 10
overdue_bonus = 50
due_today_bonus = 30
due_this_week_bonus = 10
pending_comment_weight = 15
in_review_bonus = 5

[projects]
dir = "projects"                  # relativo al directorio de configuración
default = "acme"                  # opcional

[context]
global_dir = "context"
budget_chars = 12000              # presupuesto total de contexto por llamada a la IA

[playbooks]
shell = "auto"                    # auto | sh | bash | zsh | pwsh | cmd
default_timeout_secs = 600
run_log_dir = "runs"

[session]
notify = ["desktop", "terminal"]  # canales de notificación (FR-68)
ask_closing_comment = true        # `done` ofrece escribir un comentario de cierre (opcional)
restore_status_on_abort = true
cache_ttl_secs = 600              # vigencia de los datos de la tarea en caché (FR-55)

[session.status]                  # SUGERENCIAS por etapa; se pueden cambiar cuando quieras (FR-67)
ready   = ["preparado"]           # tras el análisis; si no existe en la lista, se avisa y no se cambia
working = ["in progress"]         # tras `approve`, salvo `--status`
final   = ["Code Review", "Ready For Testing"]  # opciones que `done` muestra primero

[ui]
group_by = "project"              # none | project
```

### 13.2 Cómo se actualiza

```text
config set ai.model "<model-id>"
   │
   ▼
ConfigKey::from_str("ai.model")       ← rechaza claves desconocidas
   │
   ▼
toml_edit::DocumentMut                ← edita el documento conservando comentarios y orden
   │
   ▼
toml::from_str::<AppSettings>(doc)    ← valida el resultado completo con serde
   │ ok                     │ error → "ai.model: expected string" (no se escribe nada)
   ▼
tempfile + persist (rename)           ← escritura atómica (NFR-14)
```

- **`toml` vs `toml_edit`:** `toml` deserializa a structs tipados (validación); `toml_edit` modifica el texto sin perder comentarios ni formato (FR-21). Se usan ambos.
- **Esquema versionado:** el campo `version` permite migraciones con un `match version { 1 => ..., 2 => ... }` al cargar (FR-25).
- **Sugerencias, no reglas:** `[session.status]` y `[session_status]` de cada proyecto se pueden cambiar en cualquier momento con `config set session.status.final '["QA"]'` o editando el archivo; ningún estado queda fijo en el código.

### 13.3 Registro de proveedores y asistente

```rust
// crates/bootstrap/src/registry.rs
pub struct FieldSpec {
    pub key: &'static str,            // "team_id"
    pub label: &'static str,          // "Workspace de ClickUp"
    pub kind: FieldKind,
}

pub enum FieldKind {
    Text { default: Option<&'static str> },
    Secret { secret: &'static str },  // nombre en el llavero, p. ej. "clickup_token"
    /// Opciones obtenidas en vivo; p. ej. los workspaces disponibles para el token.
    Discover(fn(&SettingsDraft) -> BoxFuture<'static, Result<Vec<Choice>, BuildError>>),
}

pub struct TaskProviderDescriptor {
    pub kind: &'static str,
    pub display_name: &'static str,
    pub fields: &'static [FieldSpec],
    pub build: fn(&AppSettings, &dyn SecretStore) -> Result<Arc<dyn TaskProvider>, BuildError>,
}

pub fn task_providers() -> &'static [TaskProviderDescriptor] {
    &[
        #[cfg(feature = "clickup")]
        adapter_clickup::DESCRIPTOR,
        #[cfg(feature = "github")]
        adapter_github::DESCRIPTOR,
    ]
}
```

El asistente recorre `fields` sin saber qué proveedor es. `bootstrap::build` usa el mismo descriptor para construir el adaptador: una sola fuente de verdad (NFR-17).

La configuración se deserializa con `serde` a un enum etiquetado, lo que hace que `bootstrap` elija el adaptador con un `match` exhaustivo:

```rust
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum ProviderConfig {
    Clickup(ClickUpConfig),
    // Github(GithubConfig),  // fase 11
}
```

### 13.4 Secretos

Nunca en `config.toml`. Orden de búsqueda: variable de entorno → llavero del sistema (`keyring`) → error con sugerencia.

| Variable                       | Llavero                        | Uso                       |
| ------------------------------ | ------------------------------ | ------------------------- |
| `TASKFLOW_CLICKUP_TOKEN`     | `taskflow/clickup_token`     | Token personal de ClickUp |
| `TASKFLOW_ANTHROPIC_API_KEY` | `taskflow/anthropic_api_key` | Clave de la API de IA     |

En desarrollo se pueden cargar desde `.env` con `dotenvy`; `.env.example` documenta las variables sin valores reales. Los secretos viajan en memoria como `SecretString` (crate `secrecy`), que no se imprime en `Debug` ni en logs.

## 14. Proyectos y contexto

### 14.1 `project.toml`

```toml
slug = "acme"
name = "ACME Corp"
kind = "client"                    # client | internal
repo_path = "~/code/acme-backend"  # opcional (FR-30)
priority_boost = 20                # opcional (FR-31)

[[match]]
level = "list"
id = "901100200300"                # lista "ACME – Sprint"

[[match]]
level = "space"
id = "90110001"                    # espacio "Clientes / ACME"

[[match]]
tag = "acme"

[session_status]                   # opcional: sugerencias propias del proyecto, tienen prioridad (FR-67)
final = ["Ready For Testing"]

[session_hooks]                    # opcional (FR-69): Playbooks del proyecto por etapa
on_approved = "create-branch"
```

`taskflow project add` genera este archivo: obtiene el árbol con `TaskProvider::locations()`, deja elegir nodos con un selector múltiple y crea además `context/` con plantillas vacías de negocio, código y despliegue.

### 14.2 Secciones de contexto

Cada sección es un archivo Markdown con *front matter* TOML (delimitado por `+++`, como Hugo o Zola), para reutilizar el crate `toml` en lugar de agregar un parser YAML.

```markdown
+++
title = "Modelo de negocio"
role = "knowledge"
usage = ["summary", "plan", "agent_prompt"]
priority = 10
+++

ACME vende software de facturación a pymes en Colombia y México.
- Los clientes pagan por número de facturas emitidas al mes.
- Cualquier cambio en el cálculo de impuestos requiere aprobación de su contador.
```

```markdown
+++
title = "Despliegue"
role = "knowledge"
usage = ["agent_prompt", "playbook"]
priority = 20
+++

Producción en AWS ECS. `make deploy VERSION=x` construye la imagen y actualiza el servicio.
Las migraciones se corren antes con `make migrate`. Staging se despliega con cada merge a `main`.
```

Sección global de aprendizaje (`<config_dir>/taskflow/context/90-aprendizaje-rust.md`):

```markdown
+++
title = "Aprendizaje: traits en Rust"
role = "instruction"
usage = ["summary", "agent_prompt"]
priority = 50
+++

Estoy aprendiendo Rust. Cuando propongas una solución en Rust:
- Prefiere diseñar con traits cuando haya más de una implementación posible y explica en 2–3 líneas por qué un trait y no un enum.
- Señala si conviene dispatch estático (genéricos) o dinámico (`dyn Trait`) y su costo.
- Agrega al final una sección "Para practicar" con un ejercicio corto relacionado con la tarea.
```

Campos del *front matter*:

| Campo        | Valores                                               | Por defecto        |
| ------------ | ----------------------------------------------------- | ------------------ |
| `title`    | Texto                                                 | Nombre del archivo |
| `role`     | `knowledge`, `instruction`                        | `knowledge`      |
| `usage`    | `summary`, `plan`, `agent_prompt`, `playbook` | Todos              |
| `priority` | 0–255, menor = más importante                       | 100                |
| `private`  | `true`, `false`                                   | `false`          |

La clave de la sección (`SectionKey`) es el nombre del archivo sin prefijo numérico ni extensión: `10-negocio.md` → `negocio`. El prefijo solo ordena los archivos en el explorador.

### 14.3 Cómo llega el contexto a la IA

```text
             ┌──────────────── BuildContext (UC-07) ────────────────┐
 global/  ─┐ │ merge por clave → quitar privadas → filtrar por uso  │
 proyecto/ ┴►│ → ordenar por prioridad → aplicar presupuesto        │
             └───────────────┬──────────────────────────┬───────────┘
                             │ instructions             │ knowledge
                             ▼                          ▼
                   system prompt (al final)   bloque <project_context> en el mensaje
```

- Las secciones de **instrucción** se agregan al *system prompt*, porque cambian cómo responde la IA.
- Las de **conocimiento** van en el mensaje del usuario, dentro de un bloque delimitado, porque son datos de referencia.
- Las secciones omitidas por presupuesto se informan en la salida (`omitted_sections`) y en modo verbose.
- `taskflow context preview <TASK_ID>` muestra exactamente este resultado sin llamar a la IA.

## 15. Playbooks

### 15.1 Formato

Cada Playbook es un archivo TOML en `projects/<slug>/playbooks/<name>.toml`.

| Clave del paso   | Obligatoria | Descripción                                                                                                    |
| ---------------- | ----------- | --------------------------------------------------------------------------------------------------------------- |
| `id`           | Sí         | Identificador único dentro del Playbook                                                                        |
| `kind`         | Sí         | `shell`, `select_tasks`, `extract_task_ids`, `comment`, `set_status`, `ai`, `confirm`, `custom` |
| `title`        | No          | Texto mostrado en el plan y en el progreso                                                                      |
| `when`         | No          | Plantilla que debe evaluar a verdadero para ejecutar el paso                                                    |
| `for_each`     | No          | `steps.<id>`: repite el paso por cada tarea o elemento                                                        |
| `confirm`      | No          | Pide confirmación individual antes del paso                                                                    |
| `on_error`     | No          | `stop` (por defecto) o `continue`                                                                           |
| `timeout_secs` | No          | Tiempo máximo (pasos`shell`); por defecto `playbooks.default_timeout_secs`                                 |

### 15.2 Ejemplo: revisar y desplegar una release

```toml
name = "release"
description = "Revisar y desplegar una release, comentar y cerrar las tareas incluidas"

[inputs.version]
prompt = "Tag de la versión a desplegar"
required = true

[inputs.previous]
prompt = "Tag anterior"
default = "{{ shell('git describe --tags --abbrev=0 ' ~ inputs.version ~ '^') }}"

[[steps]]
id = "commits"
title = "Commits incluidos"
kind = "shell"
run = "git log --oneline {{ inputs.previous }}..{{ inputs.version }}"

[[steps]]
id = "prs"
title = "PRs mergeados"
kind = "shell"
run = "gh pr list --state merged --base main --limit 30 --json number,title,headRefName"
capture = "json"

[[steps]]
id = "review"
title = "Revisión asistida por IA"
kind = "ai"
sections = ["despliegue", "codigo"]
prompt = """
Analiza los cambios de la release {{ inputs.version }}.
Señala riesgos de despliegue, migraciones necesarias y cambios incompatibles.

Commits:
{{ steps.commits.text }}

PRs:
{{ steps.prs.json | tojson }}
"""

[[steps]]
id = "gate"
kind = "confirm"
message = "¿Revisaste el análisis y quieres desplegar {{ inputs.version }}?"

[[steps]]
id = "deploy"
title = "Desplegar"
kind = "shell"
run = "make deploy VERSION={{ inputs.version }}"
timeout_secs = 1200
confirm = true

[[steps]]
id = "task_ids"
title = "Tareas referenciadas en commits y PRs"
kind = "extract_task_ids"
from = "{{ steps.commits.text }}\n{{ steps.prs.text }}"

[[steps]]
id = "tasks"
kind = "select_tasks"
ids_from = "steps.task_ids"
statuses = ["in_review"]

[[steps]]
id = "comment"
kind = "comment"
for_each = "steps.tasks"
body = "Desplegado en producción con la versión {{ inputs.version }}."

[[steps]]
id = "close"
kind = "set_status"
for_each = "steps.tasks"
status = "done"
```

El Playbook reproduce exactamente el proceso manual: desplegar, revisar commits y PRs, encontrar las tareas incluidas (por los IDs en mensajes de commit o nombres de rama), comentar y cerrar. Cada operación de escritura pasa por el plan aprobado y los puntos de confirmación.

### 15.3 Carga y validación

```text
release.toml ──serde──► PlaybookSpec ──TryFrom──► Playbook::new ──► validar plantillas ──► Playbook
 (adapter-fs)            (solo formato)            (invariantes del     (minijinja:          (listo para
                                                    dominio)             sintaxis y           planificar)
                                                                         variables)
```

Errores reportados juntos, por ejemplo:

```text
error: playbook `release` has 2 problems
  - step `tasks`: `ids_from` references `steps.task_id`, which does not exist (did you mean `steps.task_ids`?)
  - step `comment`: unknown variable `inputs.versión` in `body`
hint: run `taskflow playbook check release` after fixing the file
```

### 15.4 Shell y procesos

- `adapter-shell` ejecuta `run` con la shell configurada: `sh -c` en Unix por defecto y `pwsh -NoProfile -Command` en Windows (NFR-19).
- El directorio de trabajo por defecto es `project.repo_path`.
- Los procesos se crean con `kill_on_drop(true)`; al cancelar o vencer el tiempo máximo se termina el proceso (y su grupo en Unix) para no dejar huérfanos (NFR-15).
- Las entradas marcadas `secret = true` se pasan como variables de entorno, no interpoladas en la línea de comando, y se enmascaran en la salida (NFR-13).

## 16. Adaptador de ClickUp

- **Autenticación:** token personal en el header `Authorization`.
- **Base URL:** `https://api.clickup.com/api/v2`

| Port method                  | ClickUp endpoint                                                                                                                                            |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `current_user`             | `GET /user`                                                                                                                                               |
| `search_tasks`             | `GET /team/{team_id}/task?assignees[]=...&list_ids[]=...&space_ids[]=...&statuses[]=...&tags[]=...&page={n}`                                              |
| `search_tasks` con `ids` | `GET /task/{task_id}` por cada ID, concurrente con límite                                                                                                |
| `task`                     | `GET /task/{task_id}`                                                                                                                                     |
| `comments`                 | `GET /task/{task_id}/comment`                                                                                                                             |
| `locations`                | `GET /team/{team_id}/space` → `GET /space/{space_id}/folder` → `GET /folder/{folder_id}/list` y `GET /space/{space_id}/list` (listas sin carpeta) |
| `set_status`               | `PUT /task/{task_id}` with `{ "status": "<name>" }`                                                                                                     |
| `add_comment`              | `POST /task/{task_id}/comment` with `{ "comment_text": "...", "notify_all": false }`                                                                    |
| `TimeTracker::start`       | `POST /team/{team_id}/time_entries/start` with `{ "tid": "<task_id>" }`                                                                                 |
| `TimeTracker::stop`        | `POST /team/{team_id}/time_entries/stop`                                                                                                                  |
| `TimeTracker::current`     | `GET /team/{team_id}/time_entries/current`                                                                                                                |
| `available_statuses`       | `GET /task/{task_id}` → `list.id` → `GET /list/{list_id}` (campo `statuses`, cada uno con `status` y `type`: open, custom, done, closed)      |

**Separación interna:**

- `api_types.rs`: structs que reflejan el JSON de ClickUp tal cual (`#[derive(Deserialize)]`, `#[serde(rename = "...")]`).
- `mapper.rs`: `impl TryFrom<ApiTask> for Task`. Aquí vive toda la traducción, incluido el mapeo de estados desde la configuración y la construcción de `TaskLocation` a partir de `space`, `folder` y `list` de la tarea.
- `provider.rs`: `impl TaskProvider for ClickUpProvider`.
- `descriptor.rs`: `DESCRIPTOR` para el registro (campos del asistente y función de construcción).
- `time_tracker.rs`: `impl TimeTracker for ClickUpTimeTracker`. ClickUp permite un solo contador activo por usuario; `start` verifica `current` antes de iniciar para no pisar otro contador.

**Consideraciones:**

- **Paginación:** iterar páginas hasta recibir una lista vacía.
- **Límite de peticiones:** ClickUp limita las peticiones por minuto según el plan; limitar la concurrencia con un `tokio::sync::Semaphore` y reintentar con espera ante HTTP 429.
- **Fechas:** ClickUp devuelve timestamps en milisegundos como string; convertir en `mapper.rs`.
- **Referencias a tareas en texto:** `task_ref_pattern` reconoce por defecto `CU-<id>` y `#<id>`, el formato que ClickUp sugiere para ramas y commits; se puede sobrescribir por Playbook con `pattern`.

**Verificado en la documentación oficial (fase 3, 2026-10-11):**

- `Authorization: <token>` sin prefijo `Bearer` para tokens personales (`pk_...`); `Bearer` solo para OAuth.
- `GET /user` → `{ "user": { "id": <entero>, "username": ... } }`. Los ids de usuario son **números**; las tareas, listas, carpetas y espacios usan ids **texto**.
- `GET /team/{team_id}/task`: páginas de hasta 100 tareas, `page` empieza en 0. Filtros: `assignees[]`, `space_ids[]`, `project_ids[]` (carpetas), `list_ids[]`, `statuses[]`, `tags[]`, `include_closed`, `subtasks`. El campo `last_page` no figura en el esquema: el adaptador lo usa si viene y, si no, se detiene ante una página con menos de 100 tareas.
- Tarea: `status { status, type }`, `priority` nulo o `{ id: "1".."4", priority }`, `due_date` y `date_updated` en milisegundos como texto, `folder { hidden }` (las listas sin carpeta tienen una carpeta oculta), `space` solo con `id`.
- `GET /task/{id}/comment`: los 25 comentarios más recientes; cada uno con `comment_text`, `user`, `resolved`, `assignee` (nulo si no está asignado) y `date` en milisegundos. Se pagina con `start` + `start_id` del último comentario.

*Pendiente de verificar:* endpoints de time tracking (y si iniciar un contador detiene el anterior) y el límite de peticiones del plan.

## 17. Adaptador de IA

- **Endpoint:** `POST https://api.anthropic.com/v1/messages`
- **Headers:** `x-api-key`, `anthropic-version`, `content-type: application/json`

**Diseño de prompts** (en `prompts.rs`, como constantes o plantillas):

- **System prompt:** rol de asistente técnico que analiza tareas de desarrollo; responde en español; devuelve exclusivamente JSON con el esquema indicado; al final, las secciones de contexto de tipo *instrucción*.
- **Mensaje del usuario:** datos de la tarea + bloque `<project_context>` con las secciones de *conocimiento*.
- **Esquema de salida para `summarize_task`:**

```json
{
  "what_is_requested": "string",
  "suggested_steps": ["string"],
  "open_questions": ["string"],
  "estimated_complexity": "low | medium | high",
  "agent_prompt": "string"
}
```

- La respuesta se deserializa con `serde_json` directamente a `TaskSummary`. Si el JSON es inválido, se reintenta una vez y luego se devuelve `AiError::InvalidResponse`.
- `complete` (pasos `ai` de Playbooks) devuelve texto libre; su resultado se muestra y puede usarse en plantillas, pero **nunca se ejecuta como comando** (NFR-12).
- **Esquema de salida para `analyze_work`** (reporte de trabajo de la sesión):

```json
{
  "what_is_requested": "string",
  "acceptance_criteria": ["string"],
  "proposed_steps": ["string"],
  "risks": ["string"],
  "open_questions": ["string"],
  "complexity": "low | medium | high",
  "agent_prompt": "string"
}
```

**Control de costo:** `max_tasks` en `plan`, truncar descripciones largas (por ejemplo, a 4.000 caracteres), incluir solo los últimos N comentarios y respetar `context.budget_chars`.

## 18. Manejo de errores

| Capa            | Tipo                                                           | Crate         | Ejemplos                                                                                                                                                                |
| --------------- | -------------------------------------------------------------- | ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `domain`      | `DomainError`, `PlaybookIssue`, `SessionError`           | `thiserror` | `EmptyTaskId`, `InvalidSlug`, `DuplicateStep`, `UnknownOrLaterStep`, `InvalidTransition { from, command, allowed }`, `NoReport`, `ReportAlreadyPublished` |
| Puertos         | `ProviderError`, `AiError`                                 | `thiserror` | `Unauthorized`, `NotFound`, `RateLimited { retry_after }`, `Network`, `InvalidResponse`                                                                       |
| Puertos         | `ConfigError`, `SecretError`, `RepoError`, `LoadError` | `thiserror` | `InvalidValue { key, reason }`, `UnknownKey`, `NotFound(path)`, `Invalid(Vec<PlaybookIssue>)`                                                                   |
| Puertos         | `RunnerError`, `InteractionError`                          | `thiserror` | `Timeout`, `Cancelled`, `SpawnFailed`, `Aborted`                                                                                                                |
| Puertos         | `NotifyError`                                                | `thiserror` | `Unavailable`, `PermissionDenied`                                                                                                                                   |
| `application` | `AppError`, `StepError`                                    | `thiserror` | Envuelven los anteriores con`#[from]`                                                                                                                                 |
| Interfaces      | `anyhow::Error`                                              | `anyhow`    | Agrega contexto:`.context("loading config")`                                                                                                                          |

Cada adaptador convierte sus errores internos (`reqwest::Error`, `std::io::Error`, códigos HTTP) a errores de su puerto. Así el núcleo nunca ve un error de `reqwest` ni de `tokio::process`.

Mensajes al usuario (NFR-10), ejemplos:

```text
error: ClickUp rejected the token (401 Unauthorized)
hint: check TASKFLOW_CLICKUP_TOKEN or run `taskflow config init`
```

```text
error: step `deploy` failed (exit code 2) after 41s
hint: 6 of 9 steps ran; see ~/.config/taskflow/runs/2026-10-10T14-02-11_acme_release.jsonl
```

```text
error: cannot `approve` while the session is `analyzing`
hint: allowed actions now: abort. Run `taskflow session` to see the current state
```

```text
error: could not change task status to "preparado" (ClickUp 503)
hint: timer is running and the report is saved; run `taskflow session retry` to apply the 2 pending steps
```

```text
error: status "Ready For Testing" does not exist in list "ACME – Sprint"
hint: available now: to do, preparado, in progress, Code Review, complete
```

## 19. Observabilidad

- `tracing` con spans por caso de uso (`today`, `plan`, `run`), por paso de Playbook y por petición HTTP.
- `-v` activa nivel `debug`; nunca se registran tokens, secretos ni cuerpos completos de respuestas.
- Duración de cada caso de uso visible en modo verbose para validar NFR-06.
- Cada ejecución de Playbook tiene un `RunId` y su registro en `runs/` (FR-49).

## 20. Estrategia de pruebas

| Nivel               | Qué se prueba                                                                                                           | Herramientas                                                                                                                                                                                          |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Unitarias           | `WeightedPolicy`, `TaskId::new`, `ProjectSlug`, `ProjectResolver`, `ProjectContext::select`, `Playbook::new` | `#[test]`, `FixedClock`                                                                                                                                                                           |
| Propiedades         | Slugs válidos siempre parsean; la selección de contexto nunca supera el presupuesto                                    | `proptest`                                                                                                                                                                                          |
| Casos de uso        | Flujos completos con fakes de todos los puertos                                                                          | `#[tokio::test]`, `FakeTaskProvider`, `FakeSummarizer`                                                                                                                                          |
| Motor de Playbooks  | Orden de pasos,`for_each`, `when`, errores, cancelación, rechazo de confirmaciones                                  | `FakeCommandRunner`, `ScriptedInteraction`                                                                                                                                                        |
| Máquina de estados | Cada par (estado, comando): siguiente estado y efectos esperados; pares inválidos rechazados                            | Pruebas por tabla;`proptest` con secuencias aleatorias de comandos (nunca dos intervalos abiertos, nunca un estado terminal con contador activo, nunca se publica el reporte sin `PublishReport`) |
| Servicio de sesión | Fallo de un efecto crítico y reanudación con`session retry`; fallo de notificación no interrumpe                    | `FakeTimeTracker`, `FakeNotifier` que falla, `FixedClock`                                                                                                                                       |
| Snapshots           | Vista previa de Playbooks, prompts generados, tablas del CLI                                                             | `insta`                                                                                                                                                                                             |
| Adaptadores         | Deserialización y mapeo con JSON real guardado                                                                          | `tests/fixtures/*.json`                                                                                                                                                                             |
| Archivos            | `config set` conserva comentarios; escritura atómica; front matter                                                    | `tempfile`                                                                                                                                                                                          |
| HTTP                | Peticiones, paginación, 401, 429                                                                                        | `wiremock`                                                                                                                                                                                          |
| Shell               | Timeout, cancelación, salida en vivo, por sistema operativo                                                             | `adapter-shell` en la matriz de CI                                                                                                                                                                  |
| Contrato            | Suite común que todo`TaskProvider` debe pasar                                                                         | `fn provider_contract<P: TaskProvider>(p: P)`                                                                                                                                                       |
| E2E (manual)        | CLI contra una cuenta real de prueba                                                                                     | Feature`e2e`, fuera de CI                                                                                                                                                                           |

La prueba de contrato es especialmente valiosa: cuando se agregue GitHub Issues, se ejecuta la misma suite y se sabe de inmediato si el adaptador cumple el contrato.

## 21. Calidad y CI

GitHub Actions en cada push y pull request:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `cargo build --release` en matriz `ubuntu-latest`, `windows-latest`, `macos-latest`

Prácticas de proyecto profesional:

- `[workspace.dependencies]` y `[workspace.lints]` centralizan versiones y lints (`unsafe_code = "forbid"`, `clippy::pedantic` en `domain`).
- `cargo deny` para licencias y vulnerabilidades conocidas.
- Documentación con `cargo doc` y ejemplos en *doctests* para los tipos del dominio.

Releases con versionado semántico y `CHANGELOG.md`. Más adelante, `cargo-dist` para publicar binarios por plataforma.

## 22. Seguridad

- Tokens solo en variables de entorno o llavero; `.env` en `.gitignore`; `SecretString` en memoria.
- Toda operación de escritura (`close`, pasos `comment`/`set_status`) requiere confirmación salvo `--yes`.
- Los Playbooks solo ejecutan comandos escritos por el usuario en sus archivos; la salida de la IA nunca se ejecuta (NFR-12).
- El plan completo se muestra antes de ejecutar; los pasos peligrosos (`deploy`) llevan `confirm = true`.
- Las entradas secretas se pasan como variables de entorno y se enmascaran en pantalla y registros (NFR-13).
- Los logs nunca incluyen secretos (el header `Authorization` se marca como sensible).
- A la IA solo se envía el `SummaryInput` mínimo y las secciones no privadas (NFR-05, FR-39).
- Solo el usuario indica que una tarea terminó (`done`); ninguna herramienta externa puede cerrar una sesión.
- El reporte de trabajo se guarda en local y nunca se publica automáticamente; publicarlo es una acción explícita (`report publish`).

## 23. Mapa de conceptos de Rust

| Concepto                               | Dónde se aplica                                                                                                                                                     |
| -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Ownership y borrowing                  | `&Task`, `&mut [Task]`, `GetTodayTasks<'a>` que toma prestado el contexto                                                                                      |
| Lifetimes                              | `pending_comments_for<'a>`, `ScoringContext<'a>`, `ContextSelection<'a>`, `ProjectResolver::group<'t>`                                                       |
| Structs, enums,`match`               | Dominio,`ProviderConfig`, `StepKind`, comandos del CLI                                                                                                           |
| Newtypes y constructores validados     | `TaskId::new`, `ProjectSlug`, `SectionKey`, `StepId`, `ConfigKey`                                                                                          |
| `FromStr`, `TryFrom`, `From`     | Slugs desde CLI (`clap`), spec TOML → dominio, errores con `#[from]`                                                                                            |
| Traits y métodos por defecto          | Puertos,`PrioritizationPolicy::rank`                                                                                                                               |
| Trait objects                          | `Arc<dyn TaskProvider>`, `Arc<dyn Action>`                                                                                                                       |
| Enum dispatch vs`dyn`                | `StepKind` (cerrado, exhaustivo) frente a `Action` (abierto, extensible)                                                                                         |
| Genéricos y trait bounds              | `fn provider_contract<P: TaskProvider>`, `PlaybookRun<S>`                                                                                                        |
| Typestate                              | `PlaybookRun<Planned>` → `PlaybookRun<Approved>`                                                                                                                |
| Máquinas de estados con enums         | `SessionState`, `match (estado, comando)` con guardas `if` y patrones `A \| B`                                                                                |
| Enums con datos                        | `StatusTarget::Explicit(StatusName)`, `Publication::Published { at }`, `ReportOutcome::Unavailable { reason }`, `SessionCommand::Finish { status, comment }` |
| Enum en lugar de`bool`               | `Publication::Draft` / `Published { at }`: el estado lleva sus datos                                                                                             |
| Hacer imposible lo inválido con tipos | `Finish` exige un `StatusName`: no se puede cerrar sin elegir estado                                                                                             |
| Patrones anidados y guardas            | `PostComment(CommentKind::Report)`, `(state, C::PublishReport) if !state.is_terminal()`                                                                          |
| Typestate frente a enum                | Playbooks (estado conocido al compilar) frente a sesiones (estado leído de disco)                                                                                   |
| Ownership para expresar fin de vida    | `SessionStore::archive(session: WorkSession)` consume la sesión                                                                                                   |
| Genéricos para decoradores            | `CachedTaskProvider<P: TaskProvider, C: Cache>`                                                                                                                    |
| Composite con trait objects            | `MultiNotifier(Vec<Box<dyn Notifier>>)`                                                                                                                            |
| Tipos recursivos                       | `LocationNode { children: Vec<LocationNode> }`                                                                                                                     |
| Colecciones                            | `IndexMap` (orden de inserción), `BTreeMap` (orden por clave), `EnumSet` (bitset)                                                                             |
| Traits estándar                       | `Display`, `Default`, `Ord`, `Hash`                                                                                                                          |
| `Option`, `Result`, `?`          | Todo el código;`bool::then`, `is_some_and`, `map_or`                                                                                                          |
| Errores tipados                        | `thiserror` en librerías, `anyhow` en binarios, `Vec<PlaybookIssue>` para errores acumulados                                                                  |
| Iteradores y closures                  | `retain`, `filter_map`, `max_by_key`, `zip`, `sort_by_cached_key`                                                                                          |
| Async y concurrencia                   | `tokio`, `try_join_all`, `Semaphore`, `spawn_blocking`                                                                                                       |
| Canales y cancelación                 | `mpsc` para `RunEvent`, `CancellationToken`, `tokio::select!`                                                                                                |
| Procesos                               | `tokio::process::Command`, `kill_on_drop`, timeouts                                                                                                              |
| `Send`, `Sync`, `Arc`            | `AppContext` compartido entre hilos                                                                                                                                |
| Serde                                  | JSON de APIs, TOML, DTOs, enums etiquetados, front matter                                                                                                            |
| Punteros a función y`static`        | Registro de proveedores (`fn` en descriptores)                                                                                                                     |
| Workspace y visibilidad                | Crates separados,`pub` vs `pub(crate)`, campos privados                                                                                                          |
| Feature flags y`cfg`                 | `--features github`, `--features e2e`, registro condicional                                                                                                      |
| `#[non_exhaustive]`                  | Enums públicos de errores, eventos y pasos                                                                                                                          |
| Testing                                | Fakes, fixtures,`wiremock`, `proptest`, `insta`, pruebas de contrato                                                                                           |
| Macros (avanzado)                      | `macro_rules!` para registrar adaptadores o acciones                                                                                                               |

## 24. Roadmap

| Fase | Entregable                                                         | Criterio de terminado                                                                                                                                                                  | Conceptos                                                              |
| ---- | ------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| 0    | Workspace y CI                                                     | `fmt`, `clippy` y `test` pasan en CI                                                                                                                                             | Cargo, workspace, lints                                                |
| 1    | `domain`: tareas y política                                     | Tests de`WeightedPolicy` con `FixedClock`                                                                                                                                          | Structs, enums, traits, lifetimes                                      |
| 2    | Puertos y`GetTodayTasks` con fakes                               | Caso de uso probado sin red                                                                                                                                                            | `async_trait`, `Arc<dyn>`                                          |
| 3    | `adapter-clickup` de lectura                                     | Imprime las tareas reales con su ubicación                                                                                                                                            | `reqwest`, `serde`, `TryFrom`, paginación                       |
| 4    | Configuración persistente +`bootstrap` + CLI `today`/`show` | `config init` interactivo deja todo listo; `taskflow today` reemplaza la revisión manual                                                                                          | `toml_edit`, escritura atómica, registro, `dialoguer`             |
| 5    | Proyectos y agrupación                                            | `today --by-project`; `project add` elige listas de ClickUp                                                                                                                        | `FromStr`, `IndexMap`, tipos recursivos                            |
| 6    | Contexto por proyecto +`adapter-anthropic` + `plan`            | Resúmenes y prompts usan las secciones;`context preview`                                                                                                                            | `EnumSet`, lifetimes en selección, prompts, JSON estructurado       |
| 7    | `close` (escritura)                                              | Cerrar una tarea sin abrir ClickUp                                                                                                                                                     | Confirmaciones, errores HTTP                                           |
| 8    | Sesión de trabajo                                                 | `start` → reporte → `approve` → `done` con el contador de ClickUp, estados sugeridos y validados por lista, publicación explícita del reporte y notificación de escritorio | Máquina de estados con enums, efectos,`TimeTracker`, `Notifier`   |
| 9    | Playbooks v1                                                       | `release` corre de punta a punta con `shell`, `select_tasks`, `comment`, `set_status`, `confirm`, `--dry-run`                                                            | Typestate, enum dispatch,`tokio::process`, canales, `select!`      |
| 10   | Playbooks v2                                                       | Pasos`ai` y `extract_task_ids`, `for_each`/`when`, registro de ejecuciones, acciones `custom`, ganchos de sesión                                                            | `minijinja`, `dyn Action`, JSON Lines                              |
| 11   | Segundo proveedor (GitHub Issues)                                  | Pasa la suite de contrato sin tocar el núcleo y aparece en el asistente; sesiones con`LocalTimeTracker`                                                                             | Genéricos, feature flags, objeto nulo                                 |
| 12   | Caché local                                                       | `start` y `plan` reutilizan datos y análisis sin repetir llamadas                                                                                                                 | Decorador`CachedTaskProvider`, puerto `Cache`, `sqlx` o archivos |
| 13   | Interfaz gráfica (opcional)                                       | App Tauri con la vista`today`, sesión activa y ejecución de Playbooks                                                                                                              | Reutilización de`application`, `Interaction` nativo               |

## 25. Registro de decisiones (ADR)

| ID      | Decisión                                                                   | Motivo                                                                                         |
| ------- | --------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| ADR-001 | Arquitectura hexagonal                                                      | Independencia de proveedor e interfaz (NFR-01 a NFR-03)                                        |
| ADR-002 | `async_trait` en puertos                                                  | Los`async fn` nativos en traits no son compatibles con `dyn`                               |
| ADR-003 | `Arc<dyn>` en `AppContext`                                              | Compartir el contexto entre hilos en futuras interfaces                                        |
| ADR-004 | Crate`bootstrap` separado                                                 | Evitar duplicar el armado de dependencias entre interfaces                                     |
| ADR-005 | DTOs serializables en la frontera                                           | Mismo resultado para terminal, JSON, Tauri y web                                               |
| ADR-006 | Tauri como opción de escritorio                                            | Backend en Rust que reutiliza`application` directamente                                      |
| ADR-007 | TOML +`toml_edit` para la configuración                                  | Legible, versionable y editable sin perder comentarios (FR-21)                                 |
| ADR-008 | Secretos en llavero con prioridad de variables de entorno                   | Nada sensible en disco en texto plano; compatible con CI (NFR-04)                              |
| ADR-009 | Registro estático de descriptores de proveedor                             | Una sola fuente para el asistente y la construcción (NFR-17)                                  |
| ADR-010 | Contexto en Markdown con front matter TOML, secciones abiertas              | Fácil de escribir y versionar; sin parser YAML adicional                                      |
| ADR-011 | Selección de contexto en`domain`                                         | Regla de negocio pura, probable con`proptest`                                                |
| ADR-012 | Playbooks declarativos en TOML, sin lenguaje de scripting en v1             | Pasos explícitos, auditables y validables antes de ejecutar                                   |
| ADR-013 | Enum dispatch para pasos integrados +`trait Action` para extensión       | Exhaustividad del compilador y apertura controlada (NFR-18)                                    |
| ADR-014 | Typestate para aprobar Playbooks                                            | La confirmación (FR-46) la garantiza el compilador, no la disciplina                          |
| ADR-015 | Puerto`Interaction` implementado por la interfaz                          | Decisiones a mitad de ejecución sin acoplar la aplicación a la terminal                      |
| ADR-016 | Eventos por`mpsc` en lugar de imprimir                                    | Misma ejecución para CLI, Tauri y registro en archivo                                         |
| ADR-017 | `minijinja` para plantillas                                               | Sintaxis conocida, sin I/O, permite listar variables no declaradas para validar                |
| ADR-018 | Sesión de trabajo como enum de estados, no typestate                       | El estado se lee de disco entre ejecuciones; solo se conoce en tiempo de ejecución            |
| ADR-019 | El dominio devuelve efectos y la aplicación los ejecuta con un diario      | Lógica pura y fácil de probar; reintentos sin repetir lo ya hecho (NFR-21)                   |
| ADR-020 | `TimeTracker` y `Notifier` como puertos separados de `TaskProvider`   | Segregación de interfaces; proveedores sin contador usan`LocalTimeTracker`                  |
| ADR-021 | Caché como decorador genérico de`TaskProvider`                          | Se activa en`bootstrap` sin cambiar adaptadores ni casos de uso                              |
| ADR-022 | `serde` permitido en `domain` solo como derive                          | Persistir la sesión sin duplicar tipos; el dominio sigue sin I/O                              |
| ADR-023 | Estados del flujo como sugerencias + elección validada en cada transición | Los estados cambian por lista y con el tiempo; nada queda fijo en código ni en configuración |
| ADR-024 | Publicar el reporte es un comando explícito (`PublishReport`)            | El reporte es un borrador de trabajo; el usuario decide qué y cuándo se comparte             |
| ADR-025 | Solo`done` (usuario) lleva la sesión a `Closed`                        | El usuario es quien declara terminada una tarea; el agente no cierra nada                      |

## 26. Decisiones abiertas

- [X] Verificar en la API de ClickUp los campos exactos de comentarios asignados y resueltos (fase 3, ver §16).
- [X] Verificar los filtros soportados por `GET /team/{team_id}/task` (fase 3, ver §16).
- [ ] Definir los estados reales del espacio de trabajo para `status_map`.
- [ ] Elegir entrega del prompt al agente: portapapeles (`arboard`), archivo o ambos.
- [ ] Decidir si la caché usa SQLite (`sqlx`) o archivos JSON.
- [ ] Decidir si el resumen de la IA responde siempre en español o según el idioma de la tarea.
- [ ] Decidir si el contexto del proyecto puede vivir también dentro del repositorio (`<repo>/.taskflow/context/`) para compartirlo con el equipo.
- [ ] Decidir si se permiten Playbooks globales (no ligados a un proyecto).
- [ ] Evaluar un lenguaje embebido (`rhai`) o plugins WASM para Playbooks avanzados después de v1.
- [ ] Definir la política de `select_tasks`: solo tareas propias o todas las del proyecto por defecto.
- [ ] Decidir si el tiempo se registra con el contador en vivo de ClickUp o como una entrada con duración al cerrar.

### Decisiones tomadas

| Fecha      | Tema                                | Resolución                                                                                                                                                                                                                                                    |
| ---------- | ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-10-10 | Estados del flujo                   | Son variables: dependen de la lista de cada tarea (por ejemplo "Code Review" o "Ready For Testing") y pueden cambiar en cualquier momento. La configuración guarda solo sugerencias; el estado se elige y valida en cada transición (FR-67, FR-70, ADR-023). |
| 2026-10-10 | Quién indica que la tarea terminó | Siempre el usuario, de forma explícita con`taskflow done` (FR-61, ADR-025).                                                                                                                                                                                 |
| 2026-10-10 | Publicación del reporte            | Queda como borrador local; se publica solo de forma explícita con`taskflow report publish`, antes de cerrar la tarea (FR-71, ADR-024).                                                                                                                      |

## 27. Glosario

| Término                | Definición                                                                                                          |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------- |
| Puerto                  | Trait que define lo que el núcleo necesita o lo que ofrece                                                          |
| Adaptador               | Implementación concreta de un puerto para una tecnología                                                           |
| Caso de uso             | Operación de la aplicación invocada por una interfaz                                                               |
| Composition root        | Único lugar donde se crean y conectan las dependencias (`bootstrap`)                                              |
| DTO                     | Estructura de datos serializable que cruza la frontera hacia las interfaces                                          |
| Dispatch dinámico      | Resolución del método en ejecución mediante`dyn Trait`                                                          |
| Enum dispatch           | Resolución del comportamiento con un`match` sobre un enum cerrado                                                 |
| Typestate               | Codificar el estado de un objeto en su tipo para que el compilador impida operaciones inválidas                     |
| Fake                    | Implementación de prueba de un trait con datos controlados                                                          |
| Prueba de contrato      | Suite común que valida que cualquier implementación de un trait se comporta igual                                  |
| Proyecto                | Cliente o proyecto de trabajo; agrupa tareas mediante reglas sobre ubicaciones del proveedor                         |
| Ubicación              | Posición de una tarea en la jerarquía del proveedor (espacio, carpeta, lista, repositorio)                         |
| Sección de contexto    | Archivo Markdown que describe un aspecto del proyecto (conocimiento) o indica a la IA cómo responder (instrucción) |
| Presupuesto de contexto | Máximo de caracteres de contexto enviados a la IA por llamada                                                       |
| Playbook                | Secuencia con nombre de pasos explícitos que automatiza un proceso operativo de un proyecto                         |
| Paso                    | Unidad de un Playbook: comando, consulta, comentario, cambio de estado, IA o confirmación                           |
| Acción nativa          | Paso implementado en Rust mediante el trait`Action`                                                                |
| Descriptor de proveedor | Metadatos estáticos de un proveedor: campos que pide el asistente y cómo se construye                              |
| Sesión de trabajo      | Seguimiento de una tarea desde`start` hasta `done`: contador, análisis, aprobación y cierre                    |
| Reporte de trabajo      | Resultado del análisis de la IA: qué hacer, criterios, pasos, riesgos, preguntas y prompt del agente               |
| Luz verde               | Aprobación explícita del reporte (`taskflow approve`) que pasa la tarea al estado de trabajo                     |
| Efecto                  | Operación externa (contador, estado, notificación) que el dominio solicita y la aplicación ejecuta                |
| Diario de efectos       | Registro de efectos aplicados, pendientes y fallidos de una sesión; base de`session retry`                        |
| Decorador               | Implementación de un trait que envuelve otra y le agrega comportamiento (por ejemplo, caché)                       |
| Estado sugerido         | Estado del proveedor propuesto por la configuración para una etapa; se usa solo si existe en la lista de la tarea   |
| Borrador del reporte    | Reporte de trabajo guardado en local, editable, que solo se publica con`report publish`                            |
