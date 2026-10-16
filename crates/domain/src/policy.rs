//! Política de prioridad (§9.5). Lógica de negocio pura: por eso vive en `domain`.

use std::cmp::Reverse;

use chrono::{DateTime, FixedOffset, TimeDelta, Utc};

use crate::project::ProjectResolver;
use crate::task::{Status, Task, UserId};

/// Datos externos que necesita una política para puntuar tareas.
#[derive(Debug, Clone, Copy)]
pub struct ScoringContext<'a> {
    /// La hora de ahora: decide si una tarea venció o vence hoy.
    pub now: DateTime<Utc>,
    /// Zona horaria del usuario: "vence hoy" depende del día local, no del día UTC.
    pub offset: FixedOffset,
    /// Quién soy: para contar los comentarios que me asignaron.
    pub me: &'a UserId,
    /// Los proyectos, para sumar su `priority_boost`.
    pub projects: &'a ProjectResolver,
}

/// Cómo se priorizan las tareas de `taskflow today` (§9.3). Una implementación solo da el
/// puntaje; el orden lo da [`PrioritizationPolicy::rank`].
pub trait PrioritizationPolicy: Send + Sync {
    /// El puntaje de `task`: más alto = más arriba. No hace I/O: todo lo que necesita
    /// (la hora, quién soy, los proyectos) llega en `ctx`.
    fn score(&self, task: &Task, ctx: &ScoringContext<'_>) -> i64;

    /// Ordena de mayor a menor puntaje. El orden es estable: los empates conservan
    /// el orden en que llegaron del proveedor.
    fn rank(&self, tasks: &mut [Task], ctx: &ScoringContext<'_>) {
        tasks.sort_by_cached_key(|t| Reverse(self.score(t, ctx)));
    }
}

/// "Vence pronto" (`DueThisWeek`): en los próximos 7 días desde ahora, no en la semana del
/// calendario.
const DUE_SOON_DAYS: i64 = 7;

/// Qué tan cerca está el vencimiento. Las categorías son excluyentes: una tarea vencida
/// no suma también el bono de "vence hoy".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Urgency {
    Overdue,
    DueToday,
    DueThisWeek,
    Later,
    NoDueDate,
}

impl Urgency {
    fn of(task: &Task, ctx: &ScoringContext<'_>) -> Self {
        let Some(due) = task.due else {
            return Self::NoDueDate;
        };
        if task.is_overdue(ctx.now) {
            return Self::Overdue;
        }
        let today = ctx.now.with_timezone(&ctx.offset).date_naive();
        if due.with_timezone(&ctx.offset).date_naive() == today {
            Self::DueToday
        } else if due - ctx.now <= TimeDelta::days(DUE_SOON_DAYS) {
            Self::DueThisWeek
        } else {
            Self::Later
        }
    }
}

/// Política por pesos configurables.
///
/// ```text
/// score = priority_weight × priority_value
///       + overdue_bonus | due_today_bonus | due_this_week_bonus   (excluyentes)
///       + pending_comment_weight × pending_comments
///       + in_review_bonus        si status == InReview
///       + project.priority_boost si la tarea pertenece a un proyecto
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeightedPolicy {
    /// Cuánto pesa cada punto de prioridad (`Urgent = 4` ... `None = 0`).
    pub priority_weight: i64,
    /// Bono si venció.
    pub overdue_bonus: i64,
    /// Bono si vence hoy.
    pub due_today_bonus: i64,
    /// Bono si vence en los próximos 7 días.
    pub due_this_week_bonus: i64,
    /// Cuánto suma cada comentario sin resolver que me asignaron.
    pub pending_comment_weight: i64,
    /// Bono si está en revisión: alguien espera mi respuesta.
    pub in_review_bonus: i64,
}

impl Default for WeightedPolicy {
    /// Los mismos valores que `config.toml` propone por defecto (§13.1).
    fn default() -> Self {
        Self {
            priority_weight: 10,
            overdue_bonus: 50,
            due_today_bonus: 30,
            due_this_week_bonus: 10,
            pending_comment_weight: 15,
            in_review_bonus: 5,
        }
    }
}

impl PrioritizationPolicy for WeightedPolicy {
    fn score(&self, task: &Task, ctx: &ScoringContext<'_>) -> i64 {
        let priority = self.priority_weight.saturating_mul(task.priority.value());

        let urgency = match Urgency::of(task, ctx) {
            Urgency::Overdue => self.overdue_bonus,
            Urgency::DueToday => self.due_today_bonus,
            Urgency::DueThisWeek => self.due_this_week_bonus,
            Urgency::Later | Urgency::NoDueDate => 0,
        };

        let pending = i64::try_from(task.pending_comments_for(ctx.me).count()).unwrap_or(i64::MAX);
        let comments = self.pending_comment_weight.saturating_mul(pending);

        let review = if task.status == Status::InReview {
            self.in_review_bonus
        } else {
            0
        };

        let project = ctx.projects.resolve(task).map_or(0, |p| p.priority_boost);

        [urgency, comments, review, project]
            .into_iter()
            .fold(priority, i64::saturating_add)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::MatchRule;
    use crate::task::{LocationLevel, Priority};
    use crate::testing::{at, comment, project, task, task_in, user};

    const NOW: &str = "2026-10-10T15:00:00Z";

    /// Policy where only the field under test has weight, so each test isolates one rule.
    fn only(f: impl FnOnce(&mut WeightedPolicy)) -> WeightedPolicy {
        let mut policy = WeightedPolicy {
            priority_weight: 0,
            overdue_bonus: 0,
            due_today_bonus: 0,
            due_this_week_bonus: 0,
            pending_comment_weight: 0,
            in_review_bonus: 0,
        };
        f(&mut policy);
        policy
    }

    fn ctx<'a>(me: &'a UserId, projects: &'a ProjectResolver) -> ScoringContext<'a> {
        ScoringContext {
            now: at(NOW),
            offset: FixedOffset::west_opt(5 * 3600).unwrap(), // Bogotá, UTC-5
            me,
            projects,
        }
    }

    fn due(t: &str) -> Task {
        let mut task = task("1");
        task.due = Some(at(t));
        task
    }

    #[test]
    fn priority_is_weighted() {
        let (me, projects) = (user("me"), ProjectResolver::default());
        let policy = only(|p| p.priority_weight = 10);
        let mut t = task("1");

        t.priority = Priority::Urgent;
        assert_eq!(policy.score(&t, &ctx(&me, &projects)), 40);
        t.priority = Priority::None;
        assert_eq!(policy.score(&t, &ctx(&me, &projects)), 0);
    }

    #[test]
    fn urgency_bonuses_are_exclusive() {
        let (me, projects) = (user("me"), ProjectResolver::default());
        let policy = WeightedPolicy {
            priority_weight: 0,
            ..WeightedPolicy::default()
        };
        let c = ctx(&me, &projects);

        assert_eq!(
            policy.score(&due("2026-10-10T14:00:00Z"), &c),
            50,
            "overdue"
        );
        assert_eq!(
            policy.score(&due("2026-10-10T20:00:00Z"), &c),
            30,
            "due today"
        );
        assert_eq!(
            policy.score(&due("2026-10-14T12:00:00Z"), &c),
            10,
            "this week"
        );
        assert_eq!(policy.score(&due("2026-10-30T12:00:00Z"), &c), 0, "later");
        assert_eq!(policy.score(&task("1"), &c), 0, "no due date");
    }

    #[test]
    fn due_today_uses_the_user_local_day() {
        let (me, projects) = (user("me"), ProjectResolver::default());
        let policy = only(|p| {
            p.due_today_bonus = 30;
            p.due_this_week_bonus = 10;
        });
        // 03:00 UTC del 11 = 22:00 del 10 en Bogotá: sigue siendo "hoy" para el usuario.
        let late_tonight = due("2026-10-11T03:00:00Z");
        assert_eq!(policy.score(&late_tonight, &ctx(&me, &projects)), 30);

        // 06:00 UTC del 11 = 01:00 del 11 en Bogotá: ya es mañana.
        let after_midnight = due("2026-10-11T06:00:00Z");
        assert_eq!(policy.score(&after_midnight, &ctx(&me, &projects)), 10);
    }

    #[test]
    fn pending_comments_for_me_add_weight() {
        let (me, projects) = (user("me"), ProjectResolver::default());
        let policy = only(|p| p.pending_comment_weight = 15);
        let mut t = task("1");
        t.comments = vec![
            comment(Some(&me), false),
            comment(Some(&me), false),
            comment(Some(&me), true),
            comment(Some(&user("other")), false),
        ];

        assert_eq!(policy.score(&t, &ctx(&me, &projects)), 30);
    }

    #[test]
    fn in_review_adds_bonus() {
        let (me, projects) = (user("me"), ProjectResolver::default());
        let policy = only(|p| p.in_review_bonus = 5);
        let mut t = task("1");
        t.status = Status::InReview;

        assert_eq!(policy.score(&t, &ctx(&me, &projects)), 5);
    }

    #[test]
    fn project_boost_applies_to_resolved_tasks() {
        let me = user("me");
        let mut acme = project(
            "acme",
            vec![MatchRule::Location {
                level: LocationLevel::Space,
                id: "S1".into(),
            }],
        );
        acme.priority_boost = 20;
        let projects = ProjectResolver::new(vec![acme]);
        let policy = only(|_| {});

        assert_eq!(
            policy.score(&task_in("1", "S1", "L1"), &ctx(&me, &projects)),
            20
        );
        assert_eq!(
            policy.score(&task_in("2", "S2", "L2"), &ctx(&me, &projects)),
            0
        );
    }

    #[test]
    fn rank_orders_by_score_and_keeps_ties_stable() {
        let (me, projects) = (user("me"), ProjectResolver::default());
        let policy = WeightedPolicy::default();

        let mut low = task("low");
        low.priority = Priority::Low;
        let mut tie_a = task("tie-a");
        tie_a.priority = Priority::Normal;
        let mut tie_b = task("tie-b");
        tie_b.priority = Priority::Normal;
        let mut overdue = task("overdue");
        overdue.priority = Priority::Low;
        overdue.due = Some(at("2026-10-09T12:00:00Z"));

        let mut tasks = vec![low, tie_a, tie_b, overdue];
        policy.rank(&mut tasks, &ctx(&me, &projects));

        let order: Vec<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(order, ["overdue", "tie-a", "tie-b", "low"]);
    }

    #[test]
    fn policy_works_as_a_trait_object() {
        let (me, projects) = (user("me"), ProjectResolver::default());
        let policy: Box<dyn PrioritizationPolicy> = Box::new(WeightedPolicy::default());
        let mut tasks = vec![task("a"), task("b")];

        policy.rank(&mut tasks, &ctx(&me, &projects));
        assert_eq!(tasks.len(), 2);
    }
}
