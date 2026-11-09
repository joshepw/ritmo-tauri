use chrono::{DateTime, Local};
use serde::Serialize;
use std::collections::BTreeMap;

use crate::domain::preferences::{
    link_preference_rows, notify_label, range_label, shortcut_label, window_label,
};
use crate::domain::timefmt::{
    format_clock, format_date_long, format_menu_clock, format_relative_day, format_weekday, initials,
};
use crate::domain::{
    note_join, sort_tasks, task_done_today, task_due_label, Account, CalendarEvent, MeetingKind, Note, PomodoroState,
    Preferences, RemoteCalendar, Suggestion, TaskItem,
};
use std::collections::HashMap;

use crate::domain::meeting::{detect_meeting, MeetingLink};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub oauth_configured: bool,
    pub connected: bool,
    pub account_name: String,
    pub account_email: String,
    pub account_initials: String,
    pub account_avatar: String,
    pub account_status: String,
    pub account_status_color: String,
    pub weekday_label: String,
    pub date_long: String,
    pub menu_clock: String,
    pub now_mark: String,
    pub bar_meeting_label: String,
    pub show_bar_timer: bool,
    pub show_bar_meeting: bool,
    pub today: Vec<EventDto>,
    pub days: Vec<DayDto>,
    pub next_title: String,
    pub next_meta: String,
    pub next_color: String,
    pub next_abbr: String,
    pub next_join_url: String,
    pub tasks: Vec<TaskDto>,
    pub notes: Vec<NoteDto>,
    pub calendars: Vec<CalendarDto>,
    pub pomodoro: PomodoroDto,
    pub suggestions: Vec<SuggestionDto>,
    pub preferences: PrefsDto,
    pub gemini_key_set: bool,
    pub factorial_key_set: bool,
    pub clickup_key_set: bool,
    pub gemini_key_mask: String,
    pub factorial_key_mask: String,
    pub clickup_key_mask: String,
    pub events_loading: bool,
    pub tasks_loading: bool,
    pub clickup_loading: bool,
    pub clickup_tasks: Vec<TaskDto>,
    pub clickup_groups: Vec<ClickUpGroupDto>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClickUpGroupDto {
    pub label: String,
    pub tasks: Vec<TaskDto>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickDto {
    pub remaining_secs: u32,
    pub duration_secs: u32,
    pub running: bool,
    pub mode: String,
    pub session: u8,
    pub focus_task_id: Option<String>,
    pub completed: BTreeMap<String, u32>,
    pub chime: bool,
    pub menu_clock: String,
    pub weekday_label: String,
    pub date_long: String,
    pub now_mark: String,
    pub bar_meeting_label: String,
    pub show_bar_timer: bool,
    pub show_bar_meeting: bool,
    pub next_title: String,
    pub next_meta: String,
    pub next_color: String,
    pub next_abbr: String,
    pub next_join_url: String,
    pub tray_title: String,
    pub today: Vec<EventDto>,
    pub days: Vec<DayDto>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDto {
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayDto {
    pub label: String,
    pub events: Vec<EventDto>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDto {
    pub id: String,
    pub title: String,
    pub due: String,
    pub late: bool,
    pub done: bool,
    pub done_today: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDto {
    pub id: String,
    pub title: String,
    pub date: String,
    pub body: String,
    pub can_join: bool,
    pub prov_color: String,
    pub prov_abbr: String,
    pub join_label: String,
    pub join_url: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarDto {
    pub id: String,
    pub label: String,
    pub sub: String,
    pub color: String,
    pub selected: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionDto {
    pub icon: String,
    pub bg: String,
    pub color: String,
    pub title: String,
    pub when_label: String,
    pub cta: String,
    pub action: String,
    pub target_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PomodoroDto {
    pub mode: String,
    pub running: bool,
    pub remaining_secs: u32,
    pub duration_secs: u32,
    pub session: u8,
    pub focus_task_id: Option<String>,
    pub completed: BTreeMap<String, u32>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefsDto {
    pub login: bool,
    pub bar_timer: bool,
    pub bar_meeting: bool,
    pub bar_meet_min: String,
    pub range: String,
    pub link_rows: Vec<LinkRowDto>,
    pub detect_notes: bool,
    pub detect_desc: bool,
    pub notify: String,
    pub auto_join: bool,
    pub hide_declined: bool,
    pub focus_min: u32,
    pub break_min: u32,
    pub long_break: u32,
    pub auto_break: bool,
    pub pause_meet: bool,
    pub auto_sound: bool,
    pub volume: u8,
    pub track: String,
    pub breath_tech: String,
    pub breath_sound: bool,
    pub breath_cycles: u8,
    pub ai_daily: bool,
    pub ai_time: String,
    pub ai_suggest: bool,
    pub ai_notes: bool,
    pub read_mail: bool,
    pub svc_cal: bool,
    pub svc_tasks: bool,
    pub svc_chat: bool,
    pub svc_gemini: bool,
    pub svc_factorial: bool,
    pub svc_clickup: bool,
    pub chat_ui: String,
    pub shortcut: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkRowDto {
    pub key: String,
    pub label: String,
    pub value: String,
}

pub struct PresentInput<'a> {
    pub now: DateTime<Local>,
    pub prefs: &'a Preferences,
    pub account: Option<&'a Account>,
    pub events: &'a [CalendarEvent],
    pub tasks: &'a [TaskItem],
    pub notes: &'a [Note],
    pub pomodoro: &'a PomodoroState,
    pub calendars: &'a [RemoteCalendar],
    pub oauth_configured: bool,
    pub gemini_key_set: bool,
    pub factorial_key_set: bool,
    pub clickup_key_set: bool,
    pub gemini_key_mask: String,
    pub factorial_key_mask: String,
    pub clickup_key_mask: String,
    pub events_loading: bool,
    pub tasks_loading: bool,
    pub clickup_loading: bool,
    pub factorial_events: &'a [CalendarEvent],
    pub clickup_tasks: &'a [TaskItem],
    pub links: &'a HashMap<String, MeetingLink>,
    pub links_pending: bool,
    pub account_avatar: String,
    pub mail: &'a [crate::domain::MailHint],
    pub ai_suggestions: &'a [Suggestion],
}

pub fn present(input: PresentInput<'_>) -> Snapshot {
    let connected = input.account.is_some();
    let mut agenda_events = input.events.to_vec();
    if input.prefs.service_factorial {
        agenda_events.extend(input.factorial_events.iter().cloned());
    }
    let agenda = crate::domain::build_agenda_with(
        input.now,
        &agenda_events,
        input.prefs,
        connected,
        input.links,
        input.links_pending,
    );
    let today_date = input.now.date_naive();
    let (name, email, status, status_color) = account_copy(input.account, input.now, input.oauth_configured);
    Snapshot {
        oauth_configured: input.oauth_configured,
        connected,
        account_name: name.clone(),
        account_email: email,
        account_initials: initials(&name),
        account_avatar: input.account_avatar.clone(),
        account_status: status,
        account_status_color: status_color,
        weekday_label: format_weekday(input.now),
        date_long: format_date_long(input.now),
        menu_clock: format_menu_clock(input.now),
        now_mark: format_clock(input.now.time()),
        bar_meeting_label: agenda.next.tray_label.clone(),
        show_bar_timer: input.prefs.show_timer_in_tray,
        show_bar_meeting: input.prefs.show_meeting_in_tray && !agenda.next.tray_label.is_empty(),
        today: agenda.today.into_iter().map(event_dto).collect(),
        days: agenda
            .days
            .into_iter()
            .map(|day| DayDto {
                label: day.label,
                events: day.events.into_iter().map(event_dto).collect(),
            })
            .collect(),
        next_title: agenda.next.title,
        next_meta: agenda.next.meta,
        next_color: agenda.next.color,
        next_abbr: agenda.next.abbr,
        next_join_url: agenda.next.join_url,
        tasks: task_rows(input.tasks, today_date, false),
        clickup_tasks: task_rows(input.clickup_tasks, today_date, false),
        clickup_groups: clickup_groups(input.clickup_tasks, today_date),
        notes: input
            .notes
            .iter()
            .map(|note| note_dto(note, input.now, input.prefs.detect_in_notes))
            .collect(),
        calendars: input
            .calendars
            .iter()
            .map(|calendar| CalendarDto {
                id: calendar.id.clone(),
                label: calendar.title.clone(),
                sub: calendar.subtitle.clone(),
                color: calendar.color.clone(),
                selected: crate::domain::calendar_visible(
                    calendar.google_selected,
                    &calendar.id,
                    input.prefs,
                ),
            })
            .collect(),
        pomodoro: pomodoro_dto(input.pomodoro, input.prefs),
        suggestions: {
            let local = crate::domain::suggestions(
                &crate::domain::DayContext {
                    now: input.now,
                    events: input.events,
                    tasks: input.tasks,
                    notes: input.notes,
                    mail: input.mail,
                    connected,
                },
                input.prefs,
            );
            let chosen = if input.ai_suggestions.is_empty() {
                local
            } else {
                input.ai_suggestions.to_vec()
            };
            chosen.into_iter().map(suggestion_dto).collect()
        },
        preferences: prefs_dto(input.prefs),
        gemini_key_set: input.gemini_key_set,
        factorial_key_set: input.factorial_key_set,
        clickup_key_set: input.clickup_key_set,
        gemini_key_mask: input.gemini_key_mask,
        factorial_key_mask: input.factorial_key_mask,
        clickup_key_mask: input.clickup_key_mask,
        events_loading: input.events_loading,
        tasks_loading: input.tasks_loading,
        clickup_loading: input.clickup_loading,
    }
}

fn clickup_groups(tasks: &[TaskItem], today: chrono::NaiveDate) -> Vec<ClickUpGroupDto> {
    let mut ordered = tasks.to_vec();
    ordered.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.title.cmp(&right.title))
    });
    let mut groups = Vec::<ClickUpGroupDto>::new();
    for task in ordered {
        let label = if task.list_id.trim().is_empty() {
            "Sin proyecto".into()
        } else {
            task.list_id.clone()
        };
        let row = task_dto(&task, today, false);
        if let Some(group) = groups.iter_mut().find(|group| group.label == label) {
            group.tasks.push(row);
        } else {
            groups.push(ClickUpGroupDto {
                label,
                tasks: vec![row],
            });
        }
    }
    groups
}

fn task_dto(task: &TaskItem, today: chrono::NaiveDate, show_list: bool) -> TaskDto {
    let (due, late) = task_due_label(today, task.due, task.done);
    let due = if show_list && !task.list_id.is_empty() {
        if due.is_empty() {
            task.list_id.clone()
        } else {
            format!("{} · {due}", task.list_id)
        }
    } else {
        due
    };
    TaskDto {
        id: task.id.clone(),
        title: task.title.clone(),
        due,
        late,
        done: task.done,
        done_today: task_done_today(today, task),
    }
}

fn task_rows(tasks: &[TaskItem], today: chrono::NaiveDate, show_list: bool) -> Vec<TaskDto> {
    let mut ordered = tasks.to_vec();
    sort_tasks(&mut ordered);
    ordered.iter().map(|task| task_dto(task, today, show_list)).collect()
}

pub fn tick_from_with(
    now: DateTime<Local>,
    prefs: &Preferences,
    pomodoro: &PomodoroState,
    events: &[CalendarEvent],
    connected: bool,
    chime: bool,
    links: &HashMap<String, MeetingLink>,
    links_pending: bool,
) -> TickDto {
    let agenda = crate::domain::build_agenda_with(now, events, prefs, connected, links, links_pending);
    let view = pomodoro_dto(pomodoro, prefs);
    let tray_title = tray_text(prefs, view.remaining_secs, &agenda.next.tray_label);
    TickDto {
        remaining_secs: view.remaining_secs,
        duration_secs: view.duration_secs,
        running: view.running,
        mode: view.mode.clone(),
        session: view.session,
        focus_task_id: view.focus_task_id.clone(),
        completed: view.completed.clone(),
        chime,
        menu_clock: format_menu_clock(now),
        weekday_label: format_weekday(now),
        date_long: format_date_long(now),
        now_mark: format_clock(now.time()),
        bar_meeting_label: agenda.next.tray_label.clone(),
        show_bar_timer: prefs.show_timer_in_tray,
        show_bar_meeting: prefs.show_meeting_in_tray && !agenda.next.tray_label.is_empty(),
        next_title: agenda.next.title,
        next_meta: agenda.next.meta,
        next_color: agenda.next.color,
        next_abbr: agenda.next.abbr,
        next_join_url: agenda.next.join_url,
        tray_title,
        today: agenda.today.into_iter().map(event_dto).collect(),
        days: agenda
            .days
            .into_iter()
            .map(|day| DayDto {
                label: day.label,
                events: day.events.into_iter().map(event_dto).collect(),
            })
            .collect(),
    }
}

pub fn tray_text(prefs: &Preferences, remaining: u32, meeting: &str) -> String {
    let mut parts = Vec::new();
    if prefs.show_timer_in_tray {
        parts.push(format!("{:02}:{:02}", remaining / 60, remaining % 60));
    }
    if prefs.show_meeting_in_tray && !meeting.is_empty() {
        parts.push(meeting.to_string());
    }
    parts.join("  ·  ")
}

fn event_dto(row: crate::domain::TodayRow) -> EventDto {
    EventDto {
        is_now: row.is_now,
        is_event: row.is_event,
        time: row.time,
        title: row.title,
        past: row.past,
        color: row.color,
        prov_name: row.prov_name,
        prov_short: row.prov_short,
        prov_abbr: row.prov_abbr,
        prov_color: row.prov_color,
        meta: row.meta,
        can_join: row.can_join,
        join_url: row.join_url,
        live: row.live,
    }
}

fn note_dto(note: &Note, now: DateTime<Local>, detect: bool) -> NoteDto {
    let link = note_join(note, detect);
    let can_join = link.kind != MeetingKind::None && link.kind != MeetingKind::Room && !link.url.is_empty();
    NoteDto {
        id: note.id.clone(),
        title: note.title.clone(),
        date: format_relative_day(now.date_naive(), note.updated_at.date_naive()),
        body: note.body.clone(),
        can_join,
        prov_color: link.color,
        prov_abbr: link.abbr,
        join_label: if can_join {
            format!("Unirse por {}", link.short)
        } else {
            String::new()
        },
        join_url: link.url,
    }
}

fn suggestion_dto(item: Suggestion) -> SuggestionDto {
    SuggestionDto {
        icon: item.icon,
        bg: item.bg,
        color: item.color,
        title: item.title,
        when_label: item.when_label,
        cta: item.cta,
        action: item.action,
        target_id: item.target_id,
    }
}

fn pomodoro_dto(state: &PomodoroState, prefs: &Preferences) -> PomodoroDto {
    PomodoroDto {
        mode: match state.mode {
            crate::domain::PomodoroMode::Focus => "focus".into(),
            crate::domain::PomodoroMode::Break => "break".into(),
        },
        running: state.running,
        remaining_secs: state.remaining_secs,
        duration_secs: state.duration_secs(prefs).max(1),
        session: state.session,
        focus_task_id: state.focus_task_id.clone(),
        completed: state.completed.clone(),
    }
}

fn prefs_dto(prefs: &Preferences) -> PrefsDto {
    PrefsDto {
        login: prefs.open_at_login,
        bar_timer: prefs.show_timer_in_tray,
        bar_meeting: prefs.show_meeting_in_tray,
        bar_meet_min: window_label(prefs.meeting_window).into(),
        range: range_label(prefs.range_days),
        link_rows: link_preference_rows(&prefs.link_openers)
            .into_iter()
            .map(|(key, label, value)| LinkRowDto {
                key: key.into(),
                label: label.into(),
                value: value.into(),
            })
            .collect(),
        detect_notes: prefs.detect_in_notes,
        detect_desc: prefs.detect_in_description,
        notify: notify_label(prefs.notify_lead_secs),
        auto_join: prefs.auto_join,
        hide_declined: prefs.hide_declined,
        focus_min: prefs.focus_minutes,
        break_min: prefs.short_break_minutes,
        long_break: prefs.long_break_minutes,
        auto_break: prefs.auto_start_break,
        pause_meet: prefs.pause_on_meeting,
        auto_sound: prefs.auto_sound,
        volume: prefs.volume,
        track: prefs.track.clone(),
        breath_tech: prefs.breath_tech.clone(),
        breath_sound: prefs.breath_sound,
        breath_cycles: prefs.breath_cycles.max(1),
        ai_daily: prefs.assistant_daily,
        ai_time: prefs.assistant_when.clone(),
        ai_suggest: prefs.assistant_suggest,
        ai_notes: prefs.assistant_reads_notes,
        read_mail: prefs.read_mail,
        svc_cal: prefs.service_calendar,
        svc_tasks: prefs.service_tasks,
        svc_chat: prefs.service_chat,
        svc_gemini: prefs.service_gemini,
        svc_factorial: prefs.service_factorial,
        svc_clickup: prefs.service_clickup,
        chat_ui: prefs.chat_ui.clone(),
        shortcut: shortcut_label(&prefs.panel_shortcut),
    }
}

fn account_copy(account: Option<&Account>, now: DateTime<Local>, oauth_configured: bool) -> (String, String, String, String) {
    let Some(account) = account else {
        let email = if oauth_configured {
            "Conecta tu cuenta de Google".into()
        } else {
            "Falta el cliente OAuth. Revisa DEVELOPMENT.md".into()
        };
        return (
            "Sin cuenta conectada".into(),
            email,
            "Desconectado".into(),
            "rgba(235,235,245,.5)".into(),
        );
    };
    let status = match account.last_sync {
        None => "Conectado".into(),
        Some(last) => {
            let mins = (now - last).num_minutes().max(0);
            if mins < 1 {
                "Conectado · sincronizado ahora".into()
            } else if mins == 1 {
                "Conectado · sincronizado hace 1 min".into()
            } else if mins < 60 {
                format!("Conectado · sincronizado hace {mins} min")
            } else {
                format!("Conectado · sincronizado hace {} h", mins / 60)
            }
        }
    };
    (
        if account.name.is_empty() { "Cuenta de Google".into() } else { account.name.clone() },
        if account.email.is_empty() {
            "Google".into()
        } else {
            format!("{} · Google", account.email)
        },
        status,
        "#32d74b".into(),
    )
}

pub fn meeting_from_url(url: &str) -> crate::domain::MeetingLink {
    detect_meeting(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{CalendarEvent, PomodoroState};
    use chrono::{Duration, TimeZone};

    #[test]
    fn the_clock_moves_the_divider_and_finished_events() {
        let prefs = Preferences::default();
        let pomodoro = PomodoroState::fresh(&prefs);
        let start = Local.with_ymd_and_hms(2026, 10, 30, 10, 0, 0).unwrap();
        let events = vec![CalendarEvent {
            id: "daily".into(),
            calendar_id: "work".into(),
            calendar_color: "#0a84ff".into(),
            title: "Daily".into(),
            start,
            end: start + Duration::minutes(30),
            all_day: false,
            location: String::new(),
            description: String::new(),
            declined: false,
            conference_url: String::new(),
            event_type: String::new(),
        }];
        let during = tick_from_with(start + Duration::minutes(15), &prefs, &pomodoro, &events, true, false, &HashMap::new(), false);
        assert!(during.today.iter().all(|row| !row.is_now));
        assert!(during.today.iter().any(|row| row.title == "Daily" && row.live && !row.past));
        let after = tick_from_with(start + Duration::minutes(31), &prefs, &pomodoro, &events, true, false, &HashMap::new(), false);
        let later = after.today.iter().find(|row| row.is_now).unwrap();
        assert_eq!(later.time, "10:31 a. m.");
        assert!(after.today.iter().any(|row| row.title == "Daily" && row.past && !row.live));
    }

    #[test]
    fn clickup_groups_follow_the_newest_task() {
        use crate::domain::TaskItem;
        let at = |secs| DateTime::UNIX_EPOCH.with_timezone(&Local) + Duration::seconds(secs);
        let task = |title: &str, place: &str, secs| TaskItem {
            id: title.into(),
            title: title.into(),
            due: None,
            done: false,
            list_id: place.into(),
            updated_at: at(secs),
            completed_at: None,
        };
        let today = Local::now().date_naive();
        let groups = clickup_groups(
            &[
                task("Vieja", "Sitio", 10),
                task("Media", "Cliente", 20),
                task("Nueva", "Sitio", 30),
            ],
            today,
        );
        assert_eq!(groups[0].label, "Sitio");
        assert_eq!(groups[0].tasks[0].title, "Nueva");
        assert_eq!(groups[0].tasks[1].title, "Vieja");
        assert_eq!(groups[1].label, "Cliente");
        assert_eq!(groups[1].tasks[0].title, "Media");
    }
}
