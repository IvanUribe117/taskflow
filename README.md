# Taskflow

CLI en Rust que automatiza el flujo diario de desarrollo con ClickUp: tareas priorizadas, contexto por proyecto, análisis con IA, sesiones de trabajo y Playbooks.

El diseño completo está en [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md). Las explicaciones de cada fase están en [`docs/learning/`](docs/learning/).

## Estado

| Fase | Entregable | Estado |
| --- | --- | --- |
| 0 | Workspace y CI | Hecho |
| 1 | `domain`: tareas, proyectos y política | Hecho |
| 2 | Puertos y `GetTodayTasks` con fakes | Hecho |
| 3 | `adapter-clickup` de lectura | Hecho ([guía](docs/learning/fase-3.md)) |

## Desarrollo

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
