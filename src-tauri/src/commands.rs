use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::application::present::Snapshot;
use crate::application::AppState;

#[tauri::command]
pub fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

fn failure(error: impl ToString) -> String {
    error.to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDto {
    pub text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenDto {
    pub message: String,
}

#[tauri::command]
pub async fn snapshot(state: State<'_, AppState>) -> Result<Snapshot, String> {
    Ok(state.inner.snapshot().await)
}

#[tauri::command]
pub async fn sign_in(state: State<'_, AppState>) -> Result<Snapshot, String> {
    state.inner.sign_in().await.map_err(failure)?;
    Ok(state.inner.snapshot().await)
}

#[tauri::command]
pub async fn sign_out(state: State<'_, AppState>) -> Result<Snapshot, String> {
    state.inner.sign_out().await.map_err(failure)?;
    Ok(state.inner.snapshot().await)
}

#[tauri::command]
pub async fn set_preferences(state: State<'_, AppState>, patch: Value) -> Result<Snapshot, String> {
    let patch = patch
        .as_object()
        .cloned()
        .ok_or_else(|| "Preferencia no válida.".to_string())?;
    state.inner.set_preferences(&patch).await.map_err(failure)?;
    Ok(state.inner.snapshot().await)
}

#[tauri::command]
pub async fn set_calendar_visible(
    state: State<'_, AppState>,
    id: String,
    visible: bool,
) -> Result<Snapshot, String> {
    state.inner.set_calendar(&id, visible).await.map_err(failure)?;
    Ok(state.inner.snapshot().await)
}

#[tauri::command]
pub async fn add_task(state: State<'_, AppState>, title: String) -> Result<Snapshot, String> {
    state.inner.add_task(&title).await.map_err(failure)?;
    Ok(state.inner.snapshot().await)
}

#[tauri::command]
pub async fn toggle_task(state: State<'_, AppState>, task_id: String) -> Result<Snapshot, String> {
    state.inner.toggle_task(&task_id).await.map_err(failure)?;
    Ok(state.inner.snapshot().await)
}

#[tauri::command]
pub async fn add_note(
    state: State<'_, AppState>,
    title: String,
    body: String,
) -> Result<Snapshot, String> {
    state.inner.add_note(title, body).await.map_err(failure)?;
    Ok(state.inner.snapshot().await)
}

#[tauri::command]
pub async fn pomodoro_action(
    state: State<'_, AppState>,
    action: String,
    task_id: Option<String>,
) -> Result<Snapshot, String> {
    state.inner.pomodoro_action(&action, task_id).await.map_err(failure)?;
    Ok(state.inner.snapshot().await)
}

#[tauri::command]
pub async fn open_meeting(state: State<'_, AppState>, url: String) -> Result<OpenDto, String> {
    let message = state.inner.open_meeting(&url).await.map_err(failure)?;
    Ok(OpenDto { message })
}

#[tauri::command]
pub async fn assistant_summary(
    state: State<'_, AppState>,
    manual: Option<bool>,
) -> Result<TextDto, String> {
    let text = if manual.unwrap_or(true) {
        state.inner.summary().await
    } else {
        state
            .inner
            .prepare_summary_if_due()
            .await
            .unwrap_or_default()
    };
    Ok(TextDto { text })
}

#[tauri::command]
pub async fn assistant_ask(state: State<'_, AppState>, question: String) -> Result<TextDto, String> {
    Ok(TextDto {
        text: state.inner.ask(&question).await.map_err(failure)?,
    })
}

#[tauri::command]
pub async fn set_gemini_key(state: State<'_, AppState>, key: String) -> Result<Snapshot, String> {
    state.inner.set_gemini_key(&key).await.map_err(failure)?;
    Ok(state.inner.snapshot().await)
}

#[tauri::command]
pub async fn clear_gemini_key(state: State<'_, AppState>) -> Result<Snapshot, String> {
    state.inner.clear_gemini_key().await.map_err(failure)?;
    Ok(state.inner.snapshot().await)
}

#[tauri::command]
pub async fn set_factorial_key(state: State<'_, AppState>, key: String) -> Result<Snapshot, String> {
    state.inner.set_factorial_key(&key).await.map_err(failure)?;
    Ok(state.inner.snapshot().await)
}

#[tauri::command]
pub async fn clear_factorial_key(state: State<'_, AppState>) -> Result<Snapshot, String> {
    state.inner.clear_factorial_key().await.map_err(failure)?;
    Ok(state.inner.snapshot().await)
}

#[tauri::command]
pub async fn set_clickup_key(state: State<'_, AppState>, key: String) -> Result<Snapshot, String> {
    state.inner.set_clickup_key(&key).await.map_err(failure)?;
    Ok(state.inner.snapshot().await)
}

#[tauri::command]
pub async fn clear_clickup_key(state: State<'_, AppState>) -> Result<Snapshot, String> {
    state.inner.clear_clickup_key().await.map_err(failure)?;
    Ok(state.inner.snapshot().await)
}

#[tauri::command]
pub async fn finish_setup(state: State<'_, AppState>) -> Result<(), String> {
    state.inner.finish_setup().await.map_err(failure)
}

#[tauri::command]
pub async fn request_notifications(state: State<'_, AppState>) -> Result<(), String> {
    state.inner.request_notifications().await.map_err(failure)
}

#[tauri::command]
pub async fn notifications_allowed(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.inner.notifications_allowed().await)
}

#[tauri::command]
pub fn video_apps() -> Vec<crate::shell::VideoApp> {
    crate::shell::video_apps()
}

#[tauri::command]
pub async fn preview_notification(state: State<'_, AppState>) -> Result<TextDto, String> {
    let text = state.inner.preview_alert().await.map_err(failure)?;
    Ok(TextDto { text })
}

#[tauri::command]
pub fn show_chat_window(app: tauri::AppHandle) -> Result<(), String> {
    crate::shell::show_chat_window(&app)
}

#[tauri::command]
pub fn hide_chat_window(app: tauri::AppHandle) -> Result<(), String> {
    crate::shell::hide_chat_window(&app)
}

#[tauri::command]
pub fn show_gemini_window(app: tauri::AppHandle) -> Result<(), String> {
    crate::shell::show_gemini_window(&app)
}

#[tauri::command]
pub fn hide_gemini_window(app: tauri::AppHandle) -> Result<(), String> {
    crate::shell::hide_gemini_window(&app)
}

#[tauri::command]
pub fn show_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    crate::shell::show_settings_window(&app)
}

#[tauri::command]
pub fn hide_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    crate::shell::hide_settings_window(&app)
}
