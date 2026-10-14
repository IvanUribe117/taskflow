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
| 4 | Configuración, `bootstrap` y CLI `today`/`show` | Hecho ([guía](docs/learning/fase-4.md)) |
| 5 | Proyectos y agrupación | Hecho ([guía](docs/learning/fase-5.md)) |
| 7 | `close`: cerrar tareas en ClickUp (escritura) | Hecho; falta probarla contra ClickUp real ([guía](docs/learning/fase-7.md)) |
| 6 | Contexto por proyecto, IA y `plan` | Pendiente |
| 8 | Sesión de trabajo (`start` → `done`) | Pendiente |

Orden acordado el 2026-10-14: **7 → 6 → 8**. La 7 no depende de la IA y es útil desde ya; la 8 necesita la 6 (el reporte con IA) y la 7 (escribir en ClickUp).

## Pendientes importantes

- **Límite de peticiones de ClickUp** (100 por minuto en los planes Free, Unlimited y Business): `project add` puede superarlo en workspaces grandes. Ver `docs/ARCHITECTURE.md` §26.

## Desarrollo

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
