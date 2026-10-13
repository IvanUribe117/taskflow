# Fase 4: configuración persistente, `bootstrap` y CLI

Objetivo del roadmap (§24): *"`config init` interactivo deja todo listo; `taskflow today` reemplaza la revisión manual"*.

Al terminar tendrás un programa de verdad, `taskflow`, en lugar del ejemplo de la fase 3:

```sh
taskflow config init      # asistente: proveedor, token (al llavero) y workspace
taskflow config check     # ✔ Configuración válida · conectado como usuario 183
taskflow today            # tus tareas priorizadas
taskflow show 86b2x1abc   # detalle de una tarea con sus comentarios
taskflow config set policy.overdue_bonus 80
```

Las firmas, los tipos y los tests ya están escritos: son **78 tests nuevos**, y ahora mismo **71 fallan a propósito**. Los 7 que ya pasan prueban cosas que no dependen de tu código, como la definición de `clap` o el `deny_unknown_fields` de `serde`. Los comprobé todos con una implementación de referencia que después borré: si uno falla, el problema está en la implementación, no en el test.

Esta fase es más grande que las anteriores porque toca **cuatro capas**. Está dividida en cuatro partes; cada una se puede terminar y probar por separado.

---

## Mapa de crates

```text
apps/cli  (bin "taskflow")          ← NUEVO: argumentos, asistente, presentación
    │
    ▼
crates/bootstrap                    ← NUEVO: registro de proveedores, arma el AppContext
    │
    ├──────────────┬────────────────┬──────────────────┐
    ▼              ▼                ▼                  ▼
adapter-clickup  adapter-fs       adapter-keyring    application
(fase 3)         ← NUEVO:         ← NUEVO:           + settings (AppSettings, ConfigKey)
                 config.toml      entorno + llavero  + ports ConfigStore, SecretStore
                                                     + InspectTask
```

**Regla de oro:** las flechas solo bajan. `application` no conoce a nadie de arriba; `bootstrap` es el único que conoce a todos los adaptadores; el CLI solo habla con `bootstrap` y `application`.

| Archivo | Qué hay |
| --- | --- |
| `crates/application/src/settings/mod.rs` | `AppSettings` y sus secciones: el contenido tipado de `config.toml` |
| `crates/application/src/settings/key.rs` | `ConfigKey`: una clave válida de `config get/set` |
| `crates/application/src/settings/error.rs` | `ConfigError`, `SettingsIssue` (ya hechos) |
| `crates/application/src/ports/config_store.rs` | Puerto `ConfigStore` (síncrono) |
| `crates/application/src/ports/secret_store.rs` | Puerto `SecretStore` y el enum `SecretName` |
| `crates/application/src/tasks/inspect_task.rs` | UC-03 `InspectTask`, sin IA todavía |
| `crates/adapters/fs/src/` | `atomic.rs`, `edit.rs`, `config_store.rs` |
| `crates/adapters/keyring/src/` | `env.rs`, `keychain.rs`, `layered.rs` |
| `crates/bootstrap/src/` | `registry.rs`, `clickup.rs` (descriptor), `build.rs` |
| `apps/cli/src/` | `main.rs`, `args.rs`, `presenter.rs`, `commands/`, `wizard.rs` |

---

## Pasos

| #  | Archivo | Qué implementar | Verificar con | Conceptos |
| -- | ------- | --------------- | ------------- | --------- |
| **A** | | **Configuración en `application`** | | |
| 1  | `settings/mod.rs` | `AppSettings::new`, `ProviderKind::as_str`, `PolicySettings::default`, `From<&PolicySettings>` | `cargo test -p application --lib settings::tests` | Atributos de `serde`, una sola fuente de verdad |
| 2  | `settings/mod.rs` | `AppSettings::validate` | (mismo comando) | Validar en dos fases, acumular errores, `HashSet` |
| 3  | `settings/key.rs` | `ConfigKey::segments`, `FromStr` | `cargo test -p application --lib settings::key` | Parse don't validate, `&'static str` |
| 4  | `ports/secret_store.rs` | `SecretName::key`, `env_var` | `cargo test -p application --lib secret_store` | Enums en vez de strings |
| 5  | `dto.rs`, `tasks/inspect_task.rs` | `CommentView::new`, `TaskDetail::new`, `InspectTask::execute` | `cargo test -p application --lib inspect` y `--lib dto` | `futures::try_join!` |
| **B** | | **Adaptadores** | | |
| 6  | `fs/src/atomic.rs` | `write` | `cargo test -p adapter-fs --lib atomic` | Escritura atómica, `tempfile`, RAII |
| 7  | `fs/src/edit.rs` | `lookup`, `get`, `parse_value`, `set` | `cargo test -p adapter-fs --lib edit` | `toml_edit`, decoración, lifetimes elididos |
| 8  | `fs/src/config_store.rs` | `default_path`, `read`, `parse`, `render`, `load`, `apply`, `get_raw` | `cargo test -p adapter-fs --lib config_store` | `toml` contra `toml_edit`, `io::ErrorKind` |
| 9  | `keyring/src/` | `EnvSecretStore`, `LayeredSecretStore`, `KeyringSecretStore` | `cargo test -p adapter-keyring` | Composite, punteros a función, `secrecy` |
| **C** | | **Raíz de composición** | | |
| 10 | `bootstrap/src/registry.rs`, `clickup.rs` | `find`, `provider_settings`, `build`, `clickup_config`, `status_map` | `cargo test -p bootstrap` | Registro, descriptores, `const` |
| 11 | `bootstrap/src/build.rs` | `config_store`, `secret_store`, `build` | `cargo test -p bootstrap` | Raíz de composición, objeto nulo |
| **D** | | **CLI** | | |
| 12 | `cli/src/args.rs` | `parse_task_id` | `cargo test -p taskflow args` | `clap` derive, `value_parser` |
| 13 | `cli/src/presenter.rs` | `due_label`, `today`, `task_detail`, `json` | `cargo test -p taskflow presenter` | Presentación pura, `comfy-table` |
| 14 | `cli/src/commands/`, `wizard.rs` | `today::run`, `show::run`, `config::run`, `config_init`, `ask` | A mano (ver abajo) | Pegamento, `dialoguer` |
| 15 | — | Probar contra tu ClickUp real | `cargo run -p taskflow -- today` | Llavero del sistema |

Durante los pasos 1 y 2, `settings::tests` mostrará fallos de `validate` hasta que termines el paso 2. Es normal.

Al terminar, **borra el bloque `#![allow(...)]` marcado con `TODO(fase 4)`** en estos cinco archivos y comprueba que todo sigue limpio:

- `crates/application/src/lib.rs`
- `crates/adapters/fs/src/lib.rs`
- `crates/adapters/keyring/src/lib.rs`
- `crates/bootstrap/src/lib.rs`
- `apps/cli/src/main.rs`

```sh
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Este bloque incluye `unused_imports`: algunos archivos ya traen los `use` que vas a necesitar, como pista.

---

## Antes de empezar: conceptos nuevos de esta fase

### Dos crates para TOML: `toml` y `toml_edit`

| | `toml` | `toml_edit` |
| --- | --- | --- |
| Para qué | Texto ↔ structs (`serde`) | Editar el **documento** tal como está escrito |
| Conserva comentarios | No | Sí, y también el orden y los espacios |
| Valida tipos | Sí (`priority_weight = "alto"` es error) | No: es solo un árbol de texto |
| Aquí se usa para | Validar (`parse`) y escribir el archivo inicial (`render`) | `config set` y `config get` |

Se usan juntos (§13.2): `config set` edita con `toml_edit` y, **antes de escribir**, valida el documento resultante con `toml`. Si la validación falla, no se escribe nada.

### Atributos de `serde` para configuración

```rust
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]          // clave desconocida → error (atrapa errores de tipeo)
pub struct AppSettings {
    pub version: u32,                  // obligatoria
    #[serde(default)]                  // si falta la sección, usa PolicySettings::default()
    pub policy: PolicySettings,
}

#[serde(default, deny_unknown_fields)] // a nivel de struct: cada campo que falte sale de Default
pub struct PolicySettings { ... }

#[serde(rename_all = "lowercase")]     // ProviderKind::ClickUp ↔ "clickup"
pub enum ProviderKind { ClickUp }

#[serde(default, skip_serializing_if = "Option::is_none")]  // None no se escribe en el archivo
pub clickup: Option<ClickUpSettings>,
```

En la fase 3, `serde` **ignoraba** los campos desconocidos del JSON de ClickUp a propósito: no controlamos esa API. Aquí es al revés: el archivo lo escribe el usuario, y `priorty_weight = 5` (mal escrito) debe ser un error, no un valor que se ignora en silencio (FR-22).

### Validar en dos fases

1. **Forma** (`serde`): tipos y nombres. `team_id = 9001` sin comillas falla porque se esperaba texto.
2. **Reglas** (`AppSettings::validate`): lo que el tipo no puede expresar. "Si el proveedor es ClickUp, la sección `[provider.clickup]` debe existir"; "`team_id` solo tiene dígitos"; "un nombre de estado no puede estar en dos listas".

`validate` junta **todos** los problemas en un `Vec<SettingsIssue>` en lugar de cortar en el primero: el usuario los corrige de una vez.

### Escritura atómica

Si `taskflow` se cae a mitad de `fs::write`, el archivo puede quedar **a medias** y la configuración, corrupta. La técnica estándar:

```text
config.toml          ← intacto todo el tiempo
.tmpXa91b            ← 1. escribir aquí el contenido nuevo completo
                       2. sync_all: asegurarse de que llegó al disco
config.toml          ← 3. rename(.tmpXa91b → config.toml): atómico
```

Renombrar dentro de un mismo sistema de archivos es **atómico**: quien lea `config.toml` ve el archivo viejo o el nuevo, nunca una mezcla. Por eso el temporal se crea **en el mismo directorio** (`NamedTempFile::new_in(dir)`): entre discos distintos, `rename` no es atómico (en realidad es copiar y borrar).

`NamedTempFile` usa RAII, como el permiso del semáforo: si algo falla antes de `persist`, el temporal se borra solo al destruirse.

### Secretos: `secrecy::SecretString`

```rust
let token = SecretString::from("pk_123".to_owned());
println!("{token:?}");                  // SecretBox<str>([REDACTED])
let real: &str = token.expose_secret(); // leerlo exige una llamada explícita
```

Es la versión "de biblioteca" del `Debug` a mano de `ClickUpConfig`: el valor no aparece en logs ni en panics, y cada lugar que lo lee queda marcado con `expose_secret()`, fácil de buscar y revisar.

### Por qué `EnvSecretStore` recibe una función

En Rust 2024, `std::env::set_var` es `unsafe`: cambiar las variables de entorno mientras otro hilo las lee es comportamiento indefinido. Este workspace prohíbe `unsafe` (`unsafe_code = "forbid"`), así que los tests **no pueden** preparar el entorno con `set_var`.

La solución es inyectar la forma de leer:

```rust
type Lookup = fn(&str) -> Option<String>;

EnvSecretStore::new()                    // usa |name| std::env::var(name).ok()
EnvSecretStore::with_lookup(|name| ...)  // en tests: un entorno simulado
```

`fn(&str) -> Option<String>` es un **puntero a función**: una closure que no captura nada se convierte en uno automáticamente. Es la inyección de dependencias más pequeña posible: no hace falta un trait para una sola operación.

### El patrón Composite: `LayeredSecretStore`

```text
LayeredSecretStore  (es un SecretStore)
 ├── EnvSecretStore       ← gana si TASKFLOW_CLICKUP_TOKEN existe
 └── KeyringSecretStore   ← si no, el llavero
```

Por fuera es un `SecretStore` más; por dentro recorre sus capas. `bootstrap` y los casos de uso no saben que hay varias. Mañana se puede agregar una capa (un gestor de contraseñas, un archivo cifrado) sin tocar a quien lo usa.

### Raíz de composición y registro de proveedores

`bootstrap` es el **único** lugar que conoce a la vez todos los adaptadores. Lee la configuración, elige y construye las piezas, y entrega un `AppContext` listo.

El registro (§13.3) describe cada proveedor con datos, no con código especial:

```rust
pub const DESCRIPTOR: TaskProviderDescriptor = TaskProviderDescriptor {
    kind: ProviderKind::ClickUp,
    display_name: "ClickUp",
    fields: &[ /* token (secreto), team_id (texto) */ ],
    provider_settings,   // fn: respuestas del asistente → [provider]
    build,               // fn: configuración + secretos → Arc<dyn TaskProvider>
};
```

- El asistente recorre `fields` sin saber qué proveedor es.
- `bootstrap::build` usa el `build` del mismo descriptor.
- Agregar GitHub en la fase 11 será agregar un descriptor: ni el asistente ni `build` cambian.

Puede ser `const` porque todos sus campos son conocidos al compilar: `&'static str`, slices `&'static [...]` y **punteros a función**. Una closure que captura variables no podría estar ahí.

**Corrección al diseño (ADR-026):** el documento decía `adapter_clickup::DESCRIPTOR`, con el descriptor dentro del adaptador. Eso crea un ciclo: el descriptor usa `BuildError` de `bootstrap`, así que `adapter-clickup` dependería de `bootstrap`, y `bootstrap` ya depende de `adapter-clickup`. Cargo no admite dependencias circulares, así que el descriptor vive en `bootstrap/src/clickup.rs`.

### Objeto nulo: `NoProjects`

Los proyectos llegan en la fase 5, pero `AppContext` exige un `ProjectRepository`. En vez de volverlo `Option` y llenar los casos de uso de `if let Some`, se pasa un repositorio que siempre responde "no hay proyectos". El código que lo usa no tiene casos especiales.

### `clap` derive

```rust
#[derive(Parser)]
pub struct Cli {
    /// Ruta de config.toml.               ← el comentario de documentación es el texto de --help
    #[arg(long, global = true)]          ← --config, válido en cualquier subcomando
    pub config: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

enum Command {
    Show {
        #[arg(value_parser = parse_task_id)]   ← función propia: &str → Result<TaskId, _>
        task_id: TaskId,
    },
    Config(ConfigCommand),                      ← subcomandos anidados: taskflow config set ...
}

enum ConfigCommand {
    Get { key: ConfigKey },     ← ConfigKey implementa FromStr: clap lo usa solo
}
```

Igual que `serde`, el derive genera el parser al compilar. Lo importante: **los tipos validan al parsear**. Un `ConfigKey` desconocido o un `TaskId` vacío nunca llegan a los comandos: `clap` muestra el error y la ayuda.

En los tests, `Cli::try_parse_from([...])` parsea sin tocar los argumentos reales del proceso, y `Cli::command().debug_assert()` revisa la definición completa (flags repetidos, nombres en conflicto).

### Presentación pura

`presenter.rs` **devuelve** `String` en lugar de imprimir. Así los tests comparan texto, y los comandos solo hacen `println!("{}", presenter::today(...))`. Es la misma idea que separar el caso de uso (`application`) de la interfaz: lo que tiene lógica se prueba; lo que solo imprime es una línea.

### Dependencias por plataforma

```toml
[target.'cfg(target_os = "macos")'.dependencies]
keyring = { workspace = true, features = ["apple-native"] }
```

`keyring` necesita un backend distinto en cada sistema (Keychain, Credential Manager, keyutils). `[target.'cfg(...)'.dependencies]` agrega dependencias o features solo al compilar para esa plataforma. El CI compila en las tres y cada una elige la suya.

---

## Pistas por paso

Intenta primero sin leerlas.

<details>
<summary>1. AppSettings::new, ProviderKind, PolicySettings</summary>

- `new`: un struct con `version: Self::CURRENT_VERSION`, el `provider` recibido y `Default::default()` para el resto.
- `as_str`: un `match` con un brazo por variante. Cuando llegue GitHub, el compilador te obligará a agregar su brazo.
- `PolicySettings::default`: **no copies los números** (10, 50, 30...). Toma `WeightedPolicy::default()` y copia sus campos: así hay una sola fuente de verdad, y el test `policy_defaults_match_the_domain_defaults` lo vigila.
- `From<&PolicySettings> for WeightedPolicy`: campo por campo. Recibe `&PolicySettings`, pero los `i64` son `Copy`: no hace falta `clone()`.

</details>

<details>
<summary>2. AppSettings::validate</summary>

- Empieza con `let mut issues = Vec::new();` y haz `issues.push(SettingsIssue::new("clave", "motivo"))` por cada regla que falle.
- Regla 2 y 3 juntas: `match (self.provider.kind, &self.provider.clickup)` con dos brazos, `(ProviderKind::ClickUp, None)` y `(ProviderKind::ClickUp, Some(clickup))`.
- Solo dígitos: `!id.is_empty() && id.chars().all(|c| c.is_ascii_digit())`.
- Regla 4: recorre las cinco listas como una sola. `[&map.todo, &map.ready, ...].into_iter().flatten()` da todos los nombres. Un `HashSet<String>` recuerda los vistos: `set.insert(name.to_lowercase())` devuelve `false` si ya estaba.
- Al final: `if issues.is_empty() { Ok(()) } else { Err(ConfigError::Invalid(issues)) }`.

</details>

<details>
<summary>3. ConfigKey</summary>

- `segments`: `self.0.split('.')`. El tipo de retorno es `Split<'static, char>`, lo que devuelve `split`.
- `from_str`: busca `s` en `Self::KNOWN` con `iter().find(|known| **known == s)`. Ojo con las referencias: `iter()` sobre `&[&str]` da `&&str`, y `find` pasa una referencia a eso, `&&&str`. Los `**` dejan un `&str` para comparar.
- Guarda **la entrada de `KNOWN`** (que es `&'static str`), no `s`: `s` solo vive lo que dura la llamada. Por eso `ConfigKey` puede ser `Copy` y no reservar memoria.
- Si no está: `ConfigError::UnknownKey(s.to_owned())`.

</details>

<details>
<summary>4. SecretName</summary>

- Un `match self` en cada método, devolviendo literales `&'static str`.

</details>

<details>
<summary>5. TaskDetail, CommentView, InspectTask</summary>

- `assigned_to_me`: `comment.assignee.as_ref() == Some(me)`. `Option<&UserId>` se compara con `Option<&UserId>`.
- `location`: por cada segmento, el nombre o, si está vacío, el id. `if s.name.is_empty() { s.id.clone() } else { s.name.clone() }`.
- `project.map(ProjectView::from)` convierte `Option<&Project>` en `Option<ProjectView>`.
- `InspectTask`: `futures::try_join!(a, b, c)` espera **tres futures de tipos distintos** a la vez y devuelve una tupla. `try_join_all` (fase 2) era para muchos futures **del mismo tipo**.

  ```rust
  let (me, mut task, comments) = futures::try_join!(
      self.ctx.tasks.current_user(),
      self.ctx.tasks.task(id),
      self.ctx.tasks.comments(id),
  )?;
  ```

- Los proyectos se piden después: `try_join!` exige que todos los errores sean del mismo tipo, y `list()` devuelve `RepoError`, no `ProviderError`.

</details>

<details>
<summary>6. atomic::write</summary>

- Directorio: `path.parent()` es `Option<&Path>`. Si es `None` (una ruta como `config.toml` sin carpeta), usa `Path::new(".")`.
- `std::fs::create_dir_all(dir)?` no falla si ya existe.
- `let mut file = NamedTempFile::new_in(dir)?;` → `file.write_all(contents.as_bytes())?;` → `file.as_file().sync_all()?;`.
- `file.persist(path)` devuelve un `PersistError`, que **no** es un `io::Error`: contiene uno en `.error` (y el archivo temporal, para reintentar). `.map_err(|e| e.error)?`.

</details>

<details>
<summary>7. edit: lookup, get, parse_value, set</summary>

- `lookup`: empieza en `doc.as_item()` y baja con `item = item.get(segment)?;` por cada segmento. El `?` sobre `Option` corta con `None` si falta un tramo.
- `get`: `lookup(...)?.as_value()?` da el `&Value`. Su `Display` incluye la decoración (espacios y comentario). Clónalo, limpia con `value.decor_mut().clear()` y usa `to_string()`.
- `parse_value`: `current.and_then(Item::as_str).is_some()` dice si el valor actual es texto. Si no, `raw.parse::<Value>()` y, si falla, `Value::from(raw)`.
- `set`:
  1. Separa el último tramo: `let mut segments: Vec<&str> = key.segments().collect(); let last = segments.pop()...`.
  2. Baja por las tablas: `table.entry(segment).or_insert_with(|| Item::Table(Table::new())).as_table_mut()`. Si no es tabla, `ConfigError::Syntax`.
  3. **El detalle importante:** si la clave existe, cambia el valor **en su lugar** con `table.get_mut(last).and_then(Item::as_value_mut)`, copiando antes su `decor()`. Si usas `table.insert` sobre una clave existente, se pierde el comentario de la línea anterior, porque ese comentario es la decoración de la **clave**. El test `set_keeps_comments_and_the_rest_of_the_file` lo detecta.

</details>

<details>
<summary>8. FsConfigStore</summary>

- `default_path`: `directories::ProjectDirs::from("", "", "taskflow")` devuelve `Option<ProjectDirs>`; `.map(|dirs| dirs.config_dir().join("config.toml"))`.
- `read`: `std::fs::read_to_string(&self.path)` y, en el error, `match e.kind() { io::ErrorKind::NotFound => ConfigError::NotFound(...), _ => self.io_error(&e) }`.
- `parse`: `toml::from_str::<AppSettings>(text)` con `map_err` a `Syntax`, después `settings.validate()?`.
- `render`: `toml::to_string_pretty(settings)` y `format!("{HEADER}{body}")`.
- `apply`: arma el texto nuevo en un `match change` y escribe una sola vez al final con `atomic::write(&self.path, &text).map_err(|e| self.io_error(&e))`.
- Parsear a `DocumentMut`: `text.parse::<DocumentMut>()`. Su error es `toml_edit::TomlError`.

</details>

<details>
<summary>9. adapter-keyring</summary>

- `EnvSecretStore::new`: `Self::with_lookup(|name| std::env::var(name).ok())`. La closure no captura nada: se convierte en puntero a función.
- `get`: `(self.lookup)(name.env_var())`. Los paréntesis son necesarios: `self.lookup(...)` buscaría un **método** llamado `lookup`. Después `.filter(|v| !v.is_empty()).map(SecretString::from)`.
- `KeyringSecretStore::get`: `self.entry(name)?.get_password()` y un `match` con tres brazos: `Ok(value)`, `Err(keyring::Error::NoEntry)` y cualquier otro error.
- `LayeredSecretStore::get`: un `for` sobre las capas con `if let Some(value) = layer.get(name)? { return Ok(Some(value)); }`.
- `set`: por cada capa, `match layer.set(name, value.clone())`. `Err(SecretError::ReadOnly(_)) => {}` sigue a la siguiente; cualquier otro resultado (`Ok` o un error real) se devuelve. `SecretString` se puede clonar.

</details>

<details>
<summary>10. registry y clickup.rs</summary>

- `find`: `task_providers().iter().find(|d| d.kind == kind).expect("...")`.
- `provider_settings`: `values.get("team_id").cloned().unwrap_or_default()`.
- `build`: `secrets.get(SecretName::ClickUpToken)?` da `Option<SecretString>`; `.ok_or(BuildError::MissingSecret(...))?` lo vuelve obligatorio. Después `clickup_config` y `Arc::new(ClickUpProvider::new(config)?)`.
- `clickup_config`: `settings.provider.clickup.as_ref().ok_or(BuildError::MissingSection("provider.clickup"))?`. Empieza con `ClickUpConfig::new(token.expose_secret(), ...)` y reemplaza `statuses`.

</details>

<details>
<summary>11. build.rs</summary>

- `config_store`: `match path { Some(p) => p.to_path_buf(), None => FsConfigStore::default_path().ok_or(BuildError::NoConfigDir)? }`.
- `secret_store`: `LayeredSecretStore::new(vec![Box::new(EnvSecretStore::new()), Box::new(KeyringSecretStore::new())])`.
- `build`: `let descriptor = registry::find(settings.provider.kind);` y `(descriptor.build)(settings, secrets)?` (otra vez los paréntesis: es un campo con una función, no un método).

</details>

<details>
<summary>12. parse_task_id</summary>

- Una línea: `TaskId::new(raw)`. `clap` muestra el `Display` del error ("task id cannot be empty").

</details>

<details>
<summary>13. presenter</summary>

- `due_label`: convierte a fecha local con `due.with_timezone(&offset).date_naive()` y compárala con `today` y `today.succ_opt()` (el día siguiente, `Option` porque podría desbordar).
- Formato de fecha: `date.format("%Y-%m-%d")`.
- `today`: recorre los grupos saltando los vacíos, con un contador `number` que **no** se reinicia entre grupos. Usa `table()` y `row()` (ya hechos) y junta los bloques con `"\n\n"`. Si al final `number == 0`, el mensaje de "sin tareas".
- `task_detail`: arma un `Vec<String>` de líneas y termina con `lines.join("\n")`. Las líneas opcionales (proyecto, etiquetas) se agregan con `if`.
- `json`: `serde_json::to_string_pretty(value)`.

</details>

<details>
<summary>14. Comandos y asistente</summary>

- `commands/mod.rs` ya está hecho: léelo primero para ver qué recibe cada comando.
- `today::run`: el `today` local es `ctx.clock.now().with_timezone(&offset).date_naive()`. Usa el reloj del contexto, no `Utc::now()`: así se puede probar con `FixedClock`.
- `config::run`: el brazo `ConfigCommand::Init | ConfigCommand::Path` nunca llega aquí; `unreachable!("...")` lo deja explícito.
- `wizard`: `Select::with_theme(&theme).with_prompt(...).items(&names).default(0).interact()?` devuelve el índice elegido. `Input::<String>::with_theme(theme).with_prompt(...).interact_text()?` y `Password::with_theme(theme).with_prompt(...).interact()?`.
- Valida (`settings.validate()?`) **antes** de guardar el token: si el `team_id` es inválido, no queda un secreto guardado de una configuración que no existe.

</details>

---

## Paso 14: probar los comandos a mano

Sin tocar tu configuración real, con un archivo temporal y un token falso:

```sh
export TASKFLOW_CLICKUP_TOKEN=pk_falso
cargo run -p taskflow -- --help
cargo run -p taskflow -- config path --config /tmp/tf.toml
cargo run -p taskflow -- today --config /tmp/tf.toml          # "no configuration at /tmp/tf.toml"
```

Crea `/tmp/tf.toml` a mano:

```toml
version = 1

[provider]
kind = "clickup"

[provider.clickup]
team_id = "9001"  # mi workspace
```

```sh
cargo run -p taskflow -- config show --config /tmp/tf.toml
cargo run -p taskflow -- config set ui.group_by project --config /tmp/tf.toml
cargo run -p taskflow -- config set policy.overdue_bonus alto --config /tmp/tf.toml   # error de tipo
cargo run -p taskflow -- config set provider.clickup.team_id abc --config /tmp/tf.toml # error de regla
cat /tmp/tf.toml                                                # el comentario sigue ahí
cargo run -p taskflow -- today --config /tmp/tf.toml           # "the provider rejected the credentials"
unset TASKFLOW_CLICKUP_TOKEN
```

El `--` separa los argumentos de `cargo` de los de tu programa.

## Paso 15: contra tu ClickUp real

```sh
cargo run -p taskflow -- config init
cargo run -p taskflow -- config check
cargo run -p taskflow -- today
cargo run -p taskflow -- show <ID de una tarea>
```

- `config init` guarda el token en el **llavero** (en macOS: app *Acceso a Llaveros*, entrada `taskflow`). `config.toml` nunca lo contiene: compruébalo con `cat "$(cargo run -q -p taskflow -- config path)"`.
- **macOS puede pedir permiso** para leer el llavero. Ocurre cada vez que recompilas, porque el binario cambia. Elige *Permitir siempre*.
- Si tienes `TASKFLOW_CLICKUP_TOKEN` exportado en la terminal, **gana sobre el llavero**: es la precedencia de `LayeredSecretStore`.

Para tener el comando `taskflow` sin `cargo run`:

```sh
cargo install --path apps/cli
taskflow today
```

**Al terminar**, borra `crates/adapters/clickup/examples/today.rs`: `taskflow today` lo reemplaza.

---

## Limitaciones conocidas (aceptadas para esta fase)

- **Sin proyectos:** `NoProjects` hasta la fase 5. Con `ui.group_by = "project"`, todo cae en un solo grupo.
- **El `team_id` se escribe a mano.** El diseño (§13.3) prevé campos `Discover` que consultan la API y ofrecen una lista de workspaces; llegará cuando haga falta.
- **Secciones de fases futuras** (`[ai]`, `[projects]`, `[session]`...) son rechazadas por `deny_unknown_fields`. Cada fase las agrega a `AppSettings`.
- **`config edit`** (FR-24) y las migraciones de versión (FR-25) quedan para después.
- **`show` sin IA:** el resumen y el prompt para el agente llegan en la fase 6.

---

## Qué observar mientras implementas

- **`GetTodayTasks` no cambia.** El caso de uso de la fase 2 ahora lo invoca una interfaz real.
- **El CLI no sabe que existe ClickUp.** Busca `clickup` en `apps/cli/src`: solo aparece en textos para el usuario. Todo lo específico vive en el adaptador y en su descriptor.
- **Cada error llega con su explicación.** `MissingSecret` dice qué variable exportar; `NotFound` dice que ejecutes `config init`; `Invalid` lista cada clave con su motivo.
- **Nada se escribe si algo es inválido.** Ni el archivo (validación antes de `atomic::write`) ni el token (validación antes de `secrets.set`).

Cuando termines, avísame y reviso tu implementación. Las dudas que surjan las agregamos al final de este documento, como en las fases anteriores.

---

# Conceptos aplicados

La fase quedó implementada completa. Esta sección explica, paso por paso, las decisiones de la implementación y las dudas que surgieron.

## Paso 1: `AppSettings`, `ProviderKind`, `PolicySettings`

### `From`: "sé construirme a partir de otro tipo"

```rust
impl From<&PolicySettings> for WeightedPolicy {
    fn from(settings: &PolicySettings) -> Self { ... }
}

let policy = WeightedPolicy::from(&settings.policy);    // explícito
let policy: WeightedPolicy = (&settings.policy).into(); // `Into` sale gratis
```

Al implementar `From<A> for B`, la biblioteca estándar regala `Into<B> for A`. Por eso siempre se implementa `From`, nunca `Into`.

**Para qué sirve aquí:** hay dos tipos con los mismos seis pesos, uno por capa.

| Tipo | Capa | Para qué existe |
| --- | --- | --- |
| `PolicySettings` | `application` | Leerse y escribirse en `config.toml` (`serde`, `deny_unknown_fields`) |
| `WeightedPolicy` | `domain` | Puntuar tareas |

El dominio no sabe que existe un archivo de configuración. La traducción vive en `application`, que conoce a ambos, igual que el `mapper` de la fase 3 traducía `ApiTask` → `Task`. Como no puede fallar, es `From` y no `TryFrom`.

**Ya lo habías usado:** el `?` convierte errores llamando a `From::from` por dentro. Así funcionaba `impl From<MapError> for ProviderError` en la fase 3.

**¿Por qué `&PolicySettings`?** Quien convierte solo tiene un préstamo: `bootstrap::build` recibe `&AppSettings` y no puede mover `settings.policy` fuera. Y no hace falta clonar: los `i64` son `Copy`.

**Regla del huérfano:** ni `From` (biblioteca estándar) ni `WeightedPolicy` (`domain`) son de `application`, y aun así compila. La regla exige que **algún** tipo de la implementación sea local, y `PolicySettings` aparece como parámetro del trait. `&T` no cambia eso: `&PolicySettings` cuenta como local.

### `as_str`: tres lecciones en un `match` de un brazo

```rust
match self {
    Self::ClickUp => "clickup",
}
```

- **Debe coincidir con `serde`.** `#[serde(rename_all = "lowercase")]` hace que el archivo diga `kind = "clickup"`. Si `as_str` devolviera `"ClickUp"` o `"clickUp"`, habría dos nombres para lo mismo.
- **Sin `_ =>`.** Con una sola variante, el comodín nunca se ejecuta, pero apaga la exhaustividad: al agregar `GitHub` en la fase 11, el compilador ya no te obligaría a decidir su nombre y devolvería basura en silencio.
- **No existe "no existe".** Si tienes un `ProviderKind`, es un proveedor válido: el tipo ya lo garantiza.

### Una sola fuente de verdad

```rust
fn default() -> Self {
    let policy = WeightedPolicy::default();
    Self {
        priority_weight: policy.priority_weight,
        // ...
    }
}
```

Los números (10, 50, 30...) viven solo en `WeightedPolicy::default()`. Si alguien los cambia ahí, la configuración por defecto cambia también, y el test `policy_defaults_match_the_domain_defaults` lo vigila. Se crea una vez en una variable en lugar de llamar a `WeightedPolicy::default()` seis veces.

## Paso 2: `validate`

### Clave y mensaje de cada problema

`SettingsIssue::new(clave, mensaje)`:

- **Clave:** la ruta con puntos del valor que está mal, la misma que acepta `config set`. Al leer el error, el usuario ya sabe qué comando corrige el problema.
- **Mensaje:** el motivo, con el valor recibido. Los tests solo comprueban la clave.

| # | Cuándo falla | Clave | Mensaje |
| --- | --- | --- | --- |
| 1 | `version != CURRENT_VERSION` | `version` | `unsupported version 2, expected 1` |
| 2 | ClickUp sin `[provider.clickup]` | `provider.clickup` | `missing section [provider.clickup]` |
| 3 | `team_id` vacío o no numérico | `provider.clickup.team_id` | `` `abc` must contain only digits `` |
| 4 | Nombre en dos listas | `provider.status_map` | `` `complete` appears in more than one list `` |

```text
Error: invalid configuration:
  - version: unsupported version 2, expected 1
  - provider.clickup.team_id: `abc` must contain only digits
```

### `match` sobre una tupla

```rust
match (self.provider.kind, &self.provider.clickup) {
    (ProviderKind::ClickUp, None) => { /* regla 2 */ }
    (ProviderKind::ClickUp, Some(clickup)) => { /* regla 3 */ }
}
```

Combina dos valores en un solo `match`, así las reglas 2 y 3 se excluyen solas (sin sección no hay `team_id` que revisar). `&self.provider.clickup` va con `&`: sin él, el `match` intentaría **mover** el `Option` fuera de `self`, que está prestado.

### Detectar repetidos con `HashSet::insert`

```rust
let mut seen = HashSet::new();
for name in [&map.todo, &map.ready, &map.in_progress, &map.in_review, &map.done]
    .into_iter()
    .flatten()
{
    if !seen.insert(name.to_lowercase()) { /* repetido */ }
}
```

- Un array de cinco `&Vec<String>` + `flatten` = un solo iterador con todos los nombres, sin copiar las listas.
- `insert` devuelve `false` si el valor ya estaba: detecta el repetido y lo registra en una sola operación.
- `to_lowercase()` porque los estados se comparan sin distinguir mayúsculas (como `StatusMap::to_domain` en la fase 3).

## Paso 3: `ConfigKey`

```rust
Self::KNOWN
    .iter()
    .find(|known| **known == s)
    .map(|known| Self(known))
    .ok_or_else(|| ConfigError::UnknownKey(s.to_owned()))
```

- **`**known`:** `KNOWN` es `&[&str]`, `iter()` da `&&str`, y `find` pasa una referencia a cada elemento: `&&&str`. Dos `*` dejan un `&str`, del mismo tipo que `s`, y `==` compara el texto. Si te pierdes, el compilador te dice cuántos `*` faltan.
- **Se guarda la entrada de `KNOWN`, no `s`.** `s` solo vive lo que dura la llamada; la entrada de `KNOWN` es `&'static str`, vive todo el programa. Por eso `ConfigKey` es `Copy` y no reserva memoria.
- **`segments` devuelve `Split<'static, char>`**, el tipo concreto de `str::split`. Con `impl Iterator` el esqueleto no compilaba: `todo!()` tiene tipo `!` ("nunca"), y `impl Trait` necesita un tipo real del que inferir.

## Paso 5: `InspectTask`

### `try_join!` contra `try_join_all`

```rust
let (me, mut task, comments) = futures::try_join!(
    self.ctx.tasks.current_user(),   // Future<Output = Result<UserId, _>>
    self.ctx.tasks.task(id),         // Future<Output = Result<Task, _>>
    self.ctx.tasks.comments(id),     // Future<Output = Result<Vec<Comment>, _>>
)?;
```

| | `try_join_all` (fase 2) | `try_join!` |
| --- | --- | --- |
| Cuántos | Cualquier cantidad, conocida en ejecución | Un número fijo, escrito en el código |
| Tipos | Todos **iguales** | **Distintos** |
| Devuelve | `Vec<T>` | Una tupla `(A, B, C)` |

Las tres peticiones son independientes, así que se lanzan a la vez: tarda lo que la más lenta, no la suma. Los proyectos se piden después porque `list()` devuelve `RepoError` y `try_join!` exige que todos los errores sean del mismo tipo.

### `comment.assignee.as_ref() == Some(me)`

`assignee` es `Option<UserId>` y `me` es `&UserId`. `as_ref()` lo convierte en `Option<&UserId>`, que se puede comparar con `Some(me)` sin clonar nada.

## Paso 6: escritura atómica

```rust
let mut file = NamedTempFile::new_in(dir)?;
file.write_all(contents.as_bytes())?;
file.as_file().sync_all()?;
file.persist(path).map_err(|e| e.error)?;
```

`persist` devuelve un `PersistError`, que contiene el `io::Error` (`.error`) **y** el archivo temporal, por si quieres reintentar. `map_err(|e| e.error)` se queda con el error. Si algo falla antes de `persist`, el `Drop` de `NamedTempFile` borra el temporal: es RAII, como el permiso del semáforo.

## Paso 7: `toml_edit`

### La decoración pertenece a la clave o al valor

```text
# Peso de la prioridad          ← decoración de la CLAVE
priority_weight = 10  # mío     ← "  # mío" es decoración del VALOR
```

Por eso `set` cambia el valor **en su lugar**:

```rust
match table.get_mut(last).and_then(Item::as_value_mut) {
    Some(existing) => {
        let decor = existing.decor().clone();
        *existing = value;               // la clave no se toca: su comentario se conserva
        *existing.decor_mut() = decor;   // y el comentario del final se copia al valor nuevo
    }
    None => {
        table.insert(last, Item::Value(value));
    }
}
```

`table.insert` sobre una clave que ya existe crea una clave nueva y pierde el comentario de la línea anterior. El test `set_keeps_comments_and_the_rest_of_the_file` encontró justo este error en la implementación de referencia.

### `lookup` con `?` sobre `Option`

```rust
let mut item = doc.as_item();
for segment in key.segments() {
    item = item.get(segment)?;   // si falta un tramo, la función devuelve None
}
Some(item)
```

`?` funciona con `Option` igual que con `Result`: `None` corta la función. Y no hay lifetimes escritos: con una sola referencia de entrada, Rust asume que la salida vive lo mismo (*elision*).

### `parse_value`: el tipo actual decide

`config set provider.clickup.team_id 9001` no debe guardar el **número** 9001: `team_id` es texto. Si el valor actual es un texto, lo nuevo se guarda como texto sin interpretarlo. Si no, se interpreta como TOML (`80`, `true`, `["qa"]`) y, si no es TOML válido (`project`), se guarda como texto.

## Paso 8: `FsConfigStore`

```rust
SettingsChange::Set { key, value } => {
    let mut doc = self.document()?;
    let value = edit::parse_value(value, edit::lookup(&doc, *key));
    edit::set(&mut doc, *key, value)?;
    let text = doc.to_string();
    parse(&text)?;          // validar el documento COMPLETO antes de escribir
    text
}
```

- `toml_edit` edita sin perder formato, pero no valida tipos. `parse` (con `toml` y `validate`) revisa el resultado final. Solo si pasa, se escribe.
- `apply` arma el texto en un `match` y escribe **una sola vez** al final: los dos brazos comparten la escritura atómica.
- `read` traduce `io::ErrorKind::NotFound` a `ConfigError::NotFound`, cuyo mensaje dice "run `taskflow config init`". Cualquier otro error de E/S es `Io` con la ruta.
- `document()` es un método privado nuevo que comparten `apply` y `get_raw`, para no repetir el parseo.

## Paso 9: secretos

- **`(self.lookup)(name.env_var())`:** los paréntesis son necesarios. `self.lookup(...)` buscaría un **método** llamado `lookup`; `(self.lookup)(...)` llama a la **función guardada en el campo**.
- **`keyring::Error::NoEntry` es `Ok(None)`:** "no está guardado" es una respuesta normal, no un fallo. Los demás errores (llavero bloqueado) sí son `Backend`.
- **`LayeredSecretStore::set`:** `Err(SecretError::ReadOnly(_)) => {}` salta las capas de solo lectura; `other => return other` devuelve cualquier otro resultado, éxito o error real. Un error del llavero no se oculta probando la siguiente capa.

## Pasos 10 y 11: `bootstrap`

```rust
let descriptor = registry::find(settings.provider.kind);
Ok(AppContext {
    tasks: (descriptor.build)(settings, secrets)?,
    projects: Arc::new(NoProjects),
    policy: Arc::new(WeightedPolicy::from(&settings.policy)),
    clock: Arc::new(SystemClock),
})
```

- `(descriptor.build)(...)`: igual que con `lookup`, se llama a la función guardada en un campo.
- `.ok_or(BuildError::MissingSecret(...))?` convierte "no está" (`None`) en un error con instrucciones: "set TASKFLOW_CLICKUP_TOKEN or run `taskflow config init`".
- `build` no sabe que existe ClickUp: solo busca el descriptor de `settings.provider.kind`. Agregar un proveedor no lo cambia.

## Pasos 13 y 14: presentación y comandos

- **El día local sale del reloj del contexto** (`ctx.clock.now().with_timezone(&offset).date_naive()`), no de `Utc::now()`: el mismo criterio que `WeightedPolicy`, y se puede probar con `FixedClock`.
- **`today.succ_opt()`** devuelve `Option` porque el día siguiente al último representable no existe. Compararlo con `Some(date)` evita un `unwrap`.
- **El número de fila sigue entre grupos** (`number` fuera del `for`): en `today --by-project` (fase 5) cada tarea tendrá un número único.
- **El asistente valida antes de guardar el token:** si el `team_id` es inválido, no queda un secreto guardado de una configuración que nunca se escribió.
- **`unreachable!`** en `config::run` documenta que `init` y `path` se atienden antes, en `commands::run`. Si algún día llegaran ahí, el programa se detiene con un mensaje claro en vez de hacer algo raro.

## Ajuste: recortar títulos largos en `today`

Los títulos muy largos rompían la tabla. Ahora se cortan en 50 caracteres (`TITLE_WIDTH`) y terminan en `…`. `show` sigue mostrando el título completo.

```rust
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(max.saturating_sub(1)).collect();
    short.push('…');
    short
}
```

### Caracteres contra bytes

Un `String` de Rust es UTF-8: cada carácter ocupa **de 1 a 4 bytes**.

| Texto | `.len()` (bytes) | `.chars().count()` (caracteres) |
| --- | --- | --- |
| `"Fix"` | 3 | 3 |
| `"Revisión"` | 9 | 8 (la `ó` ocupa 2 bytes) |
| `"ñ"` | 2 | 1 |

`&text[..10]` corta por **bytes**. Si el byte 10 cae en medio de una `ó`, Rust hace **panic** ("byte index 10 is not a char boundary"): prefiere detenerse antes que crear un texto inválido. Con títulos en español eso pasaría tarde o temprano. `chars()` recorre caracteres completos, así que `take(n)` nunca corta una letra a la mitad.

Detalles:

- **`saturating_sub(1)`:** deja lugar para el `…`. Si `max` fuera 0, `max - 1` desbordaría; `saturating_sub` se queda en 0.
- **`collect::<String>()`:** un iterador de `char` se puede reunir directamente en un `String`.
- **Límite conocido:** algunos emojis y letras con tildes combinadas son varios `char` que se ven como uno (*grafemas*). Para títulos de tareas no importa; si importara, el crate `unicode-segmentation` cuenta grafemas.
- **Alternativa descartada:** `comfy-table` puede ajustar la tabla al ancho de la terminal (`ContentArrangement::Dynamic`), pero parte los títulos en varias líneas. Recortar mantiene una tarea por línea.

## Limpieza al cerrar la fase

- Se borraron los cinco bloques `#![allow(...)]` con `TODO(fase 4)`, y un `use std::default;` que había quedado sin usar.
- Se borró `crates/adapters/clickup/examples/today.rs`: `taskflow today` lo reemplaza. Con él se fue `dotenvy`, que solo usaba el ejemplo.
- `.env.example` ahora explica que las variables son **opcionales** y que Taskflow no lee `.env`: el token va al llavero con `config init`. Si exportas `TASKFLOW_CLICKUP_TOKEN`, gana sobre el llavero.
