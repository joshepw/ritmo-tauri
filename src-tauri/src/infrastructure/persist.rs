use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::domain::{
    migrate_chat_ui, Account, AppError, CalendarEvent, Note, PomodoroState, Preferences, TaskItem, WorkLog,
};

const KEY: &str = "ritmo";

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Persisted {
    pub preferences: Preferences,
    pub pomodoro: Option<PomodoroState>,
    pub notes: Vec<Note>,
    pub account: Option<Account>,
    pub sync_tokens: HashMap<String, String>,
    pub events: Vec<CalendarEvent>,
    pub tasks: Vec<TaskItem>,
    #[serde(default)]
    pub calendar_revision: u32,
    /// The welcome window stays a normal app until this is true.
    #[serde(default)]
    pub setup_complete: bool,
    #[serde(default)]
    pub factorial_events: Vec<CalendarEvent>,
    #[serde(default)]
    pub clickup_tasks: Vec<TaskItem>,
    #[serde(default)]
    pub work_logs: Vec<WorkLog>,
    #[serde(default)]
    pub factorial_employee_id: Option<String>,
    #[serde(default)]
    pub clickup_team_id: Option<String>,
}

pub fn load(app: &AppHandle) -> Persisted {
    let Ok(store) = app.store("ritmo.json") else {
        return Persisted::default();
    };
    let Some(value) = store.get(KEY) else {
        return Persisted::default();
    };
    let mut data: Persisted = serde_json::from_value(json_clone(&value)).unwrap_or_default();
    migrate_chat_ui(&mut data.preferences);
    // Older copies never stored the conference link or the working-location type.
    if data.calendar_revision < 2 {
        data.sync_tokens.clear();
        data.calendar_revision = 2;
    }
    data
}

pub fn save(app: &AppHandle, data: &Persisted) -> Result<(), AppError> {
    let store = app
        .store("ritmo.json")
        .map_err(|err| AppError::message(format!("No se pudo abrir las preferencias: {err}")))?;
    store.set(KEY, serde_json::to_value(data).unwrap_or_default());
    store
        .save()
        .map_err(|err| AppError::message(format!("No se pudieron guardar las preferencias: {err}")))
}

fn json_clone(value: &impl serde::Serialize) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or_default()
}
