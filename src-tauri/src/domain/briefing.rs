use chrono::{DateTime, Local};

use crate::domain::preferences::Preferences;
use crate::domain::records::{build_agenda, CalendarEvent, Note, TaskItem};
use crate::domain::timefmt::format_countdown;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub icon: String,
    pub bg: String,
    pub color: String,
    pub title: String,
    pub when_label: String,
    pub cta: String,
    pub action: String,
    pub target_id: String,
}

pub struct DayContext<'a> {
    pub now: DateTime<Local>,
    pub events: &'a [CalendarEvent],
    pub tasks: &'a [TaskItem],
    pub notes: &'a [Note],
    pub connected: bool,
}

pub fn local_summary(ctx: &DayContext<'_>, prefs: &Preferences) -> String {
    let agenda = build_agenda(ctx.now, ctx.events, prefs, ctx.connected);
    let open: Vec<&TaskItem> = ctx.tasks.iter().filter(|task| !task.done).collect();
    let late = open
        .iter()
        .filter(|task| task.due.is_some_and(|due| due < ctx.now.date_naive()))
        .count();
    let mut parts = Vec::new();
    if agenda.next.join_url.is_empty() && agenda.next.event_id.is_empty() {
        parts.push("No tienes otra reunión en el rango.".to_string());
    } else {
        parts.push(format!(
            "Lo siguiente es {} ({}).",
            agenda.next.title, agenda.next.meta
        ));
    }
    if let Some(gap) = free_gap_minutes(ctx.now, ctx.events) {
        if gap >= i64::from(prefs.focus_minutes) {
            parts.push(format!(
                "Hay un hueco de {} antes de la próxima reunión.",
                format_countdown(gap)
            ));
        }
    }
    if late > 0 {
        parts.push(format!(
            "{late} {} atrasada{}.",
            if late == 1 { "tarea" } else { "tareas" },
            if late == 1 { "" } else { "s" }
        ));
    } else if open.is_empty() {
        parts.push("No tienes tareas pendientes.".into());
    } else {
        parts.push(format!(
            "{} {} pendiente{}.",
            open.len(),
            if open.len() == 1 { "tarea" } else { "tareas" },
            if open.len() == 1 { "" } else { "s" }
        ));
    }
    parts.join(" ")
}

pub fn suggestions(ctx: &DayContext<'_>, prefs: &Preferences) -> Vec<Suggestion> {
    if !prefs.assistant_suggest {
        return Vec::new();
    }
    let mut items = Vec::new();
    let open: Vec<&TaskItem> = ctx.tasks.iter().filter(|task| !task.done).collect();
    if let Some(gap) = free_gap_minutes(ctx.now, ctx.events) {
        if gap >= i64::from(prefs.focus_minutes) {
            if let Some(task) = open.first() {
                items.push(Suggestion {
                    icon: "◎".into(),
                    bg: "rgba(255,69,58,.2)".into(),
                    color: "#ff453a".into(),
                    title: format!("Pomodoro de {} min", prefs.focus_minutes),
                    when_label: format!("Para {}", task.title),
                    cta: "Iniciar".into(),
                    action: "start-pomodoro".into(),
                    target_id: task.id.clone(),
                });
            }
        }
    }
    let agenda = build_agenda(ctx.now, ctx.events, prefs, ctx.connected);
    if !agenda.next.event_id.is_empty() {
        items.push(Suggestion {
            icon: "▤".into(),
            bg: "rgba(10,132,255,.2)".into(),
            color: "#0a84ff".into(),
            title: format!("Preparar {}", agenda.next.title),
            when_label: agenda.next.meta.clone(),
            cta: "Notas".into(),
            action: "open-notes".into(),
            target_id: String::new(),
        });
    }
    if let Some(late) = open
        .iter()
        .filter(|task| task.due.is_some_and(|due| due < ctx.now.date_naive()))
        .min_by_key(|task| task.due)
    {
        items.push(Suggestion {
            icon: "!".into(),
            bg: "rgba(255,159,10,.2)".into(),
            color: "#ff9f0a".into(),
            title: late.title.clone(),
            when_label: "Tarea atrasada".into(),
            cta: "Enfocar".into(),
            action: "focus-task".into(),
            target_id: late.id.clone(),
        });
    }
    let _ = ctx.notes;
    items.truncate(3);
    items
}

pub fn assistant_prompt(ctx: &DayContext<'_>, prefs: &Preferences, question: &str) -> (String, String) {
    let system = "Eres Gemini, asistente de oficina. Hablas español neutro, tuteando, conciso. No uses markdown.".to_string();
    let summary = local_summary(ctx, prefs);
    let tasks = ctx
        .tasks
        .iter()
        .filter(|task| !task.done)
        .map(|task| format!("- {}", task.title))
        .collect::<Vec<_>>()
        .join("\n");
    let mut notes = String::new();
    if prefs.assistant_reads_notes {
        notes = ctx
            .notes
            .iter()
            .map(|note| format!("- {}: {}", note.title, note.body))
            .collect::<Vec<_>>()
            .join("\n");
    }
    let prompt = format!(
        "Contexto real de la cuenta:\n{summary}\nTareas:\n{tasks}\nNotas:\n{notes}\n\n{question}"
    );
    (system, prompt)
}

fn free_gap_minutes(now: DateTime<Local>, events: &[CalendarEvent]) -> Option<i64> {
    let next = events
        .iter()
        .filter(|event| !event.all_day && !event.declined && event.start > now)
        .min_by_key(|event| event.start)?;
    Some((next.start - now).num_minutes())
}

pub fn should_prepare_summary(now: DateTime<Local>, prefs: &Preferences) -> bool {
    if !prefs.assistant_daily || !prefs.service_gemini {
        return false;
    }
    match prefs.assistant_when.as_str() {
        "8:00" => now.time() >= chrono::NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
        "9:00" => now.time() >= chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::records::CalendarEvent;
    use chrono::{Duration, TimeZone};

    #[test]
    fn summary_uses_the_real_event_title() {
        let now = Local.with_ymd_and_hms(2026, 10, 29, 13, 0, 0).unwrap();
        let start = now + Duration::minutes(30);
        let events = vec![CalendarEvent {
            id: "1".into(),
            calendar_id: "work".into(),
            calendar_color: "#0a84ff".into(),
            title: "Revisión de contratos".into(),
            start,
            end: start + Duration::minutes(45),
            all_day: false,
            location: "https://meet.google.com/abc-defg-hij".into(),
            description: String::new(),
            declined: false,
            conference_url: String::new(),
            event_type: String::new(),
        }];
        let tasks = vec![TaskItem {
            id: "t".into(),
            title: "Enviar minuta".into(),
            due: Some(now.date_naive() - Duration::days(2)),
            done: false,
            list_id: "@default".into(),
            updated_at: now,
            completed_at: None,
        }];
        let prefs = Preferences::default();
        let ctx = DayContext {
            now,
            events: &events,
            tasks: &tasks,
            notes: &[],
            connected: true,
        };
        let text = local_summary(&ctx, &prefs);
        assert!(text.contains("Revisión de contratos"));
        assert!(!text.contains("28 de octubre"));
        assert!(!text.contains("Place2Pay"));
        let ideas = suggestions(&ctx, &prefs);
        assert!(ideas.iter().any(|item| item.target_id == "t"));
    }
}
