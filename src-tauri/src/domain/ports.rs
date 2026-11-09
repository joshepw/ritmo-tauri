use async_trait::async_trait;
use chrono::{DateTime, Local};

use crate::domain::error::AppError;
use crate::domain::records::{EventChange, TaskItem};

#[derive(Clone, Debug)]
pub struct SyncRequest {
    pub calendar_id: String,
    pub calendar_color: String,
    pub sync_token: Option<String>,
    pub from: DateTime<Local>,
    pub to: DateTime<Local>,
}

#[derive(Clone, Debug)]
pub struct SyncPage {
    pub changes: Vec<EventChange>,
    pub next_sync_token: Option<String>,
    pub incremental: bool,
}

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Local>;
}

pub trait SecretPort: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, AppError>;
    fn set(&self, key: &str, value: &str) -> Result<(), AppError>;
    fn delete(&self, key: &str) -> Result<(), AppError>;
}

#[async_trait]
pub trait CalendarPort: Send + Sync {
    async fn list_calendars(&self) -> Result<Vec<RemoteCalendar>, AppError>;
    async fn sync_events(&self, request: SyncRequest) -> Result<SyncPage, AppError>;
}

#[async_trait]
pub trait TaskPort: Send + Sync {
    async fn list_tasks(&self) -> Result<Vec<TaskItem>, AppError>;
    async fn add_task(&self, title: &str) -> Result<TaskItem, AppError>;
    async fn set_done(&self, task: &TaskItem, done: bool) -> Result<TaskItem, AppError>;
}

#[async_trait]
pub trait AssistantPort: Send + Sync {
    async fn complete(&self, system: &str, prompt: &str) -> Result<String, AppError>;
    fn has_key(&self) -> bool;
}

#[derive(Clone, Debug)]
pub struct RemoteCalendar {
    pub id: String,
    pub title: String,
    pub color: String,
    pub subtitle: String,
    pub google_selected: bool,
}


