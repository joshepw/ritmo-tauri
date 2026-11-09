use std::cmp::Reverse;
use std::collections::HashMap;

use chrono::{DateTime, Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::domain::meeting::{detect_meeting, MeetingKind, MeetingLink};
use crate::domain::preferences::{within_meeting_window, Preferences};
use crate::domain::timefmt::{
    format_clock, format_countdown, format_day_heading, format_event_clock, format_relative_day,
    format_tray_countdown,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub id: String,
    pub calendar_id: String,
    pub calendar_color: String,
    pub title: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub all_day: bool,
    pub location: String,
    pub description: String,
    pub declined: bool,
    #[serde(default)]
    pub conference_url: String,
    #[serde(default)]
    pub event_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EventChange {
    pub event: CalendarEvent,
    pub deleted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskItem {
    pub id: String,
    pub title: String,
    pub due: Option<NaiveDate>,
    pub done: bool,
    pub list_id: String,
    #[serde(default = "unknown_task_time")]
    pub updated_at: DateTime<Local>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Local>>,
}

fn unknown_task_time() -> DateTime<Local> {
    DateTime::UNIX_EPOCH.with_timezone(&Local)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
    pub updated_at: DateTime<Local>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkLog {
    pub started_at: DateTime<Local>,
    pub ended_at: DateTime<Local>,
    #[serde(default)]
    pub task_id: Option<String>,
    pub factorial: bool,
    pub clickup: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub name: String,
    pub email: String,
    pub scopes: Vec<String>,
    pub last_sync: Option<DateTime<Local>>,
}

pub fn apply_page(
    cache: &mut Vec<CalendarEvent>,
    calendar_id: &str,
    incremental: bool,
    changes: Vec<EventChange>,
) {
    if incremental {
        for change in changes {
            cache.retain(|event| event.id != change.event.id);
            if !change.deleted {
                cache.push(change.event);
            }
        }
        return;
    }
    cache.retain(|event| event.calendar_id != calendar_id);
    cache.extend(changes.into_iter().filter(|change| !change.deleted).map(|change| change.event));
}

pub fn sort_tasks(tasks: &mut [TaskItem]) {
    tasks.sort_by(|left, right| task_rank(left).cmp(&task_rank(right)));
}

fn task_rank(task: &TaskItem) -> (u8, Reverse<DateTime<Local>>, &str) {
    let group = if task.done { 1 } else { 0 };
    (group, Reverse(task.updated_at), task.title.as_str())
}

pub fn task_done_today(today: NaiveDate, task: &TaskItem) -> bool {
    if !task.done {
        return false;
    }
    task.completed_at.unwrap_or(task.updated_at).date_naive() == today
}

pub fn task_due_label(today: NaiveDate, due: Option<NaiveDate>, done: bool) -> (String, bool) {
    if done {
        return (String::new(), false);
    }
    let Some(due) = due else {
        return (String::new(), false);
    };
    let late = due < today;
    (format_relative_day(today, due), late)
}

pub fn create_note(title: String, body: String, now: DateTime<Local>) -> Note {
    Note {
        id: format!("note-{}", now.timestamp_millis()),
        title: if title.trim().is_empty() {
            "Nota".into()
        } else {
            title.trim().to_string()
        },
        body: body.trim().to_string(),
        updated_at: now,
    }
}

pub fn event_link(event: &CalendarEvent, detect_description: bool) -> MeetingLink {
    let conference = detect_meeting(event.conference_url.trim());
    if conference.kind == MeetingKind::Video || conference.kind == MeetingKind::Phone {
        return conference;
    }
    let mut text = event.location.clone();
    if detect_description {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&event.description);
    }
    if text.trim().is_empty() {
        return MeetingLink::none();
    }
    detect_meeting(&text)
}

fn is_working_location(event: &CalendarEvent) -> bool {
    if event.event_type == "workingLocation" {
        return true;
    }
    // Copies saved before the event type was stored. Google names these Office or Home
    // and leaves the location and the description empty.
    event.event_type.is_empty()
        && event.location.trim().is_empty()
        && event.description.trim().is_empty()
        && event.conference_url.trim().is_empty()
        && matches!(event.title.trim(), "Office" | "Home" | "Oficina" | "Casa")
}

#[derive(Clone, Debug)]
pub struct TodayRow {
    pub is_now: bool,
    pub is_event: bool,
    pub time: String,
    pub title: String,
    pub past: bool,
    pub color: String,
    pub prov_name: String,
    pub prov_short: String,
    pub prov_abbr: String,
    pub prov_color: String,
    pub meta: String,
    pub can_join: bool,
    pub join_url: String,
    pub live: bool,
}

#[derive(Clone, Debug)]
pub struct DayGroup {
    pub label: String,
    pub events: Vec<TodayRow>,
}

#[derive(Clone, Debug)]
pub struct NextMeeting {
    pub title: String,
    pub meta: String,
    pub color: String,
    pub abbr: String,
    pub join_url: String,
    pub starts_at: DateTime<Local>,
    pub ends_at: DateTime<Local>,
    pub event_id: String,
    pub tray_label: String,
}

#[derive(Clone, Debug)]
pub struct AgendaView {
    pub today: Vec<TodayRow>,
    pub days: Vec<DayGroup>,
    pub next: NextMeeting,
}

pub fn build_agenda(
    now: DateTime<Local>,
    events: &[CalendarEvent],
    prefs: &Preferences,
    connected: bool,
) -> AgendaView {
    build_agenda_with(now, events, prefs, connected, &HashMap::new(), false)
}

pub fn build_agenda_with(
    now: DateTime<Local>,
    events: &[CalendarEvent],
    prefs: &Preferences,
    connected: bool,
    links: &HashMap<String, MeetingLink>,
    links_pending: bool,
) -> AgendaView {
    let today = now.date_naive();
    let horizon = today + Duration::days(i64::from(prefs.range_days));
    let mut visible: Vec<&CalendarEvent> = events
        .iter()
        .filter(|event| !is_working_location(event))
        .filter(|event| !prefs.hide_declined || !event.declined)
        .filter(|event| event.end.date_naive() >= today && event.start.date_naive() <= horizon)
        .collect();
    visible.sort_by_key(|event| (event.start, event.id.clone()));

    let today_events: Vec<&CalendarEvent> = visible
        .iter()
        .copied()
        .filter(|event| event.start.date_naive() == today || (event.start < now && event.end > now))
        .filter(|event| event.start.date_naive() <= today && event.end.date_naive() >= today)
        .collect();

    let mut past: Vec<&CalendarEvent> = today_events
        .iter()
        .copied()
        .filter(|event| !event.all_day && event.end <= now)
        .collect();
    let upcoming_today: Vec<&CalendarEvent> = today_events
        .iter()
        .copied()
        .filter(|event| event.all_day || event.end > now)
        .collect();
    if past.len() > 1 {
        past = past.split_off(past.len() - 1);
    }

    let mut rows = Vec::new();
    for event in &past {
        rows.push(present_event(event, prefs, true, links, links_pending));
    }
    let live_id = upcoming_today
        .iter()
        .copied()
        .rev()
        .find(|event| {
            !event.all_day
                && event.event_type != "factorial"
                && event.start <= now
                && event.end > now
        })
        .map(|event| event.id.clone());
    if live_id.is_none() && !today_events.is_empty() {
        rows.push(TodayRow {
            is_now: true,
            is_event: false,
            time: format_clock(now.time()),
            title: String::new(),
            past: false,
            color: "#ff453a".into(),
            prov_name: String::new(),
            prov_short: String::new(),
            prov_abbr: String::new(),
            prov_color: String::new(),
            meta: String::new(),
            can_join: false,
            join_url: String::new(),
            live: false,
        });
    }
    for event in &upcoming_today {
        let mut row = present_event(event, prefs, false, links, links_pending);
        row.live = live_id.as_ref() == Some(&event.id);
        rows.push(row);
    }

    let mut groups: Vec<DayGroup> = Vec::new();
    let mut cursor = today + Duration::days(1);
    while cursor <= horizon {
        let day_events: Vec<TodayRow> = visible
            .iter()
            .copied()
            .filter(|event| event.start.date_naive() == cursor)
            .map(|event| present_event(event, prefs, false, links, links_pending))
            .collect();
        if !day_events.is_empty() {
            groups.push(DayGroup {
                label: format_day_heading(today, cursor),
                events: day_events,
            });
        }
        cursor += Duration::days(1);
    }

    let next_event = visible.iter().copied().find(|event| {
        !event.all_day
            && !event.declined
            && event.end > now
            && event.event_type != "factorial"
    });
    let next = match next_event {
        Some(event) => present_next(now, event, prefs, links, links_pending),
        None => NextMeeting {
            title: if connected {
                "Sin reunión próxima".into()
            } else {
                "Conecta tu cuenta".into()
            },
            meta: if connected {
                "No hay eventos en el rango".into()
            } else {
                "Google Calendar".into()
            },
            color: "rgba(255,255,255,.18)".into(),
            abbr: "·".into(),
            join_url: String::new(),
            starts_at: now,
            ends_at: now,
            event_id: String::new(),
            tray_label: String::new(),
        },
    };

    AgendaView {
        today: rows,
        days: groups,
        next,
    }
}

fn resolved_link(
    event: &CalendarEvent,
    prefs: &Preferences,
    links: &HashMap<String, MeetingLink>,
    links_pending: bool,
) -> MeetingLink {
    if let Some(link) = links.get(&event.id) {
        return link.clone();
    }
    event_link(event, !links_pending && prefs.detect_in_description)
}

fn present_event(
    event: &CalendarEvent,
    prefs: &Preferences,
    past: bool,
    links: &HashMap<String, MeetingLink>,
    links_pending: bool,
) -> TodayRow {
    if event.event_type == "factorial" {
        return TodayRow {
            is_now: false,
            is_event: true,
            time: "Día".into(),
            title: event.title.clone(),
            past,
            color: "#ff5a36".into(),
            prov_name: "Factorial".into(),
            prov_short: "Factorial".into(),
            prov_abbr: "F".into(),
            prov_color: "#ff5a36".into(),
            meta: if event.description.is_empty() {
                "Factorial".into()
            } else {
                event.description.clone()
            },
            can_join: false,
            join_url: String::new(),
            live: false,
        };
    }
    let link = resolved_link(event, prefs, links, links_pending);
    let time = if event.all_day {
        "Día".into()
    } else {
        format_event_clock(event.start.time())
    };
    let duration = if event.all_day {
        String::new()
    } else {
        let mins = (event.end - event.start).num_minutes().max(0);
        if mins >= 60 && mins % 60 == 0 {
            format!("{} h", mins / 60)
        } else if mins > 0 {
            format!("{mins} min")
        } else {
            String::new()
        }
    };
    let meta = match (link.kind, duration.is_empty()) {
        (MeetingKind::None, true) => String::new(),
        (MeetingKind::None, false) => duration,
        (_, true) => link.name.clone(),
        (_, false) => format!("{} · {duration}", link.name),
    };
    let color = if link.kind == MeetingKind::None {
        if event.calendar_color.is_empty() {
            "#ff9f0a".into()
        } else {
            event.calendar_color.clone()
        }
    } else {
        link.color.clone()
    };
    TodayRow {
        is_now: false,
        is_event: true,
        time,
        title: event.title.clone(),
        past,
        color: color.clone(),
        prov_name: link.name.clone(),
        prov_short: link.short.clone(),
        prov_abbr: link.abbr.clone(),
        prov_color: link.color,
        meta,
        can_join: !past && link.kind != MeetingKind::None && link.kind != MeetingKind::Room,
        join_url: link.url,
        live: false,
    }
}

fn present_next(
    now: DateTime<Local>,
    event: &CalendarEvent,
    prefs: &Preferences,
    links: &HashMap<String, MeetingLink>,
    links_pending: bool,
) -> NextMeeting {
    let link = resolved_link(event, prefs, links, links_pending);
    let in_progress = event.start <= now && event.end > now;
    let mins = if in_progress {
        0
    } else {
        (event.start - now).num_minutes().max(0)
    };
    let countdown = format_countdown(mins);
    let tray_label = if in_progress {
        "Ahora".into()
    } else if within_meeting_window(now, event.start, prefs.meeting_window) {
        format_tray_countdown(mins)
    } else {
        String::new()
    };
    let mut meta = if in_progress || mins <= 0 {
        "Ahora".to_string()
    } else {
        format!("En {countdown}")
    };
    if link.kind != MeetingKind::None {
        meta.push_str(" · ");
        meta.push_str(&link.name);
    }
    let color = if link.kind == MeetingKind::None {
        "#ff9f0a".into()
    } else {
        link.color.clone()
    };
    NextMeeting {
        title: event.title.clone(),
        meta,
        color,
        abbr: link.abbr,
        join_url: link.url,
        starts_at: event.start,
        ends_at: event.end,
        event_id: event.id.clone(),
        tray_label,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinAlert {
    pub key: String,
    pub title: String,
    pub body: String,
    pub url: String,
}

/// Avisos que todavía no se han enviado para la próxima reunión.
/// Si el enlace aún se está buscando, no se marca nada: el aviso sale cuando aparezca.
pub fn pending_join_alerts(
    now: DateTime<Local>,
    next: &NextMeeting,
    lead_secs: u32,
    links_pending: bool,
    sent: &HashMap<String, ()>,
) -> Vec<JoinAlert> {
    if next.event_id.is_empty() || (next.join_url.is_empty() && links_pending) {
        return Vec::new();
    }
    let until = (next.starts_at - now).num_seconds();
    let mut alerts = Vec::new();
    if until > 0 && until <= i64::from(lead_secs) {
        let key = format!("soon:{}", next.event_id);
        if !sent.contains_key(&key) {
            alerts.push(JoinAlert {
                key,
                title: next.title.clone(),
                body: soon_body(until, !next.join_url.is_empty()),
                url: next.join_url.clone(),
            });
        }
    }
    let just_started = next.starts_at <= now && next.ends_at > now && until >= -180;
    if just_started && !next.join_url.is_empty() {
        let key = format!("start:{}", next.event_id);
        if !sent.contains_key(&key) {
            alerts.push(JoinAlert {
                key,
                title: next.title.clone(),
                body: "Ya puedes unirte.".into(),
                url: next.join_url.clone(),
            });
        }
    }
    alerts
}

fn soon_body(until_secs: i64, can_join: bool) -> String {
    let mins = (until_secs.max(1) + 59) / 60;
    if can_join {
        if mins <= 1 {
            "Empieza en menos de 1 min. Pulsa para unirte.".into()
        } else {
            format!("Empieza en {mins} min. Pulsa para unirte.")
        }
    } else if mins <= 1 {
        "Empieza en menos de 1 min.".into()
    } else {
        format!("Empieza en {mins} min.")
    }
}

pub fn note_join(note: &Note, detect_in_notes: bool) -> MeetingLink {
    if !detect_in_notes {
        return MeetingLink::none();
    }
    detect_meeting(&format!("{}\n{}", note.title, note.body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(day: u32, hour: u32, min: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, day, hour, min, 0).unwrap()
    }

    fn event(id: &str, hour: u32, mins: i64, description: &str, declined: bool) -> CalendarEvent {
        let start = at(28, hour, 0);
        CalendarEvent {
            id: id.into(),
            calendar_id: "work".into(),
            calendar_color: "#0a84ff".into(),
            title: id.into(),
            start,
            end: start + Duration::minutes(mins),
            all_day: false,
            location: String::new(),
            description: description.into(),
            declined,
            conference_url: String::new(),
            event_type: String::new(),
        }
    }

    #[test]
    fn office_days_stay_out_of_the_agenda() {
        let mut office = event("Office", 9, 60, "", false);
        office.all_day = true;
        office.title = "Office".into();
        let view = build_agenda(at(28, 13, 0), &[office], &Preferences::default(), true);
        assert!(!view.today.iter().any(|row| row.title == "Office"));
    }

    #[test]
    fn a_conference_link_is_enough_to_join() {
        let mut meeting = event("Reunión Junta Directiva", 15, 60, "", false);
        meeting.conference_url = "https://meet.google.com/abc-defg-hij".into();
        let view = build_agenda(at(28, 13, 0), &[meeting], &Preferences::default(), true);
        let row = view.today.iter().find(|row| row.title == "Reunión Junta Directiva").unwrap();
        assert!(row.can_join);
        assert_eq!(row.prov_short, "Meet");
    }

    #[test]
    fn factorial_days_show_up_without_becoming_the_next_meeting() {
        let now = at(28, 13, 0);
        let mut birthday = event("factorial:birthday", 0, 60, "Cumpleaños", false);
        birthday.title = "Tu cumpleaños".into();
        birthday.all_day = true;
        birthday.event_type = "factorial".into();
        birthday.start = at(28, 0, 0);
        birthday.end = at(29, 0, 0);
        let meeting = event("daily", 15, 30, "https://meet.google.com/abc-defg-hij", false);
        let view = build_agenda(now, &[birthday, meeting], &Preferences::default(), true);
        let row = view.today.iter().find(|row| row.title == "Tu cumpleaños").unwrap();
        assert_eq!(row.prov_name, "Factorial");
        assert!(!row.can_join);
        assert_eq!(row.time, "Día");
        assert_eq!(view.next.title, "daily");
    }

    #[test]
    fn agenda_counts_down_and_hides_declined() {
        let now = at(28, 13, 4);
        let mut prefs = Preferences::default();
        prefs.meeting_window = crate::domain::preferences::MeetingWindow::Hour;
        let events = vec![
            event("daily", 9, 30, "https://meet.google.com/abc-defg-hij", false),
            event(
                "place",
                14,
                45,
                "https://place2pay.zoom.us/j/84512093321?pwd=x1",
                false,
            ),
            event("no", 16, 30, "https://meet.google.com/zzz-yyyy-xxx", true),
        ];
        let view = build_agenda(now, &events, &prefs, true);
        assert_eq!(view.next.title, "place");
        assert_eq!(view.next.meta, "En 56 min · Zoom");
        assert_eq!(view.next.tray_label, "en 56m");
        assert!(view.today.iter().any(|row| row.is_now));
        assert!(!view.today.iter().any(|row| row.title == "no"));
        assert!(view.today.iter().any(|row| row.title == "daily" && row.past));
    }

    #[test]
    fn description_search_can_be_turned_off() {
        let sample = event("place", 14, 45, "https://place2pay.zoom.us/j/84512093321?pwd=x1", false);
        let mut prefs = Preferences::default();
        prefs.detect_in_description = false;
        let hidden = event_link(&sample, prefs.detect_in_description);
        assert_eq!(hidden.kind, MeetingKind::None);
        prefs.detect_in_description = true;
        assert_eq!(event_link(&sample, true).provider_id, "zoom");
    }

    #[test]
    fn incremental_sync_replaces_and_deletes() {
        let mut cache = vec![event("a", 9, 30, "", false)];
        apply_page(
            &mut cache,
            "work",
            true,
            vec![
                EventChange {
                    event: event("a", 10, 30, "", false),
                    deleted: true,
                },
                EventChange {
                    event: event("b", 11, 30, "", false),
                    deleted: false,
                },
            ],
        );
        assert_eq!(cache.len(), 1);
        assert_eq!(cache[0].id, "b");
    }

    #[test]
    fn overdue_tasks_use_the_real_date() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 29).unwrap();
        let due = NaiveDate::from_ymd_opt(2026, 10, 24).unwrap();
        let (label, late) = task_due_label(today, Some(due), false);
        assert_eq!(label, "Hace 5 días");
        assert!(late);
    }

    #[test]
    fn open_tasks_show_the_newest_first_and_done_last() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 29).unwrap();
        let at = |secs| DateTime::UNIX_EPOCH.with_timezone(&Local) + Duration::seconds(secs);
        let task = |title: &str, due: Option<NaiveDate>, done: bool, secs| TaskItem {
            id: title.into(),
            title: title.into(),
            due,
            done,
            list_id: "@default".into(),
            updated_at: at(secs),
            completed_at: None,
        };
        let mut tasks = vec![
            task("Hecha", Some(today), true, 90),
            task("Vieja", None, false, 10),
            task("Nueva", Some(today + Duration::days(1)), false, 50),
        ];
        sort_tasks(&mut tasks);
        let titles: Vec<_> = tasks.iter().map(|item| item.title.as_str()).collect();
        assert_eq!(titles, ["Nueva", "Vieja", "Hecha"]);
    }

    #[test]
    fn completed_tasks_from_another_day_are_not_today() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 30).unwrap();
        let mut done_today = TaskItem {
            id: "hoy".into(),
            title: "Hoy".into(),
            due: None,
            done: true,
            list_id: "@default".into(),
            updated_at: DateTime::UNIX_EPOCH.with_timezone(&Local),
            completed_at: Some(
                today
                    .and_hms_opt(9, 0, 0)
                    .unwrap()
                    .and_local_timezone(Local)
                    .earliest()
                    .unwrap(),
            ),
        };
        assert!(task_done_today(today, &done_today));
        done_today.completed_at = Some(
            (today - Duration::days(1))
                .and_hms_opt(18, 0, 0)
                .unwrap()
                .and_local_timezone(Local)
                .earliest()
                .unwrap(),
        );
        assert!(!task_done_today(today, &done_today));
        let open = TaskItem {
            completed_at: None,
            done: false,
            ..done_today
        };
        assert!(!task_done_today(today, &open));
    }

    fn next_meeting(id: &str, start: DateTime<Local>, end: DateTime<Local>, url: &str) -> NextMeeting {
        NextMeeting {
            title: "Junta".into(),
            meta: String::new(),
            color: String::new(),
            abbr: String::new(),
            join_url: url.into(),
            starts_at: start,
            ends_at: end,
            event_id: id.into(),
            tray_label: String::new(),
        }
    }

    #[test]
    fn a_join_alert_waits_for_the_link_then_fires_before_and_at_the_start() {
        let start = at(28, 15, 0);
        let end = start + Duration::minutes(30);
        let meeting = next_meeting("junta", start, end, "https://meet.google.com/abc-defg-hij");
        let soon = start - Duration::minutes(4);
        let sent = HashMap::new();
        let waiting = next_meeting("junta", start, end, "");
        assert!(pending_join_alerts(soon, &waiting, 300, true, &sent).is_empty());
        let early = pending_join_alerts(soon, &meeting, 300, false, &sent);
        assert_eq!(early.len(), 1);
        assert_eq!(early[0].key, "soon:junta");
        assert_eq!(early[0].body, "Empieza en 4 min. Pulsa para unirte.");
        let mut sent = HashMap::new();
        sent.insert(early[0].key.clone(), ());
        assert!(pending_join_alerts(soon, &meeting, 300, false, &sent).is_empty());
        let begun = pending_join_alerts(start + Duration::seconds(20), &meeting, 300, false, &sent);
        assert_eq!(begun.len(), 1);
        assert_eq!(begun[0].key, "start:junta");
        assert_eq!(begun[0].body, "Ya puedes unirte.");
        assert!(pending_join_alerts(start + Duration::minutes(10), &meeting, 300, false, &sent).is_empty());
    }
}
