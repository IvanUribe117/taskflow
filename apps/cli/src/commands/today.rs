use application::AppContext;
use application::settings::AppSettings;
use application::tasks::{GetTodayTasks, TodayQuery};

use crate::presenter;

/// `taskflow today [--json] [--limit N]`
///
/// 1. Ejecutar `GetTodayTasks` con `limit` y el `group_by` de `settings.ui`.
/// 2. Con `--json`, imprimir `presenter::json`; si no, `presenter::today` con la zona
///    horaria y el día local de `ctx.clock`.
///
/// # Errors
///
/// Los del caso de uso (red, token inválido...).
pub async fn run(
    ctx: &AppContext,
    settings: &AppSettings,
    json: bool,
    limit: Option<usize>,
) -> anyhow::Result<()> {
    let report = GetTodayTasks::new(ctx)
        .execute(TodayQuery {
            limit,
            project: None,
            group_by: settings.ui.group_by,
        })
        .await?;

    if json {
        println!("{}", presenter::json(&report)?);
    } else {
        let offset = ctx.clock.offset();
        let today = ctx.clock.now().with_timezone(&offset).date_naive();
        println!("{}", presenter::today(&report, offset, today));
    }
    Ok(())
}
