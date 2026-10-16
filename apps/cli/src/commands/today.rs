use application::AppContext;
use application::settings::AppSettings;
use application::tasks::{GetTodayTasks, GroupBy, TodayQuery};
use domain::project::ProjectSlug;

use crate::presenter;

/// La consulta de `today` a partir de los argumentos y la configuración:
///
/// - `limit` y `project` pasan tal cual.
/// - `group_by`: `GroupBy::Project` si se pasó `--by-project`; si no, `settings.ui.group_by`.
#[must_use]
pub(crate) fn query(
    settings: &AppSettings,
    limit: Option<usize>,
    by_project: bool,
    project: Option<ProjectSlug>,
) -> TodayQuery {
    TodayQuery {
        limit,
        project,
        group_by: if by_project {
            GroupBy::Project
        } else {
            settings.ui.group_by
        },
    }
}

/// `taskflow today [--json] [--limit N] [--by-project] [--project SLUG]`
///
/// # Errors
///
/// Los del caso de uso (red, token inválido...).
pub(crate) async fn run(ctx: &AppContext, json: bool, query: TodayQuery) -> anyhow::Result<()> {
    let report = GetTodayTasks::new(ctx).execute(query).await?;

    if json {
        println!("{}", presenter::json(&report)?);
    } else {
        let offset = ctx.clock.offset();
        let today = ctx.clock.now().with_timezone(&offset).date_naive();
        println!("{}", presenter::today(&report, offset, today));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use application::settings::{
        AppSettings, ClickUpSettings, ProviderKind, ProviderSettings, StatusNames,
    };

    use super::*;

    fn settings(group_by: GroupBy) -> AppSettings {
        let mut settings = AppSettings::new(ProviderSettings {
            kind: ProviderKind::ClickUp,
            clickup: Some(ClickUpSettings {
                team_id: "9001".into(),
            }),
            status_map: StatusNames::default(),
        });
        settings.ui.group_by = group_by;
        settings
    }

    #[test]
    fn uses_the_configured_grouping_by_default() {
        let query = query(&settings(GroupBy::Project), Some(5), false, None);

        assert_eq!(
            query,
            TodayQuery {
                limit: Some(5),
                project: None,
                group_by: GroupBy::Project,
            }
        );
    }

    #[test]
    fn by_project_flag_wins_over_the_configuration() {
        let query = query(&settings(GroupBy::None), None, true, None);

        assert_eq!(query.group_by, GroupBy::Project);
    }

    #[test]
    fn project_filter_passes_through() {
        let slug: ProjectSlug = "acme".parse().unwrap();

        let query = query(&settings(GroupBy::None), None, false, Some(slug.clone()));

        assert_eq!(query.project, Some(slug));
        assert_eq!(query.group_by, GroupBy::None);
    }
}
