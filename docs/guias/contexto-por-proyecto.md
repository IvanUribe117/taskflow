# Guía: crear el contexto de un proyecto

Las **secciones de contexto** le dicen a la IA lo que necesita saber de un proyecto para resumir sus tareas, proponer un plan o escribir el prompt para el agente de código. Sin ellas, la IA solo ve el título y la descripción de la tarea; con ellas, sabe qué hace el sistema, cómo está construido y cómo trabaja el equipo.

Esta guía explica qué escribir, dónde, y cómo mantenerlo al día. Los detalles técnicos de cómo Taskflow elige las secciones están en `docs/learning/fase-6.md`.

---

## 1. Dónde viven

`taskflow config path` muestra la ruta de `config.toml`; las secciones van en la misma carpeta:

```text
<carpeta de config.toml>/                      macOS: ~/Library/Application Support/taskflow/
├── context/                                   secciones GLOBALES: aplican a todos los proyectos
│   └── 90-aprendizaje-rust.md
└── projects/
    └── <slug>/                                un proyecto (creado con `taskflow project add`)
        ├── project.toml
        └── context/                           secciones de ESE proyecto
            ├── 10-negocio.md
            ├── 20-arquitectura.md
            └── 30-desarrollo.md
```

- **El nombre del archivo es la clave:** `10-negocio.md` → `negocio`. El número de adelante solo ordena los archivos en la carpeta.
- **Una sección del proyecto reemplaza a la global con la misma clave.** Puedes tener un `estilo.md` global y uno distinto para un cliente que trabaja de otra forma.
- La clave usa minúsculas, dígitos y guiones: `10-modelo-de-negocio.md` sí, `10-Modelo Negocio.md` no.

En macOS la ruta tiene un espacio. Para la terminal:

```sh
TF="$HOME/Library/Application Support/taskflow"
mkdir -p "$TF/projects/<slug>/context"
```

---

## 2. El formato

Markdown con un encabezado TOML entre líneas `+++`:

```markdown
+++
title = "Modelo de negocio"
role = "knowledge"
usage = ["summary", "plan", "agent_prompt"]
priority = 10
+++

El cuerpo: texto normal en Markdown.
```

| Campo | Valores | Si no lo pones |
| --- | --- | --- |
| `title` | Texto libre | La clave |
| `role` | `knowledge` o `instruction` | `knowledge` |
| `usage` | Lista con `summary`, `plan`, `agent_prompt`, `playbook` | Todos |
| `priority` | 0 a 255; **menor = más importante** | 100 |
| `private` | `true` o `false` | `false` |

El encabezado es opcional: un archivo con solo texto es una sección de conocimiento, para todos los usos y con prioridad 100.

---

## 3. Las decisiones que importan

### `role`: ¿describe el proyecto o le dice a la IA cómo responder?

| `knowledge` (conocimiento) | `instruction` (instrucción) |
| --- | --- |
| Datos sobre el proyecto | Cómo quieres que responda la IA |
| Va en el mensaje, como material de referencia | Va en el *system prompt*, y cambia el comportamiento |
| "IDK es una plataforma de seguimiento de tiempo en Django" | "Propón la solución más simple; nada de abstracciones especulativas" |
| "Los despliegues pasan por GitHub Actions" | "Explica en 2–3 líneas por qué elegiste ese diseño" |

**Regla práctica:** si la frase empieza con un verbo en imperativo dirigido a la IA ("propón", "no uses", "explica"), es una instrucción.

### `usage`: ¿en qué llamadas sirve?

| Uso | Cuándo se envía | Qué le sirve |
| --- | --- | --- |
| `summary` | Al resumir una tarea (`show`) | Qué es el negocio, glosario, arquitectura general |
| `plan` | Al armar el plan del día (`plan`) | Qué es importante para el cliente; poco más |
| `agent_prompt` | Al escribir el prompt para el agente de código | Todo lo técnico: arquitectura, comandos, convenciones, estilo |
| `playbook` | En los pasos de IA de las automatizaciones | Despliegue, procesos |

Ser selectivo ahorra presupuesto: los comandos de desarrollo no ayudan a resumir una tarea, y el estilo de código no sirve para planear el día.

### `priority` y el presupuesto

Cada llamada tiene un **presupuesto de caracteres** (`context.budget_chars`, por defecto 12.000) que comparten **todas** las secciones que entran, globales y del proyecto. Entran por prioridad; las que no caben se omiten y `context preview` lo avisa.

| Prioridad | Para qué |
| --- | --- |
| 0–19 | Imprescindible: el negocio, lo que no se puede romper |
| 20–49 | Importante: arquitectura, glosario, estilo |
| 50–99 | Útil: desarrollo, despliegue, instrucciones personales |
| 100+ | Si sobra espacio |

### `private`: lo que nunca debe salir

Una sección con `private = true` aparece en `context list` pero **nunca** se envía a la IA. Sirve para notas personales sobre el proyecto. Aun así, **no guardes secretos** (tokens, contraseñas, datos de clientes) en ninguna sección: son archivos de texto plano.

---

## 4. Qué secciones crear

Un buen punto de partida para un proyecto de desarrollo:

| Archivo | `role` | `usage` | Prioridad | Contenido |
| --- | --- | --- | --- | --- |
| `10-negocio.md` | knowledge | summary, plan, agent_prompt | 10 | Qué hace el sistema, para quién, módulos principales, qué no se puede romper |
| `15-estilo-codigo.md` | instruction | agent_prompt | 15 | Cómo escribe código el equipo: simplicidad, tests, comentarios, lo que se rechaza en revisión |
| `20-arquitectura.md` | knowledge | summary, agent_prompt | 20 | Stack y cómo encajan las piezas; dónde vive cada cosa en el repo |
| `25-glosario.md` | knowledge | summary, agent_prompt | 25 | Los términos del negocio, una línea cada uno |
| `30-desarrollo.md` | knowledge | agent_prompt | 30 | Levantar el proyecto, correr tests y linters, convenciones de ramas y commits |
| `40-despliegue.md` | knowledge | agent_prompt, playbook | 40 | CI/CD y cómo se despliega |

Y globales, para todos los proyectos:

| Archivo | `role` | `usage` | Contenido |
| --- | --- | --- | --- |
| `90-aprendizaje-rust.md` | instruction | summary, agent_prompt | Lo que estás aprendiendo y cómo quieres que la IA te lo enseñe |
| `95-idioma.md` | instruction | todos | "Responde en español" u otras preferencias generales |

No hace falta tenerlas todas desde el primer día. Empieza por `negocio` y `arquitectura`, que son las que más mejoran los resúmenes.

---

## 5. Cómo escribir un buen cuerpo

| Hazlo | Evítalo |
| --- | --- |
| Hechos concretos: nombres de apps, rutas, comandos | Frases genéricas ("código limpio y mantenible") |
| Viñetas cortas | Párrafos largos o copiar el README entero |
| Lo que **no** se debe romper ("las facturas emitidas no se recalculan") | Historia del proyecto |
| Comandos exactos (`make test`, `pytest idk/invoicing`) | "Correr los tests normalmente" |
| 800 a 1.800 caracteres por sección | Secciones de 5.000 caracteres que dejan fuera a las demás |

**Una prueba rápida:** si le pasaras la sección a una persona nueva en el equipo, ¿le ahorraría preguntas en su primer día? Si sí, sirve.

---

## 6. Revisar lo que recibe la IA

```sh
taskflow context list --project <slug>                # qué secciones hay, su rol, uso, tamaño y origen
taskflow context preview <id-tarea>                   # lo que recibe la IA al resumir esa tarea
taskflow context preview <id-tarea> --usage agent_prompt
```

- **`list`** valida todos los archivos. Un nombre o un campo mal escrito da un error que dice qué archivo falla y por qué.
- **`preview`** muestra las instrucciones, el conocimiento y las secciones **omitidas por presupuesto**. Si una sección importante queda afuera, súbele la prioridad, acorta otras o aumenta el presupuesto (`taskflow config set context.budget_chars 15000`).

---

## 7. Mantenerlas al día

El contexto envejece igual que la documentación. Revísalo cuando:

- Cambia el stack o la arquitectura (una migración, un servicio nuevo).
- Cambia el proceso (otra forma de desplegar, nuevas convenciones de ramas).
- Notas que los resúmenes de la IA asumen algo que ya no es cierto.

Un recordatorio útil: la fecha del último cambio de cada sección contra la actividad del repo.

```sh
ls -l "$TF/projects/<slug>/context"
git -C <ruta-del-repo> log --since="<fecha de la sección>" --oneline | wc -l
```

Con cientos de commits desde la última revisión, vale la pena regenerarlas.

---

## 8. Generarlas con un agente

Leer un repo y resumirlo es un buen trabajo para un agente de IA. Esta es la plantilla que se usó para IDK; reemplaza los `<...>`:

```text
Write Taskflow context sections for the <PROYECTO> project, in Spanish, by reading the
repository at <RUTA-DEL-REPO>.

Read: README, CLAUDE.md / AGENTS.md if present, docs/ (architecture, modules, glossary,
development, deploy), docker-compose.yml, Dockerfile, CI workflows, pre-commit config and
`git log --oneline -30`. Do NOT open .env or any file with credentials, never copy secrets
or customer data, and do not modify the repository. When docs and code disagree, trust
the code and report the discrepancy.

Write these files in <CARPETA DE CONFIG>/projects/<SLUG>/context/ :
10-negocio.md (knowledge; summary, plan, agent_prompt; 10), 15-estilo-codigo.md
(instruction; agent_prompt; 15), 20-arquitectura.md (knowledge; summary, agent_prompt; 20),
25-glosario.md (knowledge; summary, agent_prompt; 25), 30-desarrollo.md (knowledge;
agent_prompt; 30), 40-despliegue.md (knowledge; agent_prompt, playbook; 40).

Format: Markdown with TOML front matter between +++ lines (keys: title, role, usage,
priority, private). Bodies in Spanish, concrete facts, at most 1,800 characters each and
9,000 in total.

Verify with `taskflow context list --project <SLUG>`, then report each file with its size,
every docs-vs-code discrepancy, and anything uncertain.
```

**Revisa siempre lo que escribió el agente** antes de usarlo: es la base de todas las respuestas de la IA sobre ese proyecto. Corrige lo que no sea exacto y agrega lo que solo tú sabes (decisiones del cliente, zonas delicadas del código, acuerdos del equipo).
