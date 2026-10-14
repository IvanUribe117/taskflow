//! Convierten datos del dominio en lo que se envía a la IA. Ya están hechas: son
//! conversiones campo por campo, como los DTO de las fases anteriores.

use chrono::{DateTime, FixedOffset, Utc};
use domain::task::{Comment, Task};

use crate::dto::TaskView;
use crate::ports::{PlanTask, TaskBrief};

/// `text` con como máximo `max` caracteres; si se recorta, termina en `…`.
pub(crate) fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(max.saturating_sub(1)).collect();
    short.push('…');
    short
}

/// `AAAA-MM-DD` en la zona horaria del usuario.
pub(crate) fn local_date(at: DateTime<Utc>, offset: FixedOffset) -> String {
    at.with_timezone(&offset).format("%Y-%m-%d").to_string()
}

/// Los datos de la tarea para la IA, con la descripción recortada.
pub(crate) fn task_brief(
    task: &Task,
    max_description_chars: usize,
    offset: FixedOffset,
) -> TaskBrief {
    TaskBrief {
        id: task.id.as_str().to_owned(),
        title: task.title.clone(),
        description: task
            .description
            .as_deref()
            .map(|text| truncate_chars(text, max_description_chars)),
        status: task.status.to_string(),
        priority: task.priority.to_string(),
        due: task.due.map(|due| local_date(due, offset)),
        url: task.url.clone(),
        tags: task.tags.clone(),
    }
}

/// Los últimos `max` comentarios, como `"Autor (AAAA-MM-DD): texto"`, en el mismo orden.
///
/// `&comments[inicio..]` es un *slice*: una vista de una parte del `Vec`, sin copiarlo.
/// `saturating_sub` evita restar por debajo de cero cuando hay menos de `max`.
pub(crate) fn recent_comments(
    comments: &[Comment],
    max: usize,
    offset: FixedOffset,
) -> Vec<String> {
    let start = comments.len().saturating_sub(max);
    comments[start..]
        .iter()
        .map(|c| {
            format!(
                "{} ({}): {}",
                c.author,
                local_date(c.created_at, offset),
                c.body
            )
        })
        .collect()
}

/// Una tarea de `today` para el plan, con el nombre de su proyecto.
pub(crate) fn plan_task(view: &TaskView, project: Option<&str>, offset: FixedOffset) -> PlanTask {
    PlanTask {
        id: view.id.clone(),
        title: view.title.clone(),
        project: project.map(str::to_owned),
        status: view.status.clone(),
        priority: view.priority.clone(),
        due: view.due.map(|due| local_date(due, offset)),
        overdue: view.overdue,
        pending_comments: view.pending_comments,
    }
}

#[cfg(test)]
mod tests {
    use domain::testing::{at, comment};

    use super::*;

    fn bogota() -> FixedOffset {
        FixedOffset::west_opt(5 * 3600).unwrap()
    }

    #[test]
    fn long_text_is_cut_by_characters() {
        assert_eq!(truncate_chars("ñandú", 10), "ñandú");
        assert_eq!(truncate_chars("ñandúes grandes", 6), "ñandú…");
    }

    #[test]
    fn recent_comments_keeps_the_last_ones() {
        let mut first = comment(None, false);
        first.body = "primero".into();
        let mut last = comment(None, false);
        last.body = "último".into();
        last.created_at = at("2026-10-10T03:00:00Z");

        let lines = recent_comments(&[first, last.clone()], 1, bogota());

        assert_eq!(lines, ["someone (2026-10-09): último"]);
        assert_eq!(recent_comments(&[last], 5, bogota()).len(), 1);
    }
}
