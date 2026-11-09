use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Local};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

use crate::application::present::{
    meeting_from_url, present, tick_from_with, PresentInput, Snapshot, TickDto,
};
use crate::application::sync::{merge_today, pull_calendars};
use crate::domain::{
    assistant_prompt, create_note, event_link, local_summary, open_message, open_target, should_prepare_summary,
    AppError, CalendarEvent, DayContext, FocusInterval, MeetingLink, Note, PhaseEnd, PomodoroMode,
    PomodoroState, PrefEffect, Preferences, RemoteCalendar, SecretPort, TaskItem, TaskPort, WorkLog,
};
use crate::domain::{apply_pref_patch, set_calendar_visible};
use crate::infrastructure::clock::SystemClock;
use crate::infrastructure::google::{GeminiAssistant, GoogleApi};
use crate::infrastructure::links;
use crate::infrastructure::oauth::{self, fetch_profile, refresh_access, OAuthConfig};
use crate::infrastructure::persist::{self, Persisted};
use crate::infrastructure::secrets::{self, KeyringSecrets};
use crate::infrastructure::{clickup, factorial};
use crate::domain::{AssistantPort, Clock};

#[cfg(target_os = "macos")]
fn open_notification_settings() {
    let _ = open::that("x-apple.systempreferences:com.apple.Notifications-Settings.extension");
}

#[cfg(target_os = "macos")]
fn ensure_notification_permission(app: &AppHandle) -> Result<(), String> {
    use mac_usernotifications::{AuthorizationStatus, NotificationSettingStatus};

    let settings = mac_usernotifications::blocking::get_notification_settings()
        .map_err(|_| "No se pudo leer el permiso de avisos.".to_string())?;
    match settings.authorization_status {
        AuthorizationStatus::Authorized | AuthorizationStatus::Provisional | AuthorizationStatus::Ephemeral => {
            if settings.alert_enabled == NotificationSettingStatus::Disabled {
                open_notification_settings();
                return Err("Los avisos de Ritmo están apagados. Los acabo de abrir en Ajustes del Sistema.".into());
            }
            return Ok(());
        }
        AuthorizationStatus::Denied => {
            open_notification_settings();
            return Err("macOS ya rechazó los avisos y no vuelve a preguntar. Activa Ritmo en la ventana de Ajustes que se abrió.".into());
        }
        AuthorizationStatus::NotDetermined | AuthorizationStatus::Unknown => {}
    }

    match crate::shell::request_notification_auth(app) {
        Ok(true) => Ok(()),
        Ok(false) => {
            // The crate drops requestAuthorization's NSError and reports `false`. If the
            // status is still undetermined, macOS refused the bundle (bad signature) and
            // never showed the prompt, so Settings would not list Ritmo either.
            let undecided = mac_usernotifications::blocking::get_notification_settings()
                .map(|settings| {
                    matches!(
                        settings.authorization_status,
                        AuthorizationStatus::NotDetermined | AuthorizationStatus::Unknown
                    )
                })
                .unwrap_or(true);
            if undecided {
                return Err("macOS no deja avisar a esta copia de Ritmo: el bundle no está firmado. Instala una compilación firmada.".into());
            }
            open_notification_settings();
            Err("Rechazaste los avisos. Actívalos para Ritmo en Ajustes del Sistema.".into())
        }
        Err(mac_usernotifications::Error::NoBundleIdentifier) => {
            Err("Los avisos solo funcionan en Ritmo.app, no con `tauri dev`.".into())
        }
        Err(_) => Err("No se pudo pedir permiso para los avisos.".into()),
    }
}

#[cfg(target_os = "macos")]
fn post_native_alert(app: AppHandle, title: String, body: String, url: String) {
    use mac_usernotifications::{Action, InterruptionLevel, Notification};
    let mut note = Notification::new()
        .title(title)
        .message(body)
        .default_sound()
        .interruption_level(InterruptionLevel::Active);
    if !url.is_empty() {
        note = note.action(Action::button("join", "Unirse"));
    }
    let Ok(handle) = note.send_blocking() else {
        return;
    };
    if url.is_empty() {
        return;
    }
    let Ok(response) = mac_usernotifications::block_on(handle.response()) else {
        return;
    };
    if response.is_default_action() || response.action_identifier == "join" {
        tauri::async_runtime::spawn(async move {
            if let Some(state) = app.try_state::<crate::application::AppState>() {
                let _ = state.inner.open_meeting(&url).await;
            }
        });
    }
}

struct CachedAccess {
    token: String,
    expires_at: DateTime<Local>,
}

pub struct Inner {
    clock: SystemClock,
    secrets: Arc<dyn SecretPort>,
    http: reqwest::Client,
    oauth: OAuthConfig,
    prefs: Mutex<Preferences>,
    pomodoro: Mutex<PomodoroState>,
    notes: Mutex<Vec<Note>>,
    events: Mutex<Vec<CalendarEvent>>,
    sync_tokens: Mutex<HashMap<String, String>>,
    tasks: Mutex<Vec<TaskItem>>,
    calendars: Mutex<Vec<RemoteCalendar>>,
    account: Mutex<Option<crate::domain::Account>>,
    access: Mutex<Option<CachedAccess>>,
    notified: Mutex<HashMap<String, ()>>,
    joined: Mutex<HashMap<String, ()>>,
    paused_for: Mutex<Option<String>>,
    refreshing: Mutex<bool>,
    events_loading: AtomicBool,
    tasks_loading: AtomicBool,
    links_pending: AtomicBool,
    link_cache: Mutex<HashMap<String, MeetingLink>>,
    setup_done: AtomicBool,
    factorial_events: Mutex<Vec<CalendarEvent>>,
    clickup_tasks: Mutex<Vec<TaskItem>>,
    work_logs: Mutex<Vec<WorkLog>>,
    factorial_employee: Mutex<Option<String>>,
    factorial_ready: AtomicBool,
    clickup_team: Mutex<Option<String>>,
    clickup_loading: AtomicBool,
    app: AppHandle,
    mail_cache: Mutex<Vec<crate::domain::MailHint>>,
    ai_suggestions: Mutex<Vec<crate::domain::Suggestion>>,
    avatar: Mutex<(String, String)>,
    briefing: Mutex<Briefing>,
}

struct Briefing {
    text: String,
    running: bool,
}

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<Inner>,
}

impl AppState {
    pub fn bootstrap(app: &AppHandle) -> Self {
        let config_dir = app.path().app_config_dir().ok();
        let oauth = oauth::load_oauth_config(config_dir.as_deref());
        let disk = persist::load(app);
        let prefs = disk.preferences.clone();
        let pomodoro = disk
            .pomodoro
            .clone()
            .unwrap_or_else(|| PomodoroState::fresh(&prefs));
        let http = reqwest::Client::builder()
            .user_agent("Ritmo/0.1.0")
            // Google Tasks answers "JSON mal formado" when the POST body arrives over HTTP/2.
            .http1_only()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let inner = Arc::new(Inner {
            clock: SystemClock,
            secrets: Arc::new(KeyringSecrets::new(config_dir.as_deref())),
            http,
            oauth,
            prefs: Mutex::new(prefs),
            pomodoro: Mutex::new(pomodoro),
            notes: Mutex::new(disk.notes.clone()),
            events: Mutex::new(disk.events.clone()),
            sync_tokens: Mutex::new(disk.sync_tokens.clone()),
            tasks: Mutex::new(disk.tasks.clone()),
            calendars: Mutex::new(Vec::new()),
            account: Mutex::new(disk.account.clone()),
            access: Mutex::new(None),
            notified: Mutex::new(HashMap::new()),
            joined: Mutex::new(HashMap::new()),
            paused_for: Mutex::new(None),
            refreshing: Mutex::new(false),
            events_loading: AtomicBool::new(false),
            tasks_loading: AtomicBool::new(false),
            links_pending: AtomicBool::new(false),
            link_cache: Mutex::new(HashMap::new()),
            setup_done: AtomicBool::new(disk.setup_complete),
            factorial_events: Mutex::new(disk.factorial_events.clone()),
            clickup_tasks: Mutex::new(disk.clickup_tasks.clone()),
            work_logs: Mutex::new(disk.work_logs.clone()),
            factorial_employee: Mutex::new(disk.factorial_employee_id.clone()),
            factorial_ready: AtomicBool::new(disk.factorial_employee_id.is_some()),
            clickup_team: Mutex::new(disk.clickup_team_id.clone()),
            clickup_loading: AtomicBool::new(false),
            app: app.clone(),
            mail_cache: Mutex::new(Vec::new()),
            ai_suggestions: Mutex::new(Vec::new()),
            avatar: Mutex::new((String::new(), String::new())),
            briefing: Mutex::new(Briefing {
                text: String::new(),
                running: false,
            }),
        });
        let state = Self { inner };
        if disk.preferences.open_at_login {
            let _ = state.inner.set_autostart(true);
        }
        #[cfg(not(target_os = "macos"))]
        state.inner.arm_alerts();
        state
    }

    pub fn setup_complete(&self) -> bool {
        self.inner.setup_done.load(Ordering::Relaxed)
    }

    pub fn panel_shortcut(&self) -> String {
        self.inner
            .prefs
            .try_lock()
            .map(|prefs| prefs.panel_shortcut.clone())
            .unwrap_or_else(|_| "alt+super+Space".into())
    }

    pub fn spawn(&self) {
        let inner = Arc::clone(&self.inner);
        tauri::async_runtime::spawn(async move {
            let _ = inner.refresh_remote().await;
            inner.emit_snapshot().await;
            let mut step = 0u32;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                step = step.wrapping_add(1);
                inner.tick(step).await;
            }
        });
    }
}

impl Inner {
    pub async fn snapshot(&self) -> Snapshot {
        let now = self.clock.now();
        let prefs = self.prefs.lock().await.clone();
        let account = self.account.lock().await.clone();
        let events = self.events.lock().await.clone();
        let tasks = self.tasks.lock().await.clone();
        let notes = self.notes.lock().await.clone();
        let pomodoro = self.pomodoro.lock().await.clone();
        let calendars = self.calendars.lock().await.clone();
        let gemini_mask = self.secret_mask(oauth::GEMINI_KEY).await;
        let factorial_mask = self.secret_mask(secrets::FACTORIAL_KEY).await;
        let clickup_mask = self.secret_mask(secrets::CLICKUP_KEY).await;
        let factorial_events = self.factorial_events.lock().await.clone();
        let clickup_tasks = self.clickup_tasks.lock().await.clone();
        let (links, links_pending) = self.agenda_links().await;
        let account_avatar = match account.as_ref() {
            Some(account) if !account.email.is_empty() => self.account_avatar(&account.email).await,
            _ => String::new(),
        };
        let mail = if prefs.read_mail {
            self.mail_cache.lock().await.clone()
        } else {
            Vec::new()
        };
        let ai_suggestions = self.ai_suggestions.lock().await.clone();
        present(PresentInput {
            now,
            prefs: &prefs,
            account: account.as_ref(),
            events: &events,
            tasks: &tasks,
            notes: &notes,
            pomodoro: &pomodoro,
            calendars: &calendars,
            oauth_configured: self.oauth.configured(),
            gemini_key_set: !gemini_mask.is_empty(),
            factorial_key_set: !factorial_mask.is_empty(),
            clickup_key_set: !clickup_mask.is_empty(),
            gemini_key_mask: gemini_mask,
            factorial_key_mask: factorial_mask,
            clickup_key_mask: clickup_mask,
            events_loading: self.events_loading.load(Ordering::Relaxed),
            tasks_loading: self.tasks_loading.load(Ordering::Relaxed),
            clickup_loading: self.clickup_loading.load(Ordering::Relaxed),
            factorial_events: &factorial_events,
            clickup_tasks: &clickup_tasks,
            links: &links,
            links_pending,
            account_avatar,
            mail: &mail,
            ai_suggestions: &ai_suggestions,
        })
    }

    async fn account_avatar(&self, email: &str) -> String {
        let email = email.trim().to_lowercase();
        {
            let cached = self.avatar.lock().await;
            if cached.0 == email {
                return cached.1.clone();
            }
        }
        let cache = self
            .app
            .path()
            .app_cache_dir()
            .unwrap_or_else(|_| std::env::temp_dir())
            .join("gravatar");
        let url = crate::infrastructure::gravatar::load(&self.http, &cache, &email).await;
        *self.avatar.lock().await = (email, url.clone());
        url
    }

    async fn agenda_links(&self) -> (HashMap<String, MeetingLink>, bool) {
        let links = self.link_cache.lock().await.clone();
        let pending = self.links_pending.load(Ordering::Relaxed);
        (links, pending)
    }

    pub async fn emit_snapshot(&self) {
        let snapshot = self.snapshot().await;
        let _ = self.app.emit("ritmo-snapshot", snapshot);
    }

    pub async fn tick(&self, step: u32) {
        let now = self.clock.now();
        let prefs = self.prefs.lock().await.clone();
        let mut chime = false;
        let mut closed = None;
        {
            let mut pomodoro = self.pomodoro.lock().await;
            if pomodoro.tick(&prefs) == Some(PhaseEnd::FocusEnded) {
                chime = true;
                closed = pomodoro.close_focus(now);
            }
        }
        if let Some(interval) = closed {
            self.queue_work(interval).await;
            self.flush_work_logs().await;
        }
        if step % 60 == 0 {
            let _ = self.refresh_remote().await;
        }
        self.meeting_side_effects(&prefs, now).await;
        let mut events = self.events.lock().await.clone();
        if prefs.service_factorial {
            events.extend(self.factorial_events.lock().await.clone());
        }
        let pomodoro = self.pomodoro.lock().await.clone();
        let connected = self.account.lock().await.is_some();
        let (links, links_pending) = self.agenda_links().await;
        let tick = tick_from_with(now, &prefs, &pomodoro, &events, connected, chime, &links, links_pending);
        self.update_tray(&tick);
        let _ = self.app.emit("ritmo-tick", tick);
        if step % 5 == 0 {
            let _ = self.persist().await;
        }
    }

    pub async fn sign_in(&self) -> Result<(), AppError> {
        let prefs = self.prefs.lock().await.clone();
        self.sign_in_with(&prefs).await?;
        self.refresh_remote().await?;
        self.persist().await?;
        Ok(())
    }

    pub async fn sign_out(&self) -> Result<(), AppError> {
        let _ = self.secrets.delete(oauth::REFRESH_TOKEN);
        *self.access.lock().await = None;
        *self.account.lock().await = None;
        *self.events.lock().await = Vec::new();
        *self.tasks.lock().await = Vec::new();
        *self.sync_tokens.lock().await = HashMap::new();
        *self.calendars.lock().await = Vec::new();
        self.persist().await?;
        self.emit_snapshot().await;
        Ok(())
    }

    pub async fn set_preferences(&self, patch: &Map<String, Value>) -> Result<(), AppError> {
        let current = self.prefs.lock().await.clone();
        let mut next = current.clone();
        let effects = apply_pref_patch(&mut next, patch)?;
        if effects.iter().any(|effect| matches!(effect, PrefEffect::NeedsScope(_))) {
            self.sign_in_with(&next).await?;
        }
        for effect in &effects {
            if let PrefEffect::Autostart(enabled) = effect {
                self.set_autostart(*enabled)?;
            }
        }
        if effects.iter().any(|effect| matches!(effect, PrefEffect::PomodoroTiming)) {
            self.pomodoro.lock().await.sync_duration(&next);
        }
        if effects.iter().any(|effect| matches!(effect, PrefEffect::Shortcut)) {
            if let Err(err) = crate::shell::register_panel_shortcut(&self.app, &next.panel_shortcut) {
                let _ = crate::shell::register_panel_shortcut(&self.app, &current.panel_shortcut);
                return Err(AppError::message(err));
            }
        }
        *self.prefs.lock().await = next;
        self.persist().await?;
        if patch.keys().any(|key| {
            matches!(
                key.as_str(),
                "svcCal"
                    | "svcTasks"
                    | "svcFactorial"
                    | "svcClickup"
                    | "range"
                    | "hideDeclined"
                    | "detectDesc"
            )
        }) {
            let _ = self.refresh_remote().await;
        }
        self.emit_snapshot().await;
        Ok(())
    }

    pub async fn set_calendar(&self, id: &str, visible: bool) -> Result<(), AppError> {
        {
            let mut prefs = self.prefs.lock().await;
            set_calendar_visible(&mut prefs, id, visible);
        }
        self.persist().await?;
        self.refresh_remote().await?;
        Ok(())
    }

    pub async fn add_task(&self, title: &str) -> Result<(), AppError> {
        let title = title.trim();
        if title.is_empty() {
            return Ok(());
        }
        if !self.prefs.lock().await.service_tasks {
            return Err(AppError::message("Activa Google Tasks en Cuenta."));
        }
        let api = self.api().await?;
        let task = TaskPort::add_task(&api, title).await?;
        self.tasks.lock().await.insert(0, task);
        self.persist().await?;
        Ok(())
    }

    pub async fn toggle_task(&self, task_id: &str) -> Result<(), AppError> {
        if clickup::is_clickup_task(task_id) {
            return self.complete_clickup_task(task_id).await;
        }
        if !self.prefs.lock().await.service_tasks {
            return Err(AppError::message("Activa Google Tasks en Cuenta."));
        }
        let current = self
            .tasks
            .lock()
            .await
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
            .ok_or_else(|| AppError::message("No encontré esa tarea."))?;
        let api = self.api().await?;
        let updated = TaskPort::set_done(&api, &current, !current.done).await?;
        let mut tasks = self.tasks.lock().await;
        if let Some(slot) = tasks.iter_mut().find(|task| task.id == task_id) {
            *slot = updated;
        }
        drop(tasks);
        self.persist().await?;
        Ok(())
    }

    pub async fn add_note(&self, title: String, body: String) -> Result<(), AppError> {
        let note = create_note(title, body, self.clock.now());
        self.notes.lock().await.insert(0, note);
        self.persist().await?;
        Ok(())
    }

    pub async fn pomodoro_action(&self, action: &str, task_id: Option<String>) -> Result<(), AppError> {
        let prefs = self.prefs.lock().await.clone();
        let now = self.clock.now();
        let mut closed = Vec::new();
        {
            let mut pomodoro = self.pomodoro.lock().await;
            match action {
                "toggle" => {
                    if pomodoro.running {
                        if pomodoro.mode == PomodoroMode::Focus {
                            if let Some(interval) = pomodoro.close_focus(now) {
                                closed.push(interval);
                            }
                        }
                        pomodoro.set_running(false);
                    } else {
                        pomodoro.set_running(true);
                        if pomodoro.mode == PomodoroMode::Focus {
                            pomodoro.begin_focus(now);
                        }
                    }
                }
                "reset" => {
                    if pomodoro.mode == PomodoroMode::Focus {
                        if let Some(interval) = pomodoro.close_focus(now) {
                            closed.push(interval);
                        }
                    }
                    pomodoro.reset(&prefs);
                }
                "focus" => {
                    if pomodoro.mode != PomodoroMode::Focus {
                        pomodoro.set_mode(PomodoroMode::Focus, &prefs);
                    }
                }
                "break" => {
                    if pomodoro.mode == PomodoroMode::Focus {
                        if let Some(interval) = pomodoro.close_focus(now) {
                            closed.push(interval);
                        }
                    }
                    pomodoro.set_mode(PomodoroMode::Break, &prefs);
                }
                "skip" => pomodoro.skip_break(&prefs),
                "task" | "start" => {
                    let id = task_id.filter(|value| !value.is_empty());
                    let changed = pomodoro.focus_task_id != id;
                    if changed && pomodoro.mode == PomodoroMode::Focus {
                        if let Some(interval) = pomodoro.close_focus(now) {
                            closed.push(interval);
                        }
                    }
                    pomodoro.set_focus_task(id);
                    if action == "start" {
                        if pomodoro.mode != PomodoroMode::Focus {
                            pomodoro.set_mode(PomodoroMode::Focus, &prefs);
                        }
                        pomodoro.set_running(true);
                    }
                    if pomodoro.running && pomodoro.mode == PomodoroMode::Focus {
                        pomodoro.begin_focus(now);
                    }
                }
                _ => return Err(AppError::message("Acción de pomodoro desconocida.")),
            }
        }
        if !closed.is_empty() {
            for interval in closed {
                self.queue_work(interval).await;
            }
            self.flush_work_logs().await;
        }
        self.persist().await?;
        Ok(())
    }

    pub async fn open_meeting(&self, url: &str) -> Result<String, AppError> {
        let prefs = self.prefs.lock().await.clone();
        let link = meeting_from_url(url);
        if link.kind == crate::domain::MeetingKind::None || link.url.is_empty() {
            if url.is_empty() {
                return Err(AppError::message("Ese elemento no tiene un enlace."));
            }
            let opener = prefs.link_openers.other;
            links::launch(url, opener.app_name())?;
            return Ok(format!("Abriendo en {}", opener.label()));
        }
        let opener = prefs.link_openers.for_provider(&link.provider_id);
        let Some(plan) = open_target(&link, opener) else {
            links::launch(url, None)?;
            return Ok("Abriendo el enlace".into());
        };
        links::launch(&plan.target, plan.app_name)?;
        Ok(open_message(&link, opener))
    }

    /// Starts the day analysis away from the window that asked for it.
    /// A running query is left alone, and its text stays until the new one arrives.
    pub async fn request_summary(self: &Arc<Self>, force: bool) -> (String, bool) {
        let previous = {
            let mut job = self.briefing.lock().await;
            if job.running || (!force && !job.text.is_empty()) {
                return (job.text.clone(), job.running);
            }
            job.running = true;
            job.text.clone()
        };
        let inner = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let text = inner.summarize(true).await;
            {
                let mut job = inner.briefing.lock().await;
                job.text = text.clone();
                job.running = false;
            }
            let _ = inner.app.emit("ritmo-briefing", serde_json::json!({ "text": text, "running": false }));
        });
        (previous, true)
    }

    pub async fn ask(&self, question: &str) -> Result<String, AppError> {
        let prefs = self.prefs.lock().await.clone();
        if !prefs.service_gemini {
            return Err(AppError::message("Activa Gemini en Preferencias."));
        }
        let key = self
            .secrets
            .get(oauth::GEMINI_KEY)?
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AppError::message("Agrega tu clave de Gemini en Preferencias."))?;
        let assistant = GeminiAssistant::new(self.http.clone(), key);
        let (system, prompt) = self.prompt(question).await;
        assistant.complete(&system, &prompt).await
    }

    pub async fn set_gemini_key(&self, key: &str) -> Result<(), AppError> {
        let key = key.trim();
        if key.is_empty() {
            return Err(AppError::message("La clave está vacía."));
        }
        self.secrets.set(oauth::GEMINI_KEY, key)?;
        self.prefs.lock().await.service_gemini = true;
        self.persist().await?;
        self.emit_snapshot().await;
        Ok(())
    }

    pub async fn clear_gemini_key(&self) -> Result<(), AppError> {
        self.secrets.delete(oauth::GEMINI_KEY)?;
        self.persist().await?;
        self.emit_snapshot().await;
        Ok(())
    }

    pub async fn set_factorial_key(&self, key: &str) -> Result<(), AppError> {
        let key = key.trim();
        if key.is_empty() {
            return Err(AppError::message("La clave está vacía."));
        }
        factorial::check_key(&self.http, key).await?;
        self.secrets.set(secrets::FACTORIAL_KEY, key)?;
        self.prefs.lock().await.service_factorial = true;
        self.persist().await?;
        let _ = self.refresh_remote().await;
        Ok(())
    }

    pub async fn clear_factorial_key(&self) -> Result<(), AppError> {
        self.secrets.delete(secrets::FACTORIAL_KEY)?;
        self.prefs.lock().await.service_factorial = false;
        self.factorial_events.lock().await.clear();
        *self.factorial_employee.lock().await = None;
        self.factorial_ready.store(false, Ordering::Relaxed);
        self.persist().await?;
        self.emit_snapshot().await;
        Ok(())
    }

    pub async fn set_clickup_key(&self, key: &str) -> Result<(), AppError> {
        let key = key.trim();
        if key.is_empty() {
            return Err(AppError::message("La clave está vacía."));
        }
        clickup::check_key(&self.http, key).await?;
        self.secrets.set(secrets::CLICKUP_KEY, key)?;
        self.prefs.lock().await.service_clickup = true;
        self.persist().await?;
        let _ = self.refresh_remote().await;
        Ok(())
    }

    pub async fn clear_clickup_key(&self) -> Result<(), AppError> {
        self.secrets.delete(secrets::CLICKUP_KEY)?;
        self.prefs.lock().await.service_clickup = false;
        self.clickup_tasks.lock().await.clear();
        *self.clickup_team.lock().await = None;
        self.persist().await?;
        self.emit_snapshot().await;
        Ok(())
    }

    async fn sign_in_with(&self, prefs: &Preferences) -> Result<(), AppError> {
        if !self.oauth.configured() {
            return Err(AppError::message(
                "Falta el cliente OAuth. Crea src-tauri/google.client.json como explica DEVELOPMENT.md.",
            ));
        }
        let scopes = crate::domain::scopes_for(prefs);
        let session = oauth::begin_auth(&self.oauth, &scopes)?;
        links::launch(&session.url, None)?;
        let listener = session.listener;
        let expected = session.state.clone();
        let redirect = session.redirect.clone();
        let verifier = session.verifier.clone();
        let code = tauri::async_runtime::spawn_blocking(move || oauth::wait_for_code(listener, &expected))
            .await
            .map_err(|err| AppError::message(err.to_string()))??;
        let tokens = oauth::exchange_code(&self.http, &self.oauth, &redirect, &verifier, &code).await?;
        if let Some(refresh) = &tokens.refresh_token {
            self.secrets.set(oauth::REFRESH_TOKEN, refresh)?;
        }
        let mut account = fetch_profile(&self.http, &tokens.access_token).await?;
        account.scopes = tokens.scopes;
        *self.access.lock().await = Some(CachedAccess {
            token: tokens.access_token,
            expires_at: tokens.expires_at,
        });
        *self.account.lock().await = Some(account);
        Ok(())
    }

    async fn refresh_remote(&self) -> Result<(), AppError> {
        {
            let mut refreshing = self.refreshing.lock().await;
            if *refreshing {
                return Ok(());
            }
            *refreshing = true;
        }
        let result = self.refresh_remote_inner().await;
        if result.is_err() {
            self.flush_work_logs().await;
        }
        self.events_loading.store(false, Ordering::Relaxed);
        self.tasks_loading.store(false, Ordering::Relaxed);
        if result.is_err() {
            self.links_pending.store(false, Ordering::Relaxed);
        }
        *self.refreshing.lock().await = false;
        self.emit_snapshot().await;
        result
    }

    async fn refresh_remote_inner(&self) -> Result<(), AppError> {
        self.refresh_external().await;
        if self.account.lock().await.is_none() && self.secrets.get(oauth::REFRESH_TOKEN)?.is_none() {
            self.flush_work_logs().await;
            self.persist().await?;
            return Ok(());
        }
        let prefs = self.prefs.lock().await.clone();
        let api = self.api().await?;
        let now = self.clock.now();
        self.events_loading.store(prefs.service_calendar, Ordering::Relaxed);
        self.tasks_loading.store(prefs.service_tasks, Ordering::Relaxed);
        self.links_pending.store(prefs.service_calendar, Ordering::Relaxed);
        self.emit_snapshot().await;
        if prefs.service_calendar {
            let mut events = self.events.lock().await.clone();
            merge_today(&api, &mut events, &prefs, now).await?;
            *self.events.lock().await = events;
            self.events_loading.store(false, Ordering::Relaxed);
            self.emit_snapshot().await;
        }
        let scan_links = async {
            if prefs.service_calendar {
                self.resolve_links().await;
            }
        };
        let load_tasks = async {
            if prefs.service_tasks {
                let tasks = TaskPort::list_tasks(&api).await?;
                *self.tasks.lock().await = tasks;
                self.tasks_loading.store(false, Ordering::Relaxed);
                self.emit_snapshot().await;
            }
            Ok::<(), AppError>(())
        };
        let (_, loaded) = tokio::join!(scan_links, load_tasks);
        loaded?;
        if prefs.service_calendar {
            let mut events = self.events.lock().await.clone();
            let mut tokens = self.sync_tokens.lock().await.clone();
            let calendars = pull_calendars(&api, &mut events, &mut tokens, &prefs, now).await?;
            *self.events.lock().await = events;
            *self.sync_tokens.lock().await = tokens;
            *self.calendars.lock().await = calendars;
            self.emit_snapshot().await;
            self.resolve_links().await;
        }
        self.links_pending.store(false, Ordering::Relaxed);
        if let Some(account) = self.account.lock().await.as_mut() {
            account.last_sync = Some(self.clock.now());
        }
        self.flush_work_logs().await;
        self.persist().await?;
        Ok(())
    }

    async fn refresh_external(&self) {
        let prefs = self.prefs.lock().await.clone();
        if prefs.service_factorial {
            if let Ok(Some(key)) = self.secrets.get(secrets::FACTORIAL_KEY) {
                if !key.trim().is_empty() {
                    let email = self.account.lock().await.as_ref().map(|account| account.email.clone());
                    let today = self.clock.now().date_naive();
                    let horizon = today + chrono::Duration::days(i64::from(prefs.range_days));
                    if let Ok(agenda) =
                        factorial::load(&self.http, key.trim(), email.as_deref(), today, horizon).await
                    {
                        *self.factorial_events.lock().await = agenda.events;
                        *self.factorial_employee.lock().await = agenda.employee_id;
                        self.factorial_ready.store(true, Ordering::Relaxed);
                    }
                }
            }
        } else {
            self.factorial_events.lock().await.clear();
        }
        if prefs.service_clickup {
            if let Ok(Some(token)) = self.secrets.get(secrets::CLICKUP_KEY) {
                if !token.trim().is_empty() {
                    self.clickup_loading.store(true, Ordering::Relaxed);
                    if let Ok(workspace) = clickup::workspace(&self.http, token.trim()).await {
                        *self.clickup_team.lock().await = Some(workspace.team_id.clone());
                        if let Ok(tasks) = clickup::assigned_tasks(&self.http, token.trim(), &workspace).await {
                            *self.clickup_tasks.lock().await = tasks;
                        }
                    }
                    self.clickup_loading.store(false, Ordering::Relaxed);
                }
            }
        } else {
            self.clickup_tasks.lock().await.clear();
            self.clickup_loading.store(false, Ordering::Relaxed);
        }
    }

    async fn secret_mask(&self, key: &str) -> String {
        self.secrets
            .get(key)
            .ok()
            .flatten()
            .map(|value| secrets::mask_secret(&value))
            .unwrap_or_default()
    }

    async fn notice(&self, text: &str) {
        let _ = self.app.emit("ritmo-notice", text.to_string());
    }

    async fn queue_work(&self, interval: FocusInterval) {
        let prefs = self.prefs.lock().await.clone();
        let clickup = interval.task_id.as_deref().is_some_and(clickup::is_clickup_task) && prefs.service_clickup;
        let factorial = prefs.service_factorial;
        if !clickup && !factorial {
            return;
        }
        self.work_logs.lock().await.push(WorkLog {
            started_at: interval.started_at,
            ended_at: interval.ended_at,
            task_id: interval.task_id,
            factorial,
            clickup,
        });
    }

    async fn flush_work_logs(&self) {
        let prefs = self.prefs.lock().await.clone();
        let pending = self.work_logs.lock().await.clone();
        if pending.is_empty() {
            return;
        }
        let mut kept = Vec::new();
        for mut log in pending {
            if log.factorial && !prefs.service_factorial {
                log.factorial = false;
            }
            if log.clickup && !prefs.service_clickup {
                log.clickup = false;
            }
            if log.factorial {
                match self.post_factorial(&log).await {
                    Ok(factorial::ClockResult::Saved) => log.factorial = false,
                    Ok(factorial::ClockResult::OpenShift) => {
                        log.factorial = false;
                        self.notice("Factorial ya tiene un turno abierto. Este enfoque no se fichó.").await;
                    }
                    Ok(factorial::ClockResult::UnknownEmployee) => {
                        log.factorial = false;
                        self.notice("No encontré tu ficha en Factorial. El correo de Google tiene que coincidir.").await;
                    }
                    Err(_) => {}
                }
            }
            if log.clickup {
                if self.post_clickup(&log).await.is_ok() {
                    log.clickup = false;
                }
            }
            if log.factorial || log.clickup {
                kept.push(log);
            }
        }
        *self.work_logs.lock().await = kept;
    }

    async fn post_factorial(&self, log: &WorkLog) -> Result<factorial::ClockResult, AppError> {
        if !self.factorial_ready.load(Ordering::Relaxed) {
            return Err(AppError::message("Factorial todavía no está listo."));
        }
        let Some(employee) = self.factorial_employee.lock().await.clone() else {
            return Ok(factorial::ClockResult::UnknownEmployee);
        };
        let Some(key) = self.secrets.get(secrets::FACTORIAL_KEY)?.filter(|key| !key.trim().is_empty()) else {
            return Ok(factorial::ClockResult::UnknownEmployee);
        };
        factorial::clock_interval(&self.http, key.trim(), &employee, log.started_at, log.ended_at).await
    }

    async fn complete_clickup_task(&self, task_id: &str) -> Result<(), AppError> {
        if !self.prefs.lock().await.service_clickup {
            return Err(AppError::message("Activa ClickUp en Cuenta."));
        }
        let known = self.clickup_tasks.lock().await.iter().any(|task| task.id == task_id);
        if !known {
            return Err(AppError::message("No encontré esa tarea."));
        }
        let Some(token) = self.secrets.get(secrets::CLICKUP_KEY)?.filter(|key| !key.trim().is_empty()) else {
            return Err(AppError::message("Falta la clave de ClickUp."));
        };
        clickup::complete_task(&self.http, token.trim(), task_id).await?;
        self.clickup_tasks.lock().await.retain(|task| task.id != task_id);
        self.persist().await?;
        Ok(())
    }

    async fn post_clickup(&self, log: &WorkLog) -> Result<(), AppError> {
        let Some(task_id) = log.task_id.clone().filter(|id| clickup::is_clickup_task(id)) else {
            return Ok(());
        };
        let Some(token) = self.secrets.get(secrets::CLICKUP_KEY)?.filter(|key| !key.trim().is_empty()) else {
            return Err(AppError::message("Falta la clave de ClickUp."));
        };
        let team = if let Some(team) = self.clickup_team.lock().await.clone() {
            team
        } else {
            let workspace = clickup::workspace(&self.http, token.trim()).await?;
            *self.clickup_team.lock().await = Some(workspace.team_id.clone());
            workspace.team_id
        };
        clickup::log_time(&self.http, token.trim(), &team, &task_id, log.started_at, log.ended_at).await
    }

    async fn resolve_links(&self) {
        let prefs = self.prefs.lock().await.clone();
        let events = self.events.lock().await.clone();
        for chunk in events.chunks(4) {
            let mut found = false;
            {
                let mut cache = self.link_cache.lock().await;
                for event in chunk {
                    if cache.contains_key(&event.id) {
                        continue;
                    }
                    cache.insert(event.id.clone(), event_link(event, prefs.detect_in_description));
                    found = true;
                }
            }
            if found {
                self.emit_snapshot().await;
            }
        }
    }

    async fn api(&self) -> Result<GoogleApi, AppError> {
        Ok(GoogleApi {
            http: self.http.clone(),
            token: self.access_token().await?,
        })
    }

    async fn access_token(&self) -> Result<String, AppError> {
        if let Some(cached) = self.access.lock().await.as_ref() {
            if cached.expires_at > self.clock.now() {
                return Ok(cached.token.clone());
            }
        }
        let refresh = self.secrets.get(oauth::REFRESH_TOKEN)?.ok_or(AppError::SignedOut)?;
        let tokens = match refresh_access(&self.http, &self.oauth, &refresh).await {
            Ok(tokens) => tokens,
            Err(err) if session_expired(&err) => {
                let _ = self.secrets.delete(oauth::REFRESH_TOKEN);
                *self.account.lock().await = None;
                *self.access.lock().await = None;
                return Err(AppError::message(
                    "La sesión de Google expiró. Vuelve a iniciar sesión.",
                ));
            }
            Err(err) => return Err(err),
        };
        if let Some(refresh) = &tokens.refresh_token {
            self.secrets.set(oauth::REFRESH_TOKEN, refresh)?;
        }
        let access = tokens.access_token.clone();
        if self.account.lock().await.is_none() {
            let mut account = fetch_profile(&self.http, &access).await?;
            account.scopes = tokens.scopes.clone();
            *self.account.lock().await = Some(account);
        }
        *self.access.lock().await = Some(CachedAccess {
            token: tokens.access_token,
            expires_at: tokens.expires_at,
        });
        Ok(access)
    }

    async fn summarize(&self, manual: bool) -> String {
        let prefs = self.prefs.lock().await.clone();
        let now = self.clock.now();
        if !manual && !should_prepare_summary(now, &prefs) {
            return String::new();
        }
        let question = "Escribe un párrafo de cinco o seis frases sobre cómo viene el día, los huecos y, si hay correo reciente, lo que ese correo pide y todavía no está en la agenda ni en las tareas. No repitas la lista de reuniones ni de tareas. No inventes mensajes. Después, solo si el correo trae algo que la app no muestra, escribe hasta tres líneas con el formato SUGERENCIA|título|detalle|cta. Cada sugerencia es una acción nueva: responder, preparar o dar seguimiento. Si el correo está vacío, no escribas líneas SUGERENCIA.";
        let (system, prompt, local, events, tasks) = self.prompt_parts(question).await;
        if !prefs.service_gemini {
            self.ai_suggestions.lock().await.clear();
            return local;
        }
        let key = self.secrets.get(oauth::GEMINI_KEY).ok().flatten().unwrap_or_default();
        let assistant = GeminiAssistant::new(self.http.clone(), key);
        if !assistant.has_key() {
            self.ai_suggestions.lock().await.clear();
            return local;
        }
        match assistant.complete(&system, &prompt).await {
            Ok(text) => {
                let (summary, ideas) = crate::domain::split_briefing(&text);
                *self.ai_suggestions.lock().await = crate::domain::keep_novel(ideas, &events, &tasks);
                if summary.is_empty() { text } else { summary }
            }
            Err(_) => {
                self.ai_suggestions.lock().await.clear();
                local
            }
        }
    }

    async fn prompt(&self, question: &str) -> (String, String) {
        let (system, prompt, _, _, _) = self.prompt_parts(question).await;
        (system, prompt)
    }

    async fn prompt_parts(
        &self,
        question: &str,
    ) -> (String, String, String, Vec<crate::domain::CalendarEvent>, Vec<crate::domain::TaskItem>) {
        let now = self.clock.now();
        let prefs = self.prefs.lock().await.clone();
        let events = self.events.lock().await.clone();
        let tasks = self.tasks.lock().await.clone();
        let notes = self.notes.lock().await.clone();
        let connected = self.account.lock().await.is_some();
        let mail = self.load_mail(&prefs).await;
        let ctx = DayContext {
            now,
            events: &events,
            tasks: &tasks,
            notes: &notes,
            mail: &mail,
            connected,
        };
        let local = local_summary(&ctx, &prefs);
        let (system, prompt) = assistant_prompt(&ctx, &prefs, question);
        (system, prompt, local, events, tasks)
    }

    async fn load_mail(&self, prefs: &crate::domain::Preferences) -> Vec<crate::domain::MailHint> {
        if !prefs.read_mail {
            self.mail_cache.lock().await.clear();
            return Vec::new();
        }
        let allowed = self
            .account
            .lock()
            .await
            .as_ref()
            .is_some_and(|account| account.scopes.iter().any(|scope| scope.contains("gmail.readonly")));
        if !allowed {
            return self.mail_cache.lock().await.clone();
        }
        let Ok(api) = self.api().await else {
            return self.mail_cache.lock().await.clone();
        };
        match api.recent_mail().await {
            Ok(mail) => {
                *self.mail_cache.lock().await = mail.clone();
                mail
            }
            Err(_) => self.mail_cache.lock().await.clone(),
        }
    }

    async fn meeting_side_effects(&self, prefs: &Preferences, now: DateTime<Local>) {
        let events = self.events.lock().await.clone();
        let connected = self.account.lock().await.is_some();
        let (links, links_pending) = self.agenda_links().await;
        let agenda = crate::domain::build_agenda_with(now, &events, prefs, connected, &links, links_pending);
        let sent = self.notified.lock().await.clone();
        for alert in crate::domain::pending_join_alerts(now, &agenda.next, prefs.notify_lead_secs, links_pending, &sent) {
            self.notified.lock().await.insert(alert.key.clone(), ());
            let _ = self.deliver_alert(&alert);
        }
        let until = (agenda.next.starts_at - now).num_seconds();
        if prefs.auto_join && until <= 0 && until >= -20 && !agenda.next.join_url.is_empty() {
            let mut joined = self.joined.lock().await;
            if !joined.contains_key(&agenda.next.event_id) {
                joined.insert(agenda.next.event_id.clone(), ());
                drop(joined);
                let _ = self.open_meeting(&agenda.next.join_url).await;
            }
        }
        let in_progress = agenda.next.starts_at <= now && agenda.next.ends_at > now;
        if prefs.pause_on_meeting && in_progress {
            let mut paused = self.paused_for.lock().await;
            if paused.as_deref() != Some(agenda.next.event_id.as_str()) {
                let mut pomodoro = self.pomodoro.lock().await;
                if pomodoro.running && pomodoro.mode == PomodoroMode::Focus {
                    pomodoro.running = false;
                    *paused = Some(agenda.next.event_id.clone());
                }
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn arm_alerts(&self) {
        use tauri_plugin_notification::{Action, ActionType, NotificationExt};
        let _ = self.app.notification().register_action_types(vec![ActionType::builder("join")
            .actions(vec![Action::builder("join", "Unirse").build()])
            .build()]);
        let app = self.app.clone();
        let _ = self.app.notification().on_action(move |performed| {
            let action = performed.action_id();
            if action != "tap" && action != "join" {
                return;
            }
            let Some(url) = performed
                .notification()
                .and_then(|note| note.extra().get("url"))
                .and_then(|value| value.as_str())
                .filter(|url| !url.is_empty())
                .map(str::to_string)
            else {
                return;
            };
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Some(state) = app.try_state::<crate::application::AppState>() {
                    let _ = state.inner.open_meeting(&url).await;
                }
            });
        });
    }

    pub async fn preview_alert(&self) -> Result<String, AppError> {
        #[cfg(target_os = "macos")]
        {
            let app = self.app.clone();
            tokio::task::spawn_blocking(move || ensure_notification_permission(&app))
                .await
                .map_err(|_| AppError::message("No se pudo pedir permiso para los avisos."))?
                .map_err(AppError::message)?;
        }
        self.deliver_alert(&crate::domain::JoinAlert {
            key: "preview".into(),
            title: "Reunión de prueba".into(),
            body: "Ya puedes unirte. Si ves este aviso, las alertas funcionan.".into(),
            url: "https://meet.google.com/".into(),
        })
        .map_err(AppError::message)?;
        Ok("Aviso enviado. Mira la esquina de la pantalla.".into())
    }

    fn deliver_alert(&self, alert: &crate::domain::JoinAlert) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            let title = alert.title.clone();
            let body = alert.body.clone();
            let url = alert.url.clone();
            let app = self.app.clone();
            std::thread::spawn(move || post_native_alert(app, title, body, url));
            return Ok(());
        }
        #[cfg(not(target_os = "macos"))]
        {
            use tauri_plugin_notification::NotificationExt;
            let mut builder = self.app.notification().builder().title(&alert.title).body(&alert.body);
            if !alert.url.is_empty() {
                builder = builder.action_type_id("join").extra("url", alert.url.clone());
            }
            builder.show().map_err(|err| err.to_string())
        }
    }

    fn update_tray(&self, tick: &TickDto) {
        let Some(tray) = self.app.tray_by_id("main") else {
            return;
        };
        let title = tick.tray_title.clone();
        #[cfg(target_os = "macos")]
        {
            if title.is_empty() {
                let _ = tray.set_title(None::<&str>);
            } else {
                let _ = tray.set_title(Some(title.as_str()));
            }
        }
        let tooltip = if title.is_empty() {
            "Ritmo".to_string()
        } else {
            format!("Ritmo  {title}")
        };
        let _ = tray.set_tooltip(Some(tooltip));
    }

    fn set_autostart(&self, enabled: bool) -> Result<(), AppError> {
        use tauri_plugin_autostart::ManagerExt;
        let manager = self.app.autolaunch();
        let result = if enabled { manager.enable() } else { manager.disable() };
        result.map_err(|err| AppError::message(format!("No se pudo cambiar el arranque: {err}")))
    }

    pub async fn finish_setup(&self) -> Result<(), AppError> {
        self.setup_done.store(true, Ordering::Relaxed);
        self.persist().await?;
        if let Some(window) = self.app.get_webview_window("welcome") {
            let _ = window.close();
        }
        Ok(())
    }

    /// The welcome window's "Permitir" button. Same checks as the preview alert,
    /// so a denied or unsigned copy explains itself instead of failing silently.
    pub async fn request_notifications(&self) -> Result<(), AppError> {
        #[cfg(target_os = "macos")]
        {
            let app = self.app.clone();
            return tokio::task::spawn_blocking(move || ensure_notification_permission(&app))
                .await
                .map_err(|_| AppError::message("No se pudo pedir permiso para los avisos."))?
                .map_err(AppError::message);
        }
        #[cfg(not(target_os = "macos"))]
        Ok(())
    }

    /// True when macOS already lets Ritmo show alerts. Never prompts.
    pub async fn notifications_allowed(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            use mac_usernotifications::{AuthorizationStatus, NotificationSettingStatus};
            return tokio::task::spawn_blocking(|| {
                mac_usernotifications::blocking::get_notification_settings()
                    .map(|settings| {
                        matches!(
                            settings.authorization_status,
                            AuthorizationStatus::Authorized | AuthorizationStatus::Provisional | AuthorizationStatus::Ephemeral
                        ) && settings.alert_enabled != NotificationSettingStatus::Disabled
                    })
                    .unwrap_or(false)
            })
            .await
            .unwrap_or(false);
        }
        #[cfg(not(target_os = "macos"))]
        true
    }

    pub async fn persist(&self) -> Result<(), AppError> {
        let data = Persisted {
            preferences: self.prefs.lock().await.clone(),
            pomodoro: Some(self.pomodoro.lock().await.clone()),
            notes: self.notes.lock().await.clone(),
            account: self.account.lock().await.clone(),
            sync_tokens: self.sync_tokens.lock().await.clone(),
            events: self.events.lock().await.clone(),
            tasks: self.tasks.lock().await.clone(),
            calendar_revision: 2,
            setup_complete: self.setup_done.load(Ordering::Relaxed),
            factorial_events: self.factorial_events.lock().await.clone(),
            clickup_tasks: self.clickup_tasks.lock().await.clone(),
            work_logs: self.work_logs.lock().await.clone(),
            factorial_employee_id: self.factorial_employee.lock().await.clone(),
            clickup_team_id: self.clickup_team.lock().await.clone(),
        };
        persist::save(&self.app, &data)
    }
}

fn session_expired(err: &AppError) -> bool {
    match err {
        AppError::Message(text) => {
            let lower = text.to_ascii_lowercase();
            lower.contains("invalid_grant") || lower.contains("expired or revoked")
        }
        _ => false,
    }
}

