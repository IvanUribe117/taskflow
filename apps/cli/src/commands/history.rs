use std::path::Path;

use application::AppContext;
use application::history::ListHistory;
use application::ports::HistoryFilter;
use serde::Serialize;

use crate::presenter;

/// `taskflow history [TASK_ID] [--project SLUG] [--plans] [--limit N] [--last] [--json]`.
///
/// - Sin `--last`: la tabla (o la lista en JSON).
/// - Con `--last`: el documento más reciente tal cual se guardó; con `--json`, el registro
///   y su contenido.
///
/// `base` es la carpeta del historial: la tabla muestra las rutas relativas a ella, y se
/// imprime al final para saber dónde buscar.
///
/// # Errors
///
/// Los del repositorio (no se puede leer la carpeta o el archivo).
pub async fn run(
    ctx: &AppContext,
    base: &Path,
    filter: HistoryFilter,
    limit: Option<usize>,
    last: bool,
    json: bool,
) -> anyhow::Result<()> {
    let history = ListHistory::new(ctx);
    if last {
        let Some((item, content)) = history.latest(&filter).await? else {
            println!("{}", presenter::history(&[], base));
            return Ok(());
        };
        if json {
            #[derive(Serialize)]
            struct Latest<'a> {
                #[serde(flatten)]
                item: &'a application::dto::HistoryItem,
                content: &'a str,
            }
            let latest = Latest {
                item: &item,
                content: &content,
            };
            println!("{}", presenter::json(&latest)?);
        } else {
            eprintln!("{}", presenter::path(&item.path));
            print!("{content}");
        }
        return Ok(());
    }

    let items = history.execute(&filter, limit).await?;
    if json {
        println!("{}", presenter::json(&items)?);
    } else {
        println!("{}", presenter::history(&items, base));
        if !items.is_empty() {
            println!("\nCarpeta: {}", presenter::path(base));
        }
    }
    Ok(())
}
