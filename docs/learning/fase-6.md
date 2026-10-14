# Fase 6A: contexto por proyecto (sin IA todavía)

Objetivo del roadmap (§24): *"Contexto por proyecto + `adapter-anthropic` + `plan`"*. La fase está dividida en dos:

| Parte               | Qué trae                                                                                                                                                                                 |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **6A (esta)** | Las **secciones de contexto**: archivos Markdown que describen cada proyecto o dan instrucciones a la IA. Cómo se eligen para cada llamada, `context list` y `context preview` |
| 6B                  | La IA: el adaptador de Anthropic, el resumen en `show`, `plan` y el prompt para el agente. Usa todo lo de la 6A                                                                       |

Al terminar la 6A:

```sh
taskflow context list                       # secciones globales
taskflow context list --project acme        # las que aplican a ACME, con su origen
taskflow context preview 86b2x1abc          # exactamente lo que recibiría la IA para esa tarea
taskflow context preview 86b2x1abc --usage agent_prompt
```

Esta fase tiene **7 pasos**, todos con conceptos nuevos. Lo repetitivo de fases anteriores (`match` de texto a enum, conversiones campo por campo, recorrer una carpeta, tablas, comandos) ya está hecho. Ahora mismo fallan **27 tests** que dependen de esos 7 pasos. Los comprobé con una implementación de referencia que después borré.

---

## Qué es una sección de contexto

Un archivo Markdown con un encabezado TOML entre líneas `+++` (*front matter*):

```markdown
+++
title = "Modelo de negocio"
role = "knowledge"
usage = ["summary", "plan", "agent_prompt"]
priority = 10
+++

ACME vende software de facturación a pymes en Colombia y México.
- Cualquier cambio en el cálculo de impuestos requiere aprobación de su contador.
```

| Campo        | Valores                                               | Por defecto   | Para qué                                                                                                                                |
| ------------ | ----------------------------------------------------- | ------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `title`    | Texto                                                 | La clave      | Cómo se muestra                                                                                                                         |
| `role`     | `knowledge`, `instruction`                        | `knowledge` | **Conocimiento** describe el proyecto (va en el mensaje); **instrucción** dice cómo responder (va en el *system prompt*) |
| `usage`    | `summary`, `plan`, `agent_prompt`, `playbook` | Todos         | En qué llamadas a la IA se incluye                                                                                                      |
| `priority` | 0–255, menor = más importante                       | 100           | Orden, y quién entra primero si no cabe todo                                                                                            |
| `private`  | `true`, `false`                                   | `false`     | Nunca se envía a la IA (FR-39)                                                                                                          |

**Dónde viven:**

```text
<config_dir>/taskflow/
├── context/                        ← globales: aplican a todos los proyectos
│   ├── 90-aprendizaje-rust.md      ← clave "aprendizaje-rust"
│   └── estilo.md                   ← clave "estilo"
└── projects/acme/context/          ← de ACME
    ├── 10-negocio.md
    └── 20-codigo.md
```

- **La clave** es el nombre del archivo sin `.md` y sin el prefijo numérico: `10-negocio.md` → `negocio`. El prefijo solo ordena los archivos en el explorador.
- **Una sección del proyecto reemplaza a la global con la misma clave** (FR-36). Si existen `context/negocio.md` y `projects/acme/context/negocio.md`, las tareas de ACME usan la de ACME.

### Cómo se elige qué se envía

```text
globales ─┐
          ├─ merge (el proyecto reemplaza) ─► quitar privadas ─► filtrar por uso
proyecto ─┘                                                         │
                                       ordenar por (prioridad, clave)
                                                                    │
                          sumar caracteres: si cabe en el presupuesto → incluida
                                            si no                     → omitida (y se sigue)
                                                                    │
                        instrucciones → system prompt   ·   conocimiento → mensaje
```

El presupuesto es `context.budget_chars` (por defecto 12.000 caracteres) y existe para controlar el costo de cada llamada (FR-38).

---

## Mapa de cambios

| Capa          | Archivo                                         | Qué hay                                                                                                           |
| ------------- | ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Dominio       | `domain/src/context/mod.rs`                   | `SectionKey`, `SectionRole` (hecho), `Usage`, `ContextSection`, `ProjectContext` (`merge`, `select`) |
| Aplicación   | `application/src/ports/context_repository.rs` | Puerto `ContextRepository` (hecho)                                                                               |
|               | `application/src/settings/mod.rs`             | Sección `[context]`: `global_dir`, `budget_chars` (hecho)                                                   |
|               | `application/src/dto.rs`                      | `ContextBlock`, `BuiltContext`, `SectionView`, `ContextPreview`                                            |
|               | `application/src/sections/`                   | `BuildContext`, `ListSections`, `PreviewContext`                                                             |
| Archivos      | `fs/src/front_matter.rs`                      | `split`, `parse_section`                                                                                       |
|               | `fs/src/context.rs`                           | `FsContextRepository`                                                                                            |
| `bootstrap` | `build.rs`                                    | `context_dir`                                                                                                    |
| CLI           | `commands/context.rs`, `presenter.rs`       | `context list`, `context preview`                                                                              |

**Lo que ya hice yo** (pasos repetitivos, del mismo estilo que fases anteriores):

| Qué                                                                                                   | Archivo                          | Patrón que ya conoces de              |
| ------------------------------------------------------------------------------------------------------ | -------------------------------- | -------------------------------------- |
| `Display` y `FromStr` de `Usage` y `SectionRole`                                               | `domain/src/context/mod.rs`    | `LocationLevel` (fase 5)             |
| `ContextBlock` y `section_view` (campo por campo)                                                  | `dto.rs`, `list_sections.rs` | `ProjectDetail` (fase 5)             |
| `PreviewContext` (tarea → proyecto → contexto)                                                     | `sections/preview_context.rs`  | `InspectTask` (fase 4)               |
| `read_sections` (recorrer una carpeta)                                                               | `fs/src/context.rs`            | `FsProjectRepository::list` (fase 5) |
| `context_dir`                                                                                        | `bootstrap/src/build.rs`       | `projects_dir` (fase 5)              |
| `role_label`, `origin_label`, `sections`, `context_preview`                                    | `cli/src/presenter.rs`         | El presentador de las fases 4, 5 y 7   |
| `context::run`                                                                                       | `cli/src/commands/context.rs`  | `project::run` (fase 5)              |
| El puerto, su fake, `[context]` en la configuración, la conexión en `bootstrap` y los argumentos | Varios                           | Fases 4 y 5                            |

El módulo de casos de uso se llama `sections` y no `context`, porque `application::context` ya es el módulo de `AppContext`.

---

## Pasos

| # | Archivo                                       | Qué implementar                    | Concepto nuevo                                                                                             | Verificar con                                   |
| - | --------------------------------------------- | ----------------------------------- | ---------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| 1 | `domain/src/context/mod.rs`                 | `SectionKey::from_file_name`      | `strip_suffix`, `split_once` y guarda en un `match` sobre un `Option`                              | `cargo test -p domain context`                |
| 2 | (mismo)                                       | `ProjectContext::merge`           | `BTreeMap`: fusionar y ordenar a la vez                                                                  | (mismo)                                         |
| 3 | (mismo)                                       | `ProjectContext::select`          | Ordenar por una tupla; devolver préstamos (`ContextSelection<'a>`)                                      | (mismo)                                         |
| 4 | `application/src/sections/build_context.rs` | `BuildContext::execute`           | Usar un préstamo (`selection`) mientras vive su dueño (`context`)                                    | `cargo test -p application --lib sections`    |
| 5 | `application/src/sections/list_sections.rs` | `ListSections::execute`           | `HashSet` para responder "¿está?"                                                                      | (mismo)                                         |
| 6 | `fs/src/front_matter.rs`                    | `split`                           | Cortar texto sin copiar: `&str` prestados de la entrada                                                  | `cargo test -p adapter-fs --lib front_matter` |
| 7 | (mismo)                                       | `parse_section`                   | `Option` en `serde` para valores por defecto que dependen de otros datos; `collect` a un `EnumSet` | `cargo test -p adapter-fs --lib`              |
| 8 | —                                            | Crear tus secciones y previsualizar | —                                                                                                         | Ver "Paso 8"                                    |

El paso 3 desbloquea los tests de `BuildContext` y `PreviewContext`. El paso 7 desbloquea los de `FsContextRepository`, que usan `parse_section`.

Al terminar, **borra los bloques `#![allow(...)]` con `TODO(fase 6)`** en:

- `crates/domain/src/lib.rs`
- `crates/application/src/lib.rs`
- `crates/adapters/fs/src/lib.rs`
- `crates/bootstrap/src/lib.rs`
- `apps/cli/src/main.rs`

```sh
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

---

## Conceptos nuevos

### `EnumSet`: un conjunto de variantes en un número

```rust
#[derive(Debug, EnumSetType)]
pub enum Usage { Summary, Plan, AgentPrompt, Playbook }

let usage: EnumSet<Usage> = enum_set!(Usage::Summary | Usage::Plan);
usage.contains(Usage::Plan)     // true
EnumSet::<Usage>::all()         // las cuatro
```

Por dentro, un `EnumSet<Usage>` es un entero donde cada **bit** es una variante:

|                     | `Playbook` | `AgentPrompt` | `Plan` | `Summary` |
| ------------------- | ------------ | --------------- | -------- | ----------- |
| `{Summary, Plan}` | 0            | 0               | 1        | 1           |
| `all()`           | 1            | 1               | 1        | 1           |

Guardar, copiar y comparar un conjunto de usos cuesta lo mismo que un número: no hay `Vec` ni memoria en el heap. `contains` es una operación de bits.

- **Recorrerlo:** `usage.iter()` da las variantes en el orden en que están declaradas en el enum.
- **Construirlo desde un iterador:** `collect()` funciona igual que con un `Vec`. Con un iterador de `Result<Usage, _>`, también sirve `collect::<Result<EnumSet<Usage>, _>>()`, el mismo truco de la fase 5.

### `BTreeMap` para fusionar y ordenar a la vez

```rust
let mut by_key: BTreeMap<SectionKey, ContextSection> = global.into_iter().map(|s| (s.key.clone(), s)).collect();
by_key.extend(project.into_iter().map(|s| (s.key.clone(), s)));   // misma clave → reemplaza
by_key.into_values().collect()                                     // ordenado por clave
```

Un `BTreeMap` hace dos cosas que aquí necesitamos: **insertar una clave existente reemplaza el valor** (la regla FR-36) y **se recorre siempre ordenado** por clave. A diferencia de un `HashMap`, cuyo orden es impredecible, el resultado es determinista: dos ejecuciones dan el mismo contexto, en el mismo orden.

`into_values()` consume el mapa y devuelve solo los valores. Los `ContextSection` se **mueven** fuera, sin copiarse.

### Ordenar por varios criterios: una tupla como clave

```rust
candidates.sort_by_key(|section| (section.priority, &section.key));
```

Las tuplas se comparan **elemento por elemento**: primero `priority`, y solo si empatan, `key`. Es la regla "por prioridad, y a igual prioridad por clave" en una línea.

`&section.key` va como **referencia**: `sort_by_key` llama a la función muchas veces, y clonar la clave cada vez sería un desperdicio. Se puede devolver una referencia porque apunta dentro del elemento que se está ordenando.

### `ContextSelection<'a>`: elegir sin copiar

```rust
pub struct ContextSelection<'a> {
    pub included: Vec<&'a ContextSection>,
    pub omitted: Vec<&'a SectionKey>,
}

pub fn select(&self, usage: Usage, budget_chars: usize) -> ContextSelection<'_>
```

`select` devuelve **referencias** a las secciones que ya viven dentro del `ProjectContext`. Un cuerpo puede tener miles de caracteres, y elegir no debería copiarlos. El `'_` dice: "el resultado vive lo mismo que `&self`", así que el `ProjectContext` no se puede destruir mientras uses la selección.

Las copias se hacen una sola vez, al final, al convertir a `ContextBlock` (DTO) para mandarlo fuera de la capa de aplicación.

### Rebanadas de texto prestadas: `split`

```rust
pub(crate) fn split(text: &str) -> Result<(Option<&str>, &str), String>
```

El *front matter* y el cuerpo son **trozos del mismo `text`**: `&text[inicio..fin]`. No se reserva memoria. Con una sola referencia de entrada, Rust asume que las referencias de salida salen de ella (*elision*), así que no hace falta escribir lifetimes.

El test `split_borrows_from_the_text` comprueba que el cuerpo apunte **dentro** de `text`, comparando direcciones de memoria con `as_ptr()`.

Las herramientas para cortar texto:

| Método                   | Hace                                                  | Devuelve                 |
| ------------------------- | ----------------------------------------------------- | ------------------------ |
| `s.strip_prefix("+++")` | Si empieza con eso, el resto                          | `Option<&str>`         |
| `s.strip_suffix(".md")` | Si termina con eso, lo anterior                       | `Option<&str>`         |
| `s.split_once('-')`     | Corta en el **primer** `-`                    | `Option<(&str, &str)>` |
| `s.find("\n+++")`       | Posición en **bytes** de la primera aparición | `Option<usize>`        |
| `s.trim()`              | Sin espacios ni saltos al principio y al final        | `&str` (prestado)      |

`find` da bytes, y `&s[..i]` corta por bytes. Aquí es seguro porque `i` sale de `find` y apunta justo al inicio de `\n`, que es un carácter de un byte: nunca cae en medio de una letra.

### Privacidad por diseño

Las secciones privadas se descartan **dentro de `select`**, en el dominio, no en el adaptador de IA. Así es imposible que un futuro caso de uso las envíe por olvido: si usa `select`, ya vienen filtradas. `ListSections` sí las muestra, porque listar no envía nada a ninguna parte.

### `Arc<dyn Trait>`, `Send` y `Sync`: por qué `AppContext` es así

Esta fase agrega un campo más a `AppContext`, que existe desde la fase 2:

```rust
pub struct AppContext {
    pub tasks: Arc<dyn TaskProvider>,
    pub projects: Arc<dyn ProjectRepository>,
    pub context: Arc<dyn ContextRepository>,      // ← nuevo
    pub policy: Arc<dyn PrioritizationPolicy>,
    pub clock: Arc<dyn Clock>,
}

pub trait ContextRepository: Send + Sync { ... }
```

Cada pieza responde una pregunta distinta.

**1. `dyn ContextRepository`: "cualquier tipo que implemente el trait".** El caso de uso no sabe si las secciones vienen de archivos (`FsContextRepository`) o de memoria (`FakeContextRepository`, en los tests). `dyn Trait` es un **objeto de trait**: un valor de un tipo que se decide **en ejecución**, del que solo se sabe que implementa el trait. Cada llamada a un método pasa por una tabla de punteros a funciones (la *vtable*), así que se elige la implementación correcta en el momento.

La alternativa sería un genérico: `struct AppContext<T: TaskProvider, P: ProjectRepository, C: ContextRepository, ...>`. Funciona y es un poco más rápido (se resuelve al compilar), pero cada caso de uso tendría que arrastrar esos cinco parámetros, y `bootstrap` no podría elegir el adaptador leyendo `config.toml` en ejecución. El costo de `dyn` (una indirección por llamada) es despreciable al lado de una petición HTTP.

**2. ¿Por qué no `dyn ContextRepository` solo?** Porque su tamaño no se conoce al compilar: puede ser un `FsContextRepository` (dos rutas) o un fake con mapas. Rust necesita saber el tamaño de cada campo de un struct, así que un `dyn Trait` siempre va **detrás de un puntero**: `&dyn`, `Box<dyn>` o `Arc<dyn>`.

**3. ¿Por qué `Arc` y no `Box`?**

| Puntero        | Dueños                                                 | Clonar                                   | Entre hilos                                        |
| -------------- | ------------------------------------------------------- | ---------------------------------------- | -------------------------------------------------- |
| `Box<dyn T>` | **Uno**                                           | Imposible sin clonar el valor de adentro | Se puede mover, no compartir                       |
| `Rc<dyn T>`  | Varios (cuenta referencias)                             | Barato: suma 1 a un contador             | **No**: el contador no es seguro entre hilos |
| `Arc<dyn T>` | Varios (cuenta referencias de forma **atómica**) | Barato                                   | **Sí**                                      |

`AppContext` es `Clone` y está pensado para compartirse: en una app de escritorio (Tauri) o un servidor web, cada petición puede ejecutarse en un hilo distinto, y todas usan el mismo contexto. Con `Arc`, clonar `AppContext` solo suma 1 a cinco contadores: nadie copia el cliente HTTP ni el semáforo. Cuando se destruye el último clon, el contador llega a 0 y el adaptador se libera.

"Atómico" significa que el contador se suma y resta con instrucciones del procesador que no pueden quedar a medias aunque dos hilos lo toquen a la vez. Por eso `Arc` es un poco más lento que `Rc`, y por eso existen los dos.

**4. `Send` y `Sync`: sí, es sobre hilos.** Son dos *marker traits*: no tienen métodos, solo afirman algo del tipo.

| Trait    | Significa                                                                                       | Ejemplo que **no** lo cumple                                                         |
| -------- | ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `Send` | Un valor de este tipo se puede **mover** a otro hilo                                      | `Rc<T>`: si dos hilos tuvieran clones, sumarían al contador a la vez y lo corromperían |
| `Sync` | Un valor de este tipo se puede **compartir por referencia** (`&T`) entre hilos a la vez | `RefCell<T>`: permite modificar a través de `&T` sin protección                      |

El compilador los calcula solo: un struct es `Send` si todos sus campos lo son. Y aquí está la clave: **con `dyn Trait`, el compilador ya no ve el tipo real**, así que no puede comprobarlo. Por eso el trait lo exige explícitamente:

```rust
pub trait ContextRepository: Send + Sync { ... }
//                           ^^^^^^^^^^^ "cualquiera que implemente esto debe poder
//                                        moverse y compartirse entre hilos"
```

Con eso, `Arc<dyn ContextRepository>` es `Send + Sync`, `AppContext` también, y se puede usar desde cualquier hilo. Si algún día alguien escribe un repositorio con un `Rc` adentro, el error aparece **al implementar el trait**, no en un lugar lejano donde se comparte el contexto.

**¿Y ahora mismo, con un solo programa de terminal?** También importa. `#[tokio::main]` usa por defecto un *runtime* con varios hilos, y una tarea puede **pasar de un hilo a otro** cada vez que se pausa en un `.await` (tokio reparte el trabajo entre los hilos libres). Todo lo que la tarea guarda entre un `.await` y el siguiente viaja con ella, así que tiene que ser `Send`. Por eso `#[async_trait]` exige que los futures de sus métodos sean `Send`.

Ojo: `try_join_all` (fase 2) **no** reparte los futures entre hilos. Los avanza a todos dentro de la misma tarea, intercalándolos en cada `.await`: es concurrencia, no paralelismo. Lo que puede cambiar de hilo es la tarea entera.

**Por qué `FakeTaskProvider` usa `Mutex` y no `RefCell`:** necesita modificar su registro (`writes`, `queries`) a través de `&self`, y tiene que ser `Sync`. `Mutex` permite modificar a través de `&self` **y** es `Sync`, porque solo deja entrar a un hilo a la vez. `RefCell` haría lo primero, pero no es `Sync`, y el trait no compilaría.

---

## Especificación por paso

### Paso 1: `SectionKey::from_file_name` (`domain/src/context/mod.rs`)

| # | Qué                                                             | Si falla                       |
| - | ---------------------------------------------------------------- | ------------------------------ |
| 1 | `file_name.strip_suffix(".md")`                                | No termina en `.md` → error |
| 2 | Si tiene prefijo numérico, quitarlo (tabla de abajo)            | —                             |
| 3 | `lo_que_queda.parse::<SectionKey>()` (el `FromStr` ya hecho) | Inválido → error             |

**El error** es siempre `DomainError::InvalidSectionKey(file_name.to_owned())`, con el nombre **original** del archivo, no el recortado: es lo que el usuario reconoce. Una closure evita repetirlo: `let invalid = || DomainError::InvalidSectionKey(file_name.to_owned());`.

**El prefijo**, con `split_once('-')` y un `match` con guarda:

```rust
let key = match stem.split_once('-') {
    Some((prefix, rest)) if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) => rest,
    _ => stem,
};
```

| Archivo                    | `stem`                | `split_once('-')`            | ¿Prefijo?              | Clave                |
| -------------------------- | ----------------------- | ------------------------------ | ----------------------- | -------------------- |
| `10-negocio.md`          | `10-negocio`          | `("10", "negocio")`          | Sí                     | `negocio`          |
| `negocio.md`             | `negocio`             | `None`                       | No                      | `negocio`          |
| `90-aprendizaje-rust.md` | `90-aprendizaje-rust` | `("90", "aprendizaje-rust")` | Sí                     | `aprendizaje-rust` |
| `v2-api.md`              | `v2-api`              | `("v2", "api")`              | No: `v` no es dígito | `v2-api`           |
| `10-.md`                 | `10-`                 | `("10", "")`                 | Sí                     | `""` → error      |
| `negocio.txt`            | —                      | —                             | —                      | error en el punto 1  |

Para mapear el error del punto 3: `key.parse().map_err(|_| invalid())`.

### Paso 2: `ProjectContext::merge` (mismo archivo)

```rust
let mut by_key: BTreeMap<SectionKey, ContextSection> =
    global.into_iter().map(|section| (section.key.clone(), section)).collect();
by_key.extend(project.into_iter().map(|section| (section.key.clone(), section)));
Self { sections: by_key.into_values().collect() }
```

- **Primero las globales, después las del proyecto:** `extend` reemplaza las claves repetidas, así que gana el proyecto. Si invirtieras el orden, ganarían las globales y fallaría `project_sections_replace_global_ones_and_stay_sorted`.
- `section.key.clone()` porque la clave va dos veces: como clave del mapa y dentro de la sección.

### Paso 3: `ProjectContext::select` (mismo archivo)

| #    | Qué                                 | Código clave                                                                                  |
| ---- | ------------------------------------ | ---------------------------------------------------------------------------------------------- |
| 1–2 | Candidatas: no privadas y con el uso | `self.sections.iter().filter(\|s\| !s.private && s.usage.contains(usage)).collect::<Vec<_>>()` |
| 3    | Ordenar                              | `candidates.sort_by_key(\|s\| (s.priority, &s.key))`                                           |
| 4    | Presupuesto                          | Bucle de abajo                                                                                 |

```rust
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
```

Detalles que comprueban los tests:

| Test                                            | Comprueba                                                                                                                            |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `select_respects_the_budget_and_keeps_trying` | Con presupuesto 10 y cuerpos de 6, 9 y 4: entran la de 6 y la de 4; la de 9 se omite **y se sigue probando**. No hay `break` |
| `budget_counts_characters_not_bytes`          | `"ñññññ"` son 5 caracteres (10 bytes): cabe en 5. Por eso `chars().count()` y no `len()`                                  |
| `select_orders_by_priority_then_key`          | Prioridades 20, 10, 20 con claves b, c, a → c, a, b                                                                                 |

### Paso 4: `BuildContext::execute(request)` (`application/src/sections/build_context.rs`)

| # | Qué                 | Código clave                                                                                                                        |
| - | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| 1 | Globales             | `let global = self.ctx.context.global_sections().await?;`                                                                          |
| 2 | Del proyecto, si hay | `match &request.project { Some(slug) => self.ctx.context.project_sections(slug).await?, None => Vec::new() }`                      |
| 3 | Fusionar y elegir    | `let context = ProjectContext::merge(global, project);` y `let selection = context.select(request.usage, request.budget_chars);` |
| 4 | Separar por rol      | Bucle de abajo                                                                                                                       |
| 5 | Omitidas             | `built.omitted = selection.omitted.iter().map(ToString::to_string).collect();`                                                     |
| 6 | Devolver             | `Ok(built)`                                                                                                                        |

```rust
let mut built = BuiltContext::default();
for section in selection.included {
    let block = ContextBlock::from(section);
    match section.role {
        SectionRole::Instruction => built.instructions.push(block),
        SectionRole::Knowledge => built.knowledge.push(block),
    }
}
```

`selection` toma prestado `context`: los dos deben vivir hasta el final del bucle. Como son variables locales de la misma función, eso pasa solo.

### Paso 5: `ListSections::execute` (`application/src/sections/list_sections.rs`)

| # | Qué                                                                                 | Por qué                                                                                                                              |
| - | ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------- |
| 1 | Si hay `project`: `self.ctx.projects.get(slug).await?;` (el resultado no se usa) | Un slug mal escrito debe dar `NotFound`, no una lista con solo las globales                                                         |
| 2 | Globales y, si hay proyecto, las del proyecto                                        | Igual que en el paso 4                                                                                                                |
| 3 | Las claves de cada grupo **antes** de fusionar                                 | `merge` consume los `Vec`. Guarda las claves en dos `HashSet`: `global.iter().map(\|s\| s.key.clone()).collect::<HashSet<_>>()` |
| 4 | `ProjectContext::merge(global, project_sections)`                                  | Ordenadas por clave, privadas incluidas                                                                                               |
| 5 | Cada sección → `section_view(section, origin)`                                   | Con el origen de la tabla de abajo                                                                                                    |

| ¿Clave en el proyecto? | ¿Clave en las globales? | `origin`     |
| ----------------------- | ------------------------ | -------------- |
| Sí                     | Sí                      | `"override"` |
| Sí                     | No                       | `"project"`  |
| No                      | —                       | `"global"`   |

Con un `match` sobre la tupla de dos `bool`:

```rust
let origin = match (project_keys.contains(&section.key), global_keys.contains(&section.key)) {
    (true, true) => "override",
    (true, false) => "project",
    (false, _) => "global",
};
```

`section_view` ya está hecha: llámala con cada sección y su origen.

### Paso 6: `split` (`fs/src/front_matter.rs`)

| # | Qué                                                                                                                                                                     |
| - | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1 | `let Some(rest) = text.strip_prefix("+++") else { return Ok((None, text.trim())) };`                                                                                   |
| 2 | Después de `+++` tiene que venir un salto de línea: `rest.strip_prefix('\n')`. Si no, es un texto que casualmente empieza con `+++`: `Ok((None, text.trim()))` |
| 3 | Buscar el cierre: `rest.find("\n+++")`. Si no está → `Err("front matter is not closed with +++".to_owned())`                                                       |
| 4 | `front = &rest[..end]` y `body = &rest[end + "\n+++".len()..]`                                                                                                       |
| 5 | `Ok((Some(front), body.trim()))`                                                                                                                                       |

Si quieres aceptar archivos con saltos de línea de Windows (`\r\n`), quita un `\r` opcional antes del punto 2: `let rest = rest.trim_start_matches('\r');`.

### Paso 7: `parse_section` (mismo archivo)

Los errores son `String`: convierte cada error con `.map_err(|e| e.to_string())`.

| # | Qué                | Código clave                                                                                                                                                   |
| - | ------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1 | La clave            | `SectionKey::from_file_name(file_name).map_err(\|e\| e.to_string())?`                                                                                           |
| 2 | Separar             | `let (front, body) = split(text)?;`                                                                                                                           |
| 3 | El *front matter* | `match front { Some(raw) => toml::from_str::<SectionFile>(raw).map_err(...)?, None => SectionFile::default() }`                                               |
| 4 | `role`            | `match file.role { Some(raw) => raw.parse().map_err(...)?, None => SectionRole::Knowledge }`                                                                  |
| 5 | `usage`           | `match file.usage { Some(list) => list.iter().map(\|raw\| raw.parse::<Usage>()).collect::<Result<EnumSet<Usage>, _>>().map_err(...)?, None => EnumSet::all() }` |
| 6 | El resto            | Tabla de abajo                                                                                                                                                  |

| Campo        | Valor                                                                              |
| ------------ | ---------------------------------------------------------------------------------- |
| `title`    | `file.title.unwrap_or_else(\|\| key.to_string())`                                  |
| `priority` | `file.priority.unwrap_or(DEFAULT_PRIORITY)`                                      |
| `private`  | `file.private`                                                                   |
| `body`     | `body.to_owned()` (aquí sí se copia: `ContextSection` es dueño de su texto) |

Escribe `title` **antes** de `key` en el struct literal, o calcula el título en una variable antes. `key.to_string()` lee `key`, y si `key` ya se movió al campo `key`, no compila.

En el punto 4, `raw.parse()` no sabe de qué tipo es su error hasta que lo usas, y `.map_err(|e| e.to_string())` no se lo dice. Anótalo en la closure: `.map_err(|e: DomainError| e.to_string())`. En el punto 5 no hace falta, porque el *turbofish* `parse::<Usage>()` ya fija el tipo.

**Por qué `Option` en `SectionFile` y no `#[serde(default)]`:** con `Option` se distingue "no lo escribió" (`None`) de "escribió el valor por defecto". El título por defecto depende de la clave, que `serde` no conoce, así que se resuelve después.

---

## Paso 8: crear tus secciones

> Para el uso del día a día (qué secciones escribir en cada proyecto, cómo mantenerlas y cómo generarlas con un agente) está la guía [`docs/guias/contexto-por-proyecto.md`](../guias/contexto-por-proyecto.md).


No hay un comando para crearlas todavía (FR-40 lo prevé como mejora): se escriben a mano. `taskflow config path` dice dónde está tu `config.toml`, y las carpetas van junto a él.

1. **Una sección global de aprendizaje.** Va en `<config_dir>/context/90-aprendizaje-rust.md`:

   ```markdown
   +++
   title = "Aprendizaje: Rust"
   role = "instruction"
   usage = ["summary", "agent_prompt"]
   priority = 50
   +++

   Estoy aprendiendo Rust. Cuando propongas una solución en Rust, explica en 2–3 líneas
   por qué elegiste ese diseño y agrega al final un ejercicio corto para practicar.
   ```
2. **El negocio de uno de tus proyectos.** Va en `<config_dir>/projects/<slug>/context/10-negocio.md`:

   ```markdown
   +++
   title = "Modelo de negocio"
   priority = 10
   +++

   Qué hace el cliente, quién usa el sistema y qué no se puede romper.
   ```
3. Revisa y previsualiza:

   ```sh
   cargo run -p taskflow -- context list
   cargo run -p taskflow -- context list --project <slug>
   cargo run -p taskflow -- context preview <id de una tarea de ese proyecto>
   cargo run -p taskflow -- context preview <id> --usage plan     # la de aprendizaje no aparece
   ```
4. Prueba una sección con `private = true` y comprueba que aparece en `list` pero **no** en `preview`.

---

## Limitaciones conocidas

- **Sin `context new` ni `context edit`** (FR-40): las secciones se crean a mano. Las plantillas pueden llegar más adelante.
- **Sin contexto dentro del repositorio** (`<repo>/.taskflow/context/`): sigue siendo una decisión abierta (§26).
- **El presupuesto cuenta el cuerpo, no el título.** Es una aproximación del tamaño: la IA cobra por *tokens*, no por caracteres.

---

## Qué observar mientras implementas

- **No hay IA en ninguna parte.** Toda la lógica de "qué se envía" se prueba sin red y sin API key. En la 6B, el adaptador de Anthropic solo recibirá un `BuiltContext` ya armado.
- **La selección es pura.** `ProjectContext::select` no lee archivos ni la hora: dadas las mismas secciones, da siempre el mismo resultado. Por eso se prueba con datos en memoria.
- **Las copias se hacen una vez, al final.** Dominio: referencias (`ContextSelection<'a>`). Aplicación: `ContextBlock`, ya con el texto propio, para cruzar al exterior.

Cuando termines, avísame y reviso. Después sigue la 6B, con la IA.

---

# Conceptos aplicados

La fase 6A quedó completa. Los pasos 1 a 3 los implementaste tú; los pasos 4 a 7 y la corrección del paso 1 los hice yo.

## Paso 1: no olvidar el último paso de una validación

La primera versión cortaba bien la extensión y el prefijo, pero terminaba así:

```rust
Ok(Self(key.to_string()))                // ❌ acepta cualquier texto como clave
key.parse().map_err(|_| invalid())       // ✅ valida con las reglas de SectionKey
```

Sin el `parse`, `Mal Nombre.md` se aceptaba con la clave `Mal Nombre`: mayúsculas y un espacio, justo lo que las reglas prohíben. Lo detectó el test `invalid_file_names_are_rejected`. Ahora `context list` responde:

```text
Error: invalid data: .../context/Mal Nombre.md: invalid context section `Mal Nombre.md`: use `<key>.md` or `<number>-<key>.md`, ...
```

**La regla de fondo** (*parse, don't validate*, desde la fase 1): un constructor que recibe texto y devuelve un tipo validado tiene que pasar **siempre** por la validación. Construir el tipo directamente (`Self(...)`) se salta la garantía que el tipo promete.

Un detalle menor: `split_once("-")` pasó a `split_once('-')`. Para buscar un solo carácter, un `char` es más directo que un `&str`, y clippy lo marca (`single_char_pattern`).

## Pasos 2 y 3: ✅ como en la especificación

- **`merge`:** `BTreeMap` con `collect` y después `extend`, así que el proyecto gana y todo queda ordenado por clave.
- **`select`:** `filter` → `collect` → `sort_by_key` con la tupla `(priority, &key)` → el bucle del presupuesto **sin `break`**, para que una sección más corta todavía pueda entrar.

## Paso 4: `BuildContext`, un préstamo y su dueño en la misma función

```rust
let context = ProjectContext::merge(global, project);     // dueño de las secciones
let selection = context.select(request.usage, request.budget_chars);   // préstamos de `context`

for section in selection.included {                       // `section` es &ContextSection
    let block = ContextBlock::from(section);              // la copia, una sola vez, al final
    match section.role { ... }
}
```

`selection` contiene referencias **dentro** de `context`, así que `context` tiene que vivir más que `selection`. Como los dos son variables locales de la misma función y `context` se declara primero, el compilador lo comprueba solo. Si intentaras devolver `selection` desde `execute`, no compilaría: `context` se destruye al terminar la función.

Por eso el caso de uso devuelve `BuiltContext`, con `ContextBlock` **propios** (texto copiado). Las referencias sirven dentro de una función; para cruzar a otra capa hacen falta valores con dueño.

## Paso 5: `ListSections` y guardar datos antes de que se consuman

```rust
let global_keys: HashSet<SectionKey> = global.iter().map(|s| s.key.clone()).collect();
let project_keys: HashSet<SectionKey> = project_sections.iter().map(|s| s.key.clone()).collect();

let context = ProjectContext::merge(global, project_sections);   // consume los dos Vec
```

`merge` recibe los `Vec` **por valor** y los consume. Después de llamarlo, `global` y `project_sections` ya no existen, y con ellos la información de qué sección venía de dónde. Por eso las claves se guardan **antes**.

- **`HashSet` y no `Vec`:** solo hay que responder "¿esta clave está?". `contains` en un `HashSet` cuesta lo mismo con 3 claves que con 3.000; en un `Vec` habría que recorrerlo entero.
- **El origen**, con un `match` sobre la tupla de dos `bool`. El `_` en `(false, _)` dice "si no está en el proyecto, da igual lo demás".
- **`self.ctx.projects.get(slug).await?;`** sin usar el resultado: solo importa que **falle** si el proyecto no existe. Sin esa línea, `--project acmee` (mal escrito) mostraría las globales sin avisar.

## Paso 6: `split`, cortar texto sin copiar

```rust
let Some(rest) = text.strip_prefix("+++") else { return Ok((None, text.trim())); };
let rest = rest.trim_start_matches('\r');
let Some(rest) = rest.strip_prefix('\n') else { return Ok((None, text.trim())); };
let end = rest.find("\n+++").ok_or_else(|| "front matter is not closed with +++".to_owned())?;
Ok((Some(&rest[..end]), rest[end + "\n+++".len()..].trim()))
```

- **Todo es `&str` prestado de `text`.** `strip_prefix`, `trim` y `&rest[..end]` devuelven trozos del mismo texto, sin reservar memoria. El test `split_borrows_from_the_text` lo comprueba comparando direcciones con `as_ptr()`.
- **`let ... else`:** si el patrón no encaja, ejecuta el `else`, que debe salir de la función (`return`, `?`, `continue`...). Evita anidar `if let` para el caso feliz.
- **Cortar por bytes es seguro aquí.** `find` devuelve una posición en bytes y `&rest[..end]` corta por bytes. Como `end` apunta justo a un `\n`, que ocupa un byte, el corte nunca cae en medio de una letra. Con `&texto[..10]` sobre un texto arbitrario sí podría (fase 4, `truncate`).
- **`trim_start_matches('\r')`** acepta archivos guardados en Windows, donde las líneas terminan en `\r\n`.

## Paso 7: `parse_section`, valores por defecto que dependen de otros datos

```rust
title: file.title.unwrap_or_else(|| key.to_string()),
key,
```

- **`Option` en `SectionFile` en vez de `#[serde(default)]`.** El título por defecto es la **clave**, que sale del nombre del archivo; `serde` solo ve el TOML. Con `Option`, `serde` dice "no vino" (`None`) y el valor se calcula después, con todos los datos.
- **El orden de los campos importa.** `title` lee `key` (`key.to_string()`), así que va **antes** de `key`, que mueve la clave al struct. Al revés, el compilador diría "borrow of moved value: `key`".
- **`collect::<Result<EnumSet<Usage>, _>>()`:** el truco de la fase 5, pero construyendo un `EnumSet` en lugar de un `Vec`. `collect` funciona con cualquier tipo que implemente `FromIterator`, y `EnumSet` lo implementa.
- **`.map_err(|e: DomainError| e.to_string())`** en el `role`: `raw.parse()` no sabe a qué tipo parsear hasta que el resultado se usa, y la closure no da pistas sobre el error. Anotar el tipo del error resuelve la ambigüedad. En `usage` no hace falta, porque `parse::<Usage>()` ya fija el tipo.

## Al cerrar la fase

- Se borraron los cinco bloques `#![allow(...)]`.
- `cargo test --workspace`: 281 tests. Clippy y `cargo fmt` sin avisos.
- Falta el **paso 8**: crear tus secciones reales y probar `context preview` con una tarea de tu ClickUp.
