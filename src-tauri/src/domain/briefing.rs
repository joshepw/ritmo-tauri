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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailHint {
    pub id: String,
    pub from_name: String,
    pub subject: String,
    pub snippet: String,
}

pub struct DayContext<'a> {
    pub now: DateTime<Local>,
    pub events: &'a [CalendarEvent],
    pub tasks: &'a [TaskItem],
    pub notes: &'a [Note],
    pub mail: &'a [MailHint],
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
    let fresh = novel_mail(ctx);
    if !fresh.is_empty() {
        let subjects = fresh
            .iter()
            .take(3)
            .map(|mail| mail.subject.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!(
            "En el correo hay {} que no aparecen en la agenda ni en las tareas: {subjects}.",
            if fresh.len() == 1 { "algo" } else { "cosas" }
        ));
    }
    parts.join(" ")
}

pub fn suggestions(ctx: &DayContext<'_>, prefs: &Preferences) -> Vec<Suggestion> {
    if !prefs.assistant_suggest {
        return Vec::new();
    }
    novel_mail(ctx)
        .into_iter()
        .take(3)
        .map(|mail| Suggestion {
            icon: "✉".into(),
            bg: "rgba(255,255,255,.08)".into(),
            color: "#f5f5f7".into(),
            title: format!("Responder a {}", mail.from_name),
            when_label: mail.subject.clone(),
            cta: "Abrir".into(),
            action: "open-url".into(),
            target_id: format!("https://mail.google.com/mail/u/0/#inbox/{}", mail.id),
        })
        .collect()
}

pub fn keep_novel(items: Vec<Suggestion>, events: &[CalendarEvent], tasks: &[TaskItem]) -> Vec<Suggestion> {
    items
        .into_iter()
        .filter(|item| !already_shown(&item.title, events, tasks) && !already_shown(&item.when_label, events, tasks))
        .take(3)
        .collect()
}

/// Splits a Gemini reply into the paragraph and the new suggestions.
pub fn split_briefing(text: &str) -> (String, Vec<Suggestion>) {
    let mut summary = Vec::new();
    let mut items = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_matches('`');
        let Some(rest) = line.strip_prefix("SUGERENCIA|") else {
            if !line.is_empty() && !line.eq_ignore_ascii_case("json") {
                summary.push(line);
            }
            continue;
        };
        let mut parts = rest.splitn(3, '|');
        let title = parts.next().unwrap_or("").trim();
        let detail = parts.next().unwrap_or("").trim();
        let cta = parts.next().unwrap_or("Abrir").trim();
        if title.is_empty() {
            continue;
        }
        items.push(Suggestion {
            icon: "✉".into(),
            bg: "rgba(255,255,255,.08)".into(),
            color: "#f5f5f7".into(),
            title: title.to_string(),
            when_label: detail.to_string(),
            cta: if cta.is_empty() { "Abrir".into() } else { cta.to_string() },
            action: "open-mail".into(),
            target_id: format!(
                "https://mail.google.com/mail/u/0/#search/{}",
                urlencoding::encode(title)
            ),
        });
    }
    (summary.join(" "), items)
}

fn novel_mail<'a>(ctx: &'a DayContext<'_>) -> Vec<&'a MailHint> {
    ctx.mail
        .iter()
        .filter(|mail| !mail.subject.trim().is_empty())
        .filter(|mail| !already_shown(&mail.subject, ctx.events, ctx.tasks))
        .collect()
}

fn already_shown(text: &str, events: &[CalendarEvent], tasks: &[TaskItem]) -> bool {
    let text = text.trim().to_lowercase();
    if text.len() < 6 {
        return false;
    }
    events.iter().any(|event| overlaps(&text, &event.title))
        || tasks.iter().filter(|task| !task.done).any(|task| overlaps(&text, &task.title))
}

fn overlaps(left: &str, title: &str) -> bool {
    let title = title.trim().to_lowercase();
    title.len() >= 6 && (left.contains(&title) || title.contains(left))
}

pub fn assistant_prompt(ctx: &DayContext<'_>, prefs: &Preferences, question: &str) -> (String, String) {
    let system = "Eres el asistente de Ritmo. Hablas español neutro y tuteas. No uses markdown. No inventes reuniones, tareas ni correos.".into();
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
    let mail = ctx
        .mail
        .iter()
        .map(|item| format!("- De {}: {} — {}", item.from_name, item.subject, item.snippet))
        .collect::<Vec<_>>()
        .join("\n");
    let prompt = format!(
        "Contexto real de la cuenta:\n{summary}\nTareas ya visibles:\n{tasks}\nNotas:\n{notes}\nCorreo reciente:\n{mail}\n\n{question}"
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
        let mail = vec![MailHint {
            id: "msg-1".into(),
            from_name: "Contabilidad".into(),
            subject: "Factura de marzo".into(),
            snippet: "Queda pendiente el comprobante.".into(),
        }];
        let ctx = DayContext {
            now,
            events: &events,
            tasks: &tasks,
            notes: &[],
            mail: &mail,
            connected: true,
        };
        let text = local_summary(&ctx, &prefs);
        assert!(text.contains("Revisión de contratos"));
        assert!(text.contains("Factura de marzo"));
        assert!(!text.contains("28 de octubre"));
        let ideas = suggestions(&ctx, &prefs);
        assert!(ideas.iter().any(|item| item.when_label == "Factura de marzo"));
        assert!(ideas.iter().all(|item| !item.title.contains("Enviar minuta")));
        let (summary, parsed) = split_briefing("La jornada deja un hueco después de la revisión.\nSUGERENCIA|Pedir el comprobante|Factura de marzo|Abrir");
        assert!(summary.contains("hueco"));
        assert_eq!(parsed.len(), 1);
        assert!(keep_novel(parsed, &events, &tasks).len() == 1);
    }
}
