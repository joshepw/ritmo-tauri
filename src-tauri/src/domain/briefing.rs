use chrono::{DateTime, Local};

use crate::domain::preferences::Preferences;
use crate::domain::records::{build_agenda, event_link, CalendarEvent, Note, TaskItem};
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
    pub compact: bool,
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
    pub factorial: &'a [CalendarEvent],
    pub clickup: &'a [TaskItem],
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
    let mut items = Vec::new();
    if let Some(item) = calendar_insight(ctx, prefs) {
        items.push(item);
    }
    items.extend(factorial_today(ctx));
    items.extend(clickup_focus(ctx));
    if let Some(item) = task_focus(ctx) {
        items.push(item);
    }
    items.extend(novel_mail(ctx).into_iter().take(1).map(|mail| Suggestion {
        icon: "✉".into(),
        bg: "rgba(255,255,255,.08)".into(),
        color: "#f5f5f7".into(),
        title: format!("Responder a {}", mail.from_name),
        when_label: mail.subject.clone(),
        cta: "Abrir".into(),
        action: "open-url".into(),
        target_id: format!("https://mail.google.com/mail/u/0/#inbox/{}", mail.id),
        compact: false,
    }));
    items.truncate(6);
    items.extend(factorial_ahead(ctx));
    items
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
            compact: false,
        });
    }
    (tighten_summary(&summary.join("\n")), items)
}

/// One paragraph, at most four concrete sentences. Vague lines stay out.
pub fn tighten_summary(text: &str) -> String {
    let mut kept = Vec::new();
    for sentence in brief_sentences(text) {
        if kept.len() == 4 {
            break;
        }
        if brief_sentence_helps(&sentence) {
            kept.push(sentence);
        }
    }
    kept.join(" ")
}

fn brief_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches(['#', '-', '*', '•']).trim();
        if line.is_empty() || line.eq_ignore_ascii_case("json") {
            continue;
        }
        let mut current = String::new();
        for ch in line.chars() {
            current.push(ch);
            if matches!(ch, '.' | '!' | '?') {
                let sentence = current.trim().to_string();
                if !sentence.is_empty() {
                    out.push(sentence);
                }
                current.clear();
            }
        }
        let rest = current.trim();
        if !rest.is_empty() {
            out.push(rest.to_string());
        }
    }
    out
}

fn brief_sentence_helps(sentence: &str) -> bool {
    let lower = sentence.to_lowercase();
    let filler = [
        "aprovecha",
        "organiza tu",
        "mantente",
        "mantén",
        "manten el",
        "productiv",
        "en general",
        "recuerda",
        "es importante",
        "podrías",
        "podrias",
        "sería bueno",
        "seria bueno",
        "sería ideal",
        "seria ideal",
        "considera",
        "ten en cuenta",
        "buen día",
        "buen dia",
        "al máximo",
        "al maximo",
        "día ocupado",
        "dia ocupado",
        "prioriza",
        "no olvides",
        "te recomiendo",
        "éxito",
        "exito",
        "quizá",
        "quizas",
        "tal vez",
        "puede que",
        "podría",
        "podria",
        "parece que",
        "en algún momento",
        "en algun momento",
        "cuando puedas",
        "si puedes",
        "hay cosas",
        "varias cosas",
        "algunas cosas",
    ];
    let vague = filler.iter().any(|phrase| lower.contains(phrase));
    let concrete = sentence.chars().any(|ch| ch.is_ascii_digit());
    if vague && !concrete {
        return false;
    }
    sentence.chars().count() >= 12
}

fn calendar_insight(ctx: &DayContext<'_>, prefs: &Preferences) -> Option<Suggestion> {
    let next = ctx.events.iter().filter(|event| {
        !event.all_day && !event.declined && event.event_type != "factorial" && event.end > ctx.now
    }).min_by_key(|event| event.start)?;
    let in_progress = next.start <= ctx.now;
    if !in_progress && next.start.date_naive() > ctx.now.date_naive() {
        return None;
    }
    let mins = if in_progress { 0 } else { (next.start - ctx.now).num_minutes().max(0) };
    if !in_progress && mins >= i64::from(prefs.focus_minutes) {
        if let Some(gap) = free_gap_minutes(ctx.now, ctx.events) {
            if gap >= i64::from(prefs.focus_minutes) {
                return Some(Suggestion {
                    icon: "◎".into(),
                    bg: "rgba(10,132,255,.18)".into(),
                    color: "#64b0ff".into(),
                    title: format!("Tienes un hueco de {}", format_countdown(gap)),
                    when_label: format!("Antes de {}", next.title),
                    cta: "Enfocar".into(),
                    action: "focus".into(),
                    target_id: String::new(),
                    compact: false,
                });
            }
        }
    }
    let link = event_link(next, prefs.detect_in_description);
    let can_join = !link.url.is_empty();
    Some(Suggestion {
        icon: "↗".into(),
        bg: "rgba(10,132,255,.18)".into(),
        color: "#64b0ff".into(),
        title: if in_progress {
            format!("Estás en {}", next.title)
        } else {
            format!("Prepárate para {}", next.title)
        },
        when_label: if in_progress { "Ahora".into() } else { format!("En {}", format_countdown(mins)) },
        cta: if can_join { "Unirse".into() } else { String::new() },
        action: if can_join { "open-url".into() } else { "none".into() },
        target_id: if can_join { link.url } else { String::new() },
        compact: false,
    })
}

fn factorial_today(ctx: &DayContext<'_>) -> Vec<Suggestion> {
    let today = ctx.now.date_naive();
    ctx.factorial
        .iter()
        .filter(|event| event.start.date_naive() <= today && event.end.date_naive() > today)
        .take(2)
        .map(|event| {
            let birthday = event.id.contains("birthday") || event.description.eq_ignore_ascii_case("Cumpleaños");
            Suggestion {
                icon: if birthday { "★".into() } else { "–".into() },
                bg: if birthday { "rgba(255,159,10,.18)".into() } else { "rgba(255,90,54,.16)".into() },
                color: if birthday { "#ff9f0a".into() } else { "#ff5a36".into() },
                title: event.title.clone(),
                when_label: if birthday {
                    "Cumpleaños · hoy".into()
                } else if event.description.is_empty() {
                    "Ausencia · hoy".into()
                } else {
                    format!("{} · hoy", event.description)
                },
                cta: String::new(),
                action: "none".into(),
                target_id: String::new(),
                compact: false,
            }
        })
        .collect()
}

fn clickup_focus(ctx: &DayContext<'_>) -> Vec<Suggestion> {
    let today = ctx.now.date_naive();
    let open: Vec<&TaskItem> = ctx.clickup.iter().filter(|task| !task.done).collect();
    let mut ranked: Vec<&TaskItem> = open.iter().copied().filter(|task| priority_rank(&task.priority).is_some()).collect();
    ranked.sort_by_key(|task| priority_rank(&task.priority).unwrap_or(9));
    if !ranked.is_empty() {
        return ranked.into_iter().take(2).map(|task| priority_suggestion(task, today)).collect();
    }
    attention_task(&open, today).map(|task| focus_suggestion(task, "Presta atención a", today)).into_iter().collect()
}

fn task_focus(ctx: &DayContext<'_>) -> Option<Suggestion> {
    let today = ctx.now.date_naive();
    let open: Vec<&TaskItem> = ctx.tasks.iter().filter(|task| !task.done).collect();
    let task = attention_task(&open, today)?;
    Some(focus_suggestion(task, "Presta atención a", today))
}

fn priority_rank(priority: &str) -> Option<u8> {
    match priority {
        "urgent" => Some(0),
        "high" => Some(1),
        _ => None,
    }
}

fn attention_task<'a>(tasks: &[&'a TaskItem], today: chrono::NaiveDate) -> Option<&'a TaskItem> {
    tasks.iter().copied().filter(|task| task.due.is_some()).min_by_key(|task| {
        let due = task.due.unwrap_or(today);
        (if due < today { 0 } else if due == today { 1 } else { 2 }, due)
    })
}

fn priority_suggestion(task: &TaskItem, today: chrono::NaiveDate) -> Suggestion {
    let label = if task.priority == "urgent" { "Urgente" } else { "Prioridad alta" };
    let mut item = focus_suggestion(task, label, today);
    if !task.status.is_empty() {
        item.when_label = task.status.clone();
    }
    item
}

fn focus_suggestion(task: &TaskItem, label: &str, today: chrono::NaiveDate) -> Suggestion {
    Suggestion {
        icon: "!".into(),
        bg: "rgba(255,90,54,.16)".into(),
        color: "#ff5a36".into(),
        title: format!("{label}: {}", task.title),
        when_label: task_when(task, today),
        cta: "Enfocar".into(),
        action: "focus".into(),
        target_id: task.id.clone(),
        compact: false,
    }
}

fn factorial_ahead(ctx: &DayContext<'_>) -> Vec<Suggestion> {
    use std::collections::HashSet;
    let today = ctx.now.date_naive();
    let until = today + chrono::Duration::days(15);
    let mut upcoming: Vec<&CalendarEvent> = ctx
        .factorial
        .iter()
        .filter(|event| {
            let day = event.start.date_naive();
            day > today && day <= until
        })
        .collect();
    upcoming.sort_by_key(|event| event.start);
    let mut seen = HashSet::new();
    let mut picked = Vec::new();
    for event in upcoming {
        if !seen.insert(factorial_key(event)) {
            continue;
        }
        picked.push(event);
        if picked.len() == 4 {
            break;
        }
    }
    picked
        .into_iter()
        .map(|event| {
            let birthday = event.id.contains("birthday") || event.description.eq_ignore_ascii_case("Cumpleaños");
            let day = event.start.date_naive();
            let days = (day - today).num_days();
            let when = if days == 1 { "Mañana".to_string() } else { format!("En {days} días") };
            Suggestion {
                icon: if birthday { "★".into() } else { "–".into() },
                bg: "transparent".into(),
                color: "rgba(235,235,245,.55)".into(),
                title: event.title.clone(),
                when_label: if birthday {
                    format!("Cumpleaños · {when}")
                } else if event.description.is_empty() {
                    format!("Ausencia · {when}")
                } else {
                    format!("{} · {when}", event.description)
                },
                cta: String::new(),
                action: "none".into(),
                target_id: String::new(),
                compact: true,
            }
        })
        .collect()
}

fn factorial_key(event: &CalendarEvent) -> String {
    let mut parts = event.id.split(':');
    let kind = parts.nth(1).unwrap_or("dia");
    let id = parts.next().unwrap_or(event.id.as_str());
    format!("{kind}:{id}")
}

fn task_when(task: &TaskItem, today: chrono::NaiveDate) -> String {
    match task.due {
        Some(due) if due < today => "Atrasada".into(),
        Some(due) if due == today => "Vence hoy".into(),
        Some(due) => due.format("%d/%m").to_string(),
        None => if task.status.is_empty() { "Sin fecha".into() } else { task.status.clone() },
    }
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
            clickup_list: String::new(),
            clickup_group: String::new(),
            status: String::new(),
            status_color: String::new(),
            priority: String::new(),
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
            factorial: &[],
            clickup: &[],
            connected: true,
        };
        let text = local_summary(&ctx, &prefs);
        assert!(text.contains("Revisión de contratos"));
        assert!(text.contains("Factura de marzo"));
        assert!(!text.contains("28 de octubre"));
        let ideas = suggestions(&ctx, &prefs);
        assert!(ideas.iter().any(|item| item.when_label == "Factura de marzo"));
        assert!(ideas.iter().any(|item| item.title.contains("Enviar minuta")));
        let (summary, parsed) = split_briefing("La jornada deja un hueco después de la revisión.\nSUGERENCIA|Pedir el comprobante|Factura de marzo|Abrir");
        assert!(summary.contains("hueco"));
        assert_eq!(parsed.len(), 1);
        assert!(keep_novel(parsed, &events, &tasks).len() == 1);
    }

    #[test]
    fn insights_use_todays_factorial_and_urgent_clickup() {
        let now = Local.with_ymd_and_hms(2026, 11, 5, 9, 0, 0).unwrap();
        let day = now.date_naive();
        let start = day.and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Local).earliest().unwrap();
        let end = (day + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Local).earliest().unwrap();
        let birthday = CalendarEvent {
            id: "factorial:birthday:2:2026-11-05".into(),
            calendar_id: "factorial".into(),
            calendar_color: "#ff5a36".into(),
            title: "Cumpleaños de Ana".into(),
            start,
            end,
            all_day: true,
            location: String::new(),
            description: "Cumpleaños".into(),
            declined: false,
            conference_url: String::new(),
            event_type: "factorial".into(),
        };
        let urgent = TaskItem {
            id: "clickup:1".into(),
            title: "Arreglar el pago".into(),
            due: None,
            done: false,
            list_id: "Sitio".into(),
            clickup_list: "44".into(),
            clickup_group: "folder:7".into(),
            status: "en curso".into(),
            status_color: "#f8ae00".into(),
            priority: "urgent".into(),
            updated_at: now,
            completed_at: None,
        };
        let soon_day = day + Duration::days(10);
        let soon = CalendarEvent {
            id: format!("factorial:birthday:3:{soon_day}"),
            title: "Cumpleaños de Luis".into(),
            start: soon_day.and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Local).earliest().unwrap(),
            end: (soon_day + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Local).earliest().unwrap(),
            description: "Cumpleaños".into(),
            ..birthday.clone()
        };
        let later_day = day + Duration::days(20);
        let later = CalendarEvent {
            id: format!("factorial:birthday:4:{later_day}"),
            title: "Cumpleaños de Marta".into(),
            start: later_day.and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Local).earliest().unwrap(),
            end: (later_day + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Local).earliest().unwrap(),
            ..soon.clone()
        };
        let leave_day = day + Duration::days(2);
        let leave = CalendarEvent {
            id: format!("factorial:leave:9:{leave_day}"),
            title: "Ana".into(),
            description: "Vacaciones".into(),
            start: leave_day.and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Local).earliest().unwrap(),
            end: (leave_day + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Local).earliest().unwrap(),
            ..birthday.clone()
        };
        let factorial = [birthday, soon, later, leave];
        let ctx = DayContext {
            now,
            events: &[],
            tasks: &[],
            notes: &[],
            mail: &[],
            factorial: &factorial,
            clickup: &[urgent],
            connected: true,
        };
        let ideas = suggestions(&ctx, &Preferences::default());
        assert!(ideas.iter().any(|item| item.title == "Cumpleaños de Ana" && !item.compact));
        assert!(ideas.iter().any(|item| item.title == "Cumpleaños de Luis" && item.compact && item.when_label == "Cumpleaños · En 10 días"));
        assert!(ideas.iter().any(|item| item.title == "Ana" && item.compact && item.when_label == "Vacaciones · En 2 días"));
        assert!(ideas.iter().all(|item| item.title != "Cumpleaños de Marta"));
        assert!(ideas.iter().any(|item| item.title == "Urgente: Arreglar el pago" && item.when_label == "en curso"));
    }

    #[test]
    fn the_day_text_is_one_concrete_paragraph() {
        let text = "Hoy será un día productivo. Aprovecha para organizarte.\n\nLa revisión empieza en 30 minutos. Hay un hueco de 2 h antes.\n\nRecuerda priorizar lo importante. Quizá haya cosas por hacer.\n\nLa minuta sigue atrasada. Queda 1 correo de Contabilidad.\n\nOtra frase de más que no cabe.";
        let tight = tighten_summary(text);
        assert!(tight.contains("30 minutos"));
        assert!(tight.contains("hueco de 2 h"));
        assert!(tight.contains("minuta sigue atrasada"));
        assert!(!tight.to_lowercase().contains("productivo"));
        assert!(!tight.to_lowercase().contains("priorizar"));
        assert!(!tight.to_lowercase().contains("quizá"));
        assert!(!tight.contains('\n'));
        assert!(!tight.contains("no cabe"));
    }
}
