use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::domain::error::AppError;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MeetingWindow {
    Min15,
    Hour,
    Hours12,
    Hours24,
    Today,
    Always,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub open_at_login: bool,
    pub show_timer_in_tray: bool,
    pub show_meeting_in_tray: bool,
    pub meeting_window: MeetingWindow,
    pub hidden_calendar_ids: Vec<String>,
    pub shown_calendar_ids: Vec<String>,
    pub range_days: u16,
    #[serde(default)]
    pub link_openers: LinkOpeners,
    pub detect_in_notes: bool,
    pub detect_in_description: bool,
    pub notify_lead_secs: u32,
    pub auto_join: bool,
    pub hide_declined: bool,
    pub focus_minutes: u32,
    pub short_break_minutes: u32,
    pub long_break_minutes: u32,
    pub auto_start_break: bool,
    pub pause_on_meeting: bool,
    pub auto_sound: bool,
    pub volume: u8,
    pub track: String,
    pub breath_tech: String,
    pub breath_sound: bool,
    #[serde(default = "default_breath_cycles")]
    pub breath_cycles: u8,
    pub assistant_daily: bool,
    pub assistant_when: String,
    pub assistant_suggest: bool,
    pub assistant_reads_notes: bool,
    #[serde(default)]
    pub read_mail: bool,
    pub service_calendar: bool,
    pub service_tasks: bool,
    pub service_chat: bool,
    pub service_gemini: bool,
    #[serde(default)]
    pub service_factorial: bool,
    #[serde(default)]
    pub service_clickup: bool,
    #[serde(default = "default_chat_ui")]
    pub chat_ui: String,
    #[serde(default = "default_panel_shortcut")]
    pub panel_shortcut: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            open_at_login: true,
            show_timer_in_tray: true,
            show_meeting_in_tray: true,
            meeting_window: MeetingWindow::Hour,
            hidden_calendar_ids: Vec::new(),
            shown_calendar_ids: Vec::new(),
            range_days: 7,
            link_openers: LinkOpeners::default(),
            detect_in_notes: true,
            detect_in_description: true,
            notify_lead_secs: 300,
            auto_join: false,
            hide_declined: true,
            focus_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            auto_start_break: true,
            pause_on_meeting: true,
            auto_sound: true,
            volume: 40,
            track: "brown".into(),
            breath_tech: "coherente".into(),
            breath_sound: true,
            breath_cycles: default_breath_cycles(),
            assistant_daily: true,
            assistant_when: "Al abrir".into(),
            assistant_suggest: true,
            assistant_reads_notes: true,
            read_mail: false,
            service_calendar: true,
            service_tasks: false,
            service_chat: false,
            service_gemini: false,
            service_factorial: false,
            service_clickup: false,
            chat_ui: default_chat_ui(),
            panel_shortcut: default_panel_shortcut(),
        }
    }
}

fn default_chat_ui() -> String {
    "Ventana".into()
}

fn default_panel_shortcut() -> String {
    "alt+super+Space".into()
}

pub fn normalize_shortcut(raw: &str) -> Result<String, AppError> {
    let mut shift = false;
    let mut control = false;
    let mut alt = false;
    let mut super_key = false;
    let mut key = None;
    for token in raw.split('+').map(str::trim).filter(|token| !token.is_empty()) {
        match token.to_ascii_lowercase().as_str() {
            "shift" => shift = true,
            "control" | "ctrl" => control = true,
            "alt" | "option" | "opt" => alt = true,
            "super" | "cmd" | "command" | "meta" => super_key = true,
            other => {
                if key.is_some() {
                    return Err(AppError::message("El atajo solo puede tener una tecla principal."));
                }
                key = Some(canonical_shortcut_key(other)?);
            }
        }
    }
    if !(shift || control || alt || super_key) {
        return Err(AppError::message("El atajo necesita ⌘, ⌥, ⌃ o ⇧."));
    }
    let Some(key) = key else {
        return Err(AppError::message("El atajo necesita una tecla además del modificador."));
    };
    let mut parts = Vec::new();
    if shift {
        parts.push("shift");
    }
    if control {
        parts.push("control");
    }
    if alt {
        parts.push("alt");
    }
    if super_key {
        parts.push("super");
    }
    parts.push(key.as_str());
    Ok(parts.join("+"))
}

fn canonical_shortcut_key(token: &str) -> Result<String, AppError> {
    let token = token.to_ascii_lowercase();
    if token.len() == 1 {
        let byte = token.as_bytes()[0];
        if byte.is_ascii_alphabetic() {
            return Ok(token.to_ascii_uppercase());
        }
        if byte.is_ascii_digit() {
            return Ok(token);
        }
    }
    Ok(match token.as_str() {
        "space" | "espacio" => "Space".into(),
        "enter" | "return" => "Enter".into(),
        "tab" => "Tab".into(),
        "arrowup" | "up" => "ArrowUp".into(),
        "arrowdown" | "down" => "ArrowDown".into(),
        "arrowleft" | "left" => "ArrowLeft".into(),
        "arrowright" | "right" => "ArrowRight".into(),
        _ => return Err(AppError::message("Esa tecla no se puede usar como atajo.")),
    })
}

pub fn shortcut_label(spec: &str) -> String {
    let mut marks = Vec::new();
    let mut key = String::new();
    for token in spec.split('+').map(str::trim).filter(|token| !token.is_empty()) {
        match token.to_ascii_lowercase().as_str() {
            "shift" => marks.push("⇧"),
            "control" | "ctrl" => marks.push("⌃"),
            "alt" | "option" | "opt" => marks.push("⌥"),
            "super" | "cmd" | "command" | "meta" => marks.push("⌘"),
            "space" => key = "Espacio".into(),
            "enter" => key = "↩".into(),
            "tab" => key = "⇥".into(),
            "arrowup" => key = "↑".into(),
            "arrowdown" => key = "↓".into(),
            "arrowleft" => key = "←".into(),
            "arrowright" => key = "→".into(),
            other => key = other.trim_start_matches("key").to_ascii_uppercase(),
        }
    }
    if !key.is_empty() {
        marks.push(key.as_str());
    }
    marks.join(" ")
}

pub fn migrate_chat_ui(prefs: &mut Preferences) {
    let next = match prefs.chat_ui.as_str() {
        "Ventana" | "Navegador" => return,
        "Enlace" => "Navegador",
        _ => "Ventana",
    };
    prefs.chat_ui = next.into();
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrefEffect {
    Autostart(bool),
    NeedsScope(String),
    PomodoroTiming,
    Shortcut,
}

pub fn calendar_visible(google_selected: bool, id: &str, prefs: &Preferences) -> bool {
    if prefs.hidden_calendar_ids.iter().any(|item| item == id) {
        return false;
    }
    if prefs.shown_calendar_ids.iter().any(|item| item == id) {
        return true;
    }
    google_selected
}

pub fn set_calendar_visible(prefs: &mut Preferences, id: &str, visible: bool) {
    prefs.hidden_calendar_ids.retain(|item| item != id);
    prefs.shown_calendar_ids.retain(|item| item != id);
    if visible {
        prefs.shown_calendar_ids.push(id.to_string());
    } else {
        prefs.hidden_calendar_ids.push(id.to_string());
    }
}

pub fn apply_pref_patch(
    prefs: &mut Preferences,
    patch: &Map<String, Value>,
) -> Result<Vec<PrefEffect>, AppError> {
    let mut effects = Vec::new();
    for (key, value) in patch {
        match key.as_str() {
            "login" => {
                let enabled = expect_bool(value)?;
                prefs.open_at_login = enabled;
                effects.push(PrefEffect::Autostart(enabled));
            }
            "barTimer" => prefs.show_timer_in_tray = expect_bool(value)?,
            "barMeeting" => prefs.show_meeting_in_tray = expect_bool(value)?,
            "barMeetMin" => prefs.meeting_window = parse_window(expect_str(value)?)?,
            "range" => prefs.range_days = parse_range(expect_str(value)?)?,
            "openZoom" => prefs.link_openers.zoom = parse_opener(expect_str(value)?)?,
            "openTeams" => prefs.link_openers.teams = parse_opener(expect_str(value)?)?,
            "openWebex" => prefs.link_openers.webex = parse_opener(expect_str(value)?)?,
            "openMeet" => prefs.link_openers.meet = parse_opener(expect_str(value)?)?,
            "openHangouts" => prefs.link_openers.hangouts = parse_opener(expect_str(value)?)?,
            "openSlack" => prefs.link_openers.slack = parse_opener(expect_str(value)?)?,
            "openJitsi" => prefs.link_openers.jitsi = parse_opener(expect_str(value)?)?,
            "openWhereby" => prefs.link_openers.whereby = parse_opener(expect_str(value)?)?,
            "openGoto" => prefs.link_openers.goto = parse_opener(expect_str(value)?)?,
            "openChime" => prefs.link_openers.chime = parse_opener(expect_str(value)?)?,
            "openDiscord" => prefs.link_openers.discord = parse_opener(expect_str(value)?)?,
            "openFacetime" => prefs.link_openers.facetime = parse_opener(expect_str(value)?)?,
            "openOther" => prefs.link_openers.other = parse_opener(expect_str(value)?)?,
            "detectNotes" => prefs.detect_in_notes = expect_bool(value)?,
            "detectDesc" => prefs.detect_in_description = expect_bool(value)?,
            "notify" => prefs.notify_lead_secs = parse_notify(expect_str(value)?)?,
            "autoJoin" => prefs.auto_join = expect_bool(value)?,
            "hideDeclined" => prefs.hide_declined = expect_bool(value)?,
            "focusMin" => {
                prefs.focus_minutes = expect_u32(value)?.clamp(5, 90);
                effects.push(PrefEffect::PomodoroTiming);
            }
            "breakMin" => {
                prefs.short_break_minutes = expect_u32(value)?.clamp(1, 30);
                effects.push(PrefEffect::PomodoroTiming);
            }
            "longBreak" => {
                prefs.long_break_minutes = expect_u32(value)?.clamp(5, 45);
                effects.push(PrefEffect::PomodoroTiming);
            }
            "autoBreak" => prefs.auto_start_break = expect_bool(value)?,
            "pauseMeet" => prefs.pause_on_meeting = expect_bool(value)?,
            "autoSound" => prefs.auto_sound = expect_bool(value)?,
            "volume" => prefs.volume = expect_u32(value)?.min(100) as u8,
            "track" => prefs.track = expect_str(value)?.to_string(),
            "breathTech" => prefs.breath_tech = expect_str(value)?.to_string(),
            "breathSound" => prefs.breath_sound = expect_bool(value)?,
            "chatUi" => {
                let value = expect_str(value)?;
                prefs.chat_ui = match value {
                    "Ventana" | "iframe" | "Nativa" => "Ventana".into(),
                    "Navegador" | "Enlace" => "Navegador".into(),
                    _ => return Err(AppError::message("Elige Ventana o Navegador.")),
                };
            }
            "breathCycles" => prefs.breath_cycles = expect_u32(value)?.clamp(1, 12) as u8,
            "aiDaily" => prefs.assistant_daily = expect_bool(value)?,
            "aiTime" => prefs.assistant_when = expect_str(value)?.to_string(),
            "aiSuggest" => prefs.assistant_suggest = expect_bool(value)?,
            "aiNotes" => prefs.assistant_reads_notes = expect_bool(value)?,
            "readMail" => {
                let enabled = expect_bool(value)?;
                if enabled && !prefs.read_mail {
                    effects.push(PrefEffect::NeedsScope(
                        "https://www.googleapis.com/auth/gmail.readonly".into(),
                    ));
                }
                prefs.read_mail = enabled;
            }
            "svcCal" => prefs.service_calendar = expect_bool(value)?,
            "svcTasks" => {
                let enabled = expect_bool(value)?;
                if enabled && !prefs.service_tasks {
                    effects.push(PrefEffect::NeedsScope(
                        "https://www.googleapis.com/auth/tasks".into(),
                    ));
                }
                prefs.service_tasks = enabled;
            }
            "svcChat" => prefs.service_chat = expect_bool(value)?,
            "shortcut" => {
                let value = normalize_shortcut(expect_str(value)?)?;
                if value != prefs.panel_shortcut {
                    prefs.panel_shortcut = value;
                    effects.push(PrefEffect::Shortcut);
                }
            }
            "svcGemini" => prefs.service_gemini = expect_bool(value)?,
            "svcFactorial" => prefs.service_factorial = expect_bool(value)?,
            "svcClickup" => prefs.service_clickup = expect_bool(value)?,
            other => {
                return Err(AppError::message(format!(
                    "Preferencia desconocida: {other}"
                )));
            }
        }
    }
    Ok(effects)
}

pub fn window_label(window: MeetingWindow) -> &'static str {
    match window {
        MeetingWindow::Min15 => "15 min",
        MeetingWindow::Hour => "1 h",
        MeetingWindow::Hours12 => "12 h",
        MeetingWindow::Hours24 => "24 h",
        MeetingWindow::Today => "Hoy",
        MeetingWindow::Always => "Siempre",
    }
}

pub fn range_label(days: u16) -> String {
    format!("{days} días")
}

pub fn notify_label(secs: u32) -> String {
    format!("{} min", secs / 60)
}

pub fn within_meeting_window(now: chrono::DateTime<chrono::Local>, starts_at: chrono::DateTime<chrono::Local>, window: MeetingWindow) -> bool {
    if starts_at < now {
        return false;
    }
    let mins = (starts_at - now).num_minutes();
    match window {
        MeetingWindow::Always => true,
        MeetingWindow::Min15 => mins <= 15,
        MeetingWindow::Hour => mins <= 60,
        MeetingWindow::Hours12 => mins <= 12 * 60,
        MeetingWindow::Hours24 => mins <= 24 * 60,
        MeetingWindow::Today => starts_at.date_naive() == now.date_naive(),
    }
}

fn parse_window(value: &str) -> Result<MeetingWindow, AppError> {
    match value {
        "15 min" => Ok(MeetingWindow::Min15),
        "1 h" => Ok(MeetingWindow::Hour),
        "12 h" => Ok(MeetingWindow::Hours12),
        "24 h" => Ok(MeetingWindow::Hours24),
        "Hoy" => Ok(MeetingWindow::Today),
        "Siempre" => Ok(MeetingWindow::Always),
        _ => Err(AppError::message("Ventana de reunión no válida.")),
    }
}

fn parse_range(value: &str) -> Result<u16, AppError> {
    match value {
        "3 días" => Ok(3),
        "7 días" => Ok(7),
        "14 días" => Ok(14),
        _ => Err(AppError::message("Rango no válido.")),
    }
}

fn parse_notify(value: &str) -> Result<u32, AppError> {
    match value {
        "1 min" => Ok(60),
        "5 min" => Ok(300),
        "10 min" => Ok(600),
        _ => Err(AppError::message("Aviso no válido.")),
    }
}

fn default_breath_cycles() -> u8 {
    6
}

fn expect_bool(value: &Value) -> Result<bool, AppError> {
    value
        .as_bool()
        .ok_or_else(|| AppError::message("Se esperaba un sí o no."))
}

fn expect_str(value: &Value) -> Result<&str, AppError> {
    value
        .as_str()
        .ok_or_else(|| AppError::message("Se esperaba un texto."))
}

fn expect_u32(value: &Value) -> Result<u32, AppError> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| AppError::message("Se esperaba un número."))
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum LinkOpener {
    #[default]
    DefaultBrowser,
    NativeApp,
    Chrome,
    Firefox,
    Brave,
    Vivaldi,
    Edge,
    Opera,
    Safari,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LinkOpeners {
    #[serde(default = "native_app")]
    pub zoom: LinkOpener,
    pub teams: LinkOpener,
    pub webex: LinkOpener,
    #[serde(default = "chrome")]
    pub meet: LinkOpener,
    #[serde(default = "chrome")]
    pub hangouts: LinkOpener,
    #[serde(default = "native_app")]
    pub slack: LinkOpener,
    pub jitsi: LinkOpener,
    pub whereby: LinkOpener,
    #[serde(default = "native_app")]
    pub goto: LinkOpener,
    #[serde(default = "native_app")]
    pub chime: LinkOpener,
    #[serde(default = "native_app")]
    pub discord: LinkOpener,
    #[serde(default = "native_app")]
    pub facetime: LinkOpener,
    pub other: LinkOpener,
}

fn native_app() -> LinkOpener {
    LinkOpener::NativeApp
}

fn chrome() -> LinkOpener {
    LinkOpener::Chrome
}

impl Default for LinkOpeners {
    fn default() -> Self {
        Self {
            zoom: native_app(),
            teams: LinkOpener::DefaultBrowser,
            webex: LinkOpener::DefaultBrowser,
            meet: chrome(),
            hangouts: chrome(),
            slack: native_app(),
            jitsi: LinkOpener::DefaultBrowser,
            whereby: LinkOpener::DefaultBrowser,
            goto: native_app(),
            chime: native_app(),
            discord: native_app(),
            facetime: native_app(),
            other: LinkOpener::DefaultBrowser,
        }
    }
}

impl LinkOpeners {
    pub fn for_provider(&self, provider_id: &str) -> LinkOpener {
        match provider_id {
            "zoom" => self.zoom,
            "teams" => self.teams,
            "webex" => self.webex,
            "meet" => self.meet,
            "hangouts" => self.hangouts,
            "slack" => self.slack,
            "jitsi" => self.jitsi,
            "whereby" => self.whereby,
            "goto" => self.goto,
            "chime" => self.chime,
            "discord" => self.discord,
            "facetime" => self.facetime,
            _ => self.other,
        }
    }
}

impl LinkOpener {
    pub fn id(self) -> &'static str {
        match self {
            Self::DefaultBrowser => "defaultBrowser",
            Self::NativeApp => "nativeApp",
            Self::Chrome => "chrome",
            Self::Firefox => "firefox",
            Self::Brave => "brave",
            Self::Vivaldi => "vivaldi",
            Self::Edge => "edge",
            Self::Opera => "opera",
            Self::Safari => "safari",
        }
    }

    pub fn app_name(self) -> Option<&'static str> {
        match self {
            Self::Chrome => Some("Google Chrome"),
            Self::Firefox => Some("Firefox"),
            Self::Brave => Some("Brave Browser"),
            Self::Vivaldi => Some("Vivaldi"),
            Self::Edge => Some("Microsoft Edge"),
            Self::Opera => Some("Opera"),
            Self::Safari => Some("Safari"),
            Self::DefaultBrowser | Self::NativeApp => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::DefaultBrowser => "el navegador por defecto",
            Self::NativeApp => "la app nativa",
            Self::Chrome => "Chrome",
            Self::Firefox => "Firefox",
            Self::Brave => "Brave",
            Self::Vivaldi => "Vivaldi",
            Self::Edge => "Microsoft Edge",
            Self::Opera => "Opera",
            Self::Safari => "Safari",
        }
    }
}

pub fn link_preference_rows(openers: &LinkOpeners) -> [(&'static str, &'static str, &'static str); 13] {
    [
        ("openZoom", "Abrir Zoom", openers.zoom.id()),
        ("openTeams", "Abrir Microsoft Teams", openers.teams.id()),
        ("openWebex", "Abrir Webex", openers.webex.id()),
        ("openMeet", "Abrir Google Meet", openers.meet.id()),
        ("openHangouts", "Abrir Google Hangouts", openers.hangouts.id()),
        ("openSlack", "Abrir Slack", openers.slack.id()),
        ("openJitsi", "Abrir Jitsi", openers.jitsi.id()),
        ("openWhereby", "Abrir Whereby", openers.whereby.id()),
        ("openGoto", "Abrir GoTo Meeting", openers.goto.id()),
        ("openChime", "Abrir Amazon Chime", openers.chime.id()),
        ("openDiscord", "Abrir Discord", openers.discord.id()),
        ("openFacetime", "Abrir FaceTime", openers.facetime.id()),
        ("openOther", "Otros enlaces", openers.other.id()),
    ]
}

fn parse_opener(value: &str) -> Result<LinkOpener, AppError> {
    match value {
        "defaultBrowser" => Ok(LinkOpener::DefaultBrowser),
        "nativeApp" => Ok(LinkOpener::NativeApp),
        "chrome" => Ok(LinkOpener::Chrome),
        "firefox" => Ok(LinkOpener::Firefox),
        "brave" => Ok(LinkOpener::Brave),
        "vivaldi" => Ok(LinkOpener::Vivaldi),
        "edge" => Ok(LinkOpener::Edge),
        "opera" => Ok(LinkOpener::Opera),
        "safari" => Ok(LinkOpener::Safari),
        _ => Err(AppError::message("No reconozco esa forma de abrir el enlace.")),
    }
}

pub fn base_scopes() -> Vec<String> {
    vec![
        "openid".into(),
        "email".into(),
        "profile".into(),
        "https://www.googleapis.com/auth/calendar.readonly".into(),
    ]
}

pub fn scopes_for(prefs: &Preferences) -> Vec<String> {
    let mut scopes = base_scopes();
    if prefs.service_tasks {
        scopes.push("https://www.googleapis.com/auth/tasks".into());
    }
    if prefs.read_mail {
        scopes.push("https://www.googleapis.com/auth/gmail.readonly".into());
    }
    scopes
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tasks_toggle_requests_scope_once() {
        let mut prefs = Preferences::default();
        let patch = json!({ "svcTasks": true }).as_object().unwrap().clone();
        let effects = apply_pref_patch(&mut prefs, &patch).unwrap();
        assert!(prefs.service_tasks);
        assert_eq!(
            effects,
            vec![PrefEffect::NeedsScope(
                "https://www.googleapis.com/auth/tasks".into()
            )]
        );
        let again = apply_pref_patch(&mut prefs, &patch).unwrap();
        assert!(again.is_empty());
    }

    #[test]
    fn stored_prefs_without_link_openers_keep_defaults() {
        let mut value = serde_json::to_value(Preferences::default()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("linkOpeners");
        object.insert("openInApp".into(), json!(true));
        let mut prefs: Preferences = serde_json::from_value(value).unwrap();
        assert_eq!(prefs.link_openers.zoom, LinkOpener::NativeApp);
        assert_eq!(prefs.link_openers.meet, LinkOpener::Chrome);
        assert_eq!(prefs.link_openers.teams, LinkOpener::DefaultBrowser);
        let patch = json!({ "openMeet": "safari" }).as_object().unwrap().clone();
        apply_pref_patch(&mut prefs, &patch).unwrap();
        assert_eq!(prefs.link_openers.meet, LinkOpener::Safari);
    }

    #[test]
    fn factorial_and_clickup_stay_off_until_enabled() {
        let mut value = serde_json::to_value(Preferences::default()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("serviceFactorial");
        object.remove("serviceClickup");
        let mut prefs: Preferences = serde_json::from_value(value).unwrap();
        assert!(!prefs.service_factorial);
        assert!(!prefs.service_clickup);
        let patch = json!({ "svcFactorial": true, "svcClickup": true })
            .as_object()
            .unwrap()
            .clone();
        let effects = apply_pref_patch(&mut prefs, &patch).unwrap();
        assert!(effects.is_empty());
        assert!(prefs.service_factorial);
        assert!(prefs.service_clickup);
    }

    #[test]
    fn breath_cycles_default_to_six_and_can_change() {
        let mut value = serde_json::to_value(Preferences::default()).unwrap();
        value.as_object_mut().unwrap().remove("breathCycles");
        let mut prefs: Preferences = serde_json::from_value(value).unwrap();
        assert_eq!(prefs.breath_cycles, 6);
        let patch = json!({ "breathCycles": 4 }).as_object().unwrap().clone();
        apply_pref_patch(&mut prefs, &patch).unwrap();
        assert_eq!(prefs.breath_cycles, 4);
    }

    #[test]
    fn chat_ui_defaults_to_ventana_and_migrates_iframe() {
        let mut value = serde_json::to_value(Preferences::default()).unwrap();
        value.as_object_mut().unwrap().remove("chatUi");
        let mut prefs: Preferences = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(prefs.chat_ui, "Ventana");
        value["chatUi"] = json!("iframe");
        let mut stored: Preferences = serde_json::from_value(value).unwrap();
        migrate_chat_ui(&mut stored);
        assert_eq!(stored.chat_ui, "Ventana");
        apply_pref_patch(&mut prefs, json!({ "chatUi": "Enlace" }).as_object().unwrap()).unwrap();
        assert_eq!(prefs.chat_ui, "Navegador");
        apply_pref_patch(&mut prefs, json!({ "chatUi": "Nativa" }).as_object().unwrap()).unwrap();
        assert_eq!(prefs.chat_ui, "Ventana");
        assert!(apply_pref_patch(&mut prefs, json!({ "chatUi": "webview" }).as_object().unwrap()).is_err());
    }

    #[test]
    fn twelve_hours_twenty_four_hours_and_the_rest_of_today() {
        use chrono::{Duration, TimeZone};
        let now = chrono::Local.with_ymd_and_hms(2026, 10, 30, 18, 0, 0).unwrap();
        let in_twelve = now + Duration::hours(12);
        let later_today = chrono::Local.with_ymd_and_hms(2026, 10, 30, 23, 30, 0).unwrap();
        let tomorrow = chrono::Local.with_ymd_and_hms(2026, 10, 31, 8, 0, 0).unwrap();
        assert!(within_meeting_window(now, in_twelve, MeetingWindow::Hours12));
        assert!(!within_meeting_window(now, in_twelve + Duration::minutes(1), MeetingWindow::Hours12));
        assert!(within_meeting_window(now, tomorrow, MeetingWindow::Hours24));
        assert!(!within_meeting_window(now, tomorrow, MeetingWindow::Hours12));
        assert!(within_meeting_window(now, later_today, MeetingWindow::Today));
        assert!(!within_meeting_window(now, tomorrow, MeetingWindow::Today));
        let mut prefs = Preferences::default();
        apply_pref_patch(&mut prefs, json!({ "barMeetMin": "Hoy" }).as_object().unwrap()).unwrap();
        assert_eq!(prefs.meeting_window, MeetingWindow::Today);
        assert_eq!(window_label(MeetingWindow::Hours12), "12 h");
        assert_eq!(window_label(MeetingWindow::Hours24), "24 h");
    }

    #[test]
    fn shortcut_needs_a_modifier_and_shows_mac_symbols() {
        assert!(normalize_shortcut("Space").is_err());
        assert_eq!(normalize_shortcut("cmd+alt+space").unwrap(), "alt+super+Space");
        assert_eq!(shortcut_label("alt+super+Space"), "⌥ ⌘ Espacio");
        let mut prefs = Preferences::default();
        let effects = apply_pref_patch(&mut prefs, json!({ "shortcut": "super+shift+K" }).as_object().unwrap()).unwrap();
        assert_eq!(prefs.panel_shortcut, "shift+super+K");
        assert!(effects.contains(&PrefEffect::Shortcut));
    }
}
