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
| 6A | Contexto por proyecto (secciones, `context list`/`preview`) | Hecho ([guía](docs/learning/fase-6.md)) |
| 6B | IA: `show --ai`, `plan` y prompt para el agente con Claude Code (`claude -p`), leyendo el repositorio del proyecto | Hecho; falta probarla con tus tareas ([guía](docs/learning/fase-6b.md)) |
| 6B+ | Historial: `show --ai --save`, `plan --save` y `history`, por proyecto | Hecho ([guía](docs/learning/fase-6b.md#ajuste-posterior-historial-de-análisis-y-planes)) |
| 8 | Sesión de trabajo (`start` → `done`) | Pendiente |

Orden acordado el 2026-10-14: **7 → 6 → 8**. La 7 no depende de la IA y es útil desde ya; la 8 necesita la 6 (el reporte con IA) y la 7 (escribir en ClickUp).

## Guías de uso

- [Crear el contexto de un proyecto](docs/guias/contexto-por-proyecto.md): qué secciones escribir para que la IA entienda cada proyecto, cómo revisarlas y cómo generarlas con un agente.

## Pendientes importantes

- **Límite de peticiones de ClickUp** (100 por minuto en los planes Free, Unlimited y Business). Corregido el 2026-10-14: `project add` pide el árbol por espacio, y ante un 429 se espera lo que indica ClickUp. Queda pendiente una caché local para no consultar la API en cada comando. Ver `docs/ARCHITECTURE.md` §26.

## Desarrollo

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
