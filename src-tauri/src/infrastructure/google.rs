use async_trait::async_trait;
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};

use crate::domain::{
    AppError, AssistantPort, CalendarPort, EventChange, RemoteCalendar, SyncPage, SyncRequest, TaskItem,
    TaskPort,
};

pub struct GoogleApi {
    pub http: reqwest::Client,
    pub token: String,
}

impl GoogleApi {
    async fn send(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, AppError> {
        let mut request = self.http.request(method, url).bearer_auth(&self.token);
        if let Some(body) = body {
            let bytes = serde_json::to_vec(&body).map_err(|_| AppError::message("No se pudo preparar la solicitud."))?;
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json; charset=utf-8")
                .body(bytes);
        }
        let response = request
            .send()
            .await
            .map_err(|err| AppError::message(format!("Google no respondió: {err}")))?;
        let status = response.status();
        if status.as_u16() == 410 {
            return Err(AppError::SyncTokenExpired);
        }
        let text = response
            .text()
            .await
            .map_err(|err| AppError::message(err.to_string()))?;
        if text.trim().is_empty() {
            if status.is_success() {
                return Ok(serde_json::json!({}));
            }
            return Err(AppError::message(format!("Google respondió {status}")));
        }
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|_| AppError::message(format!("Respuesta inválida de Google ({status})")))?;
        if !status.is_success() {
            let message = value
                .pointer("/error/message")
                .and_then(|item| item.as_str())
                .unwrap_or("Google rechazó la solicitud");
            return Err(AppError::message(format!("{message} ({status})")));
        }
        Ok(value)
    }
}

#[async_trait]
impl CalendarPort for GoogleApi {
    async fn list_calendars(&self) -> Result<Vec<RemoteCalendar>, AppError> {
        let mut page: Option<String> = None;
        let mut calendars = Vec::new();
        loop {
            let mut url = "https://www.googleapis.com/calendar/v3/users/me/calendarList?maxResults=250".to_string();
            if let Some(token) = &page {
                url.push_str("&pageToken=");
                url.push_str(&urlencoding::encode(token));
            }
            let body = self.send(reqwest::Method::GET, &url, None).await?;
            for item in body.get("items").and_then(|v| v.as_array()).into_iter().flatten() {
                if let Some(calendar) = parse_calendar(item) {
                    calendars.push(calendar);
                }
            }
            page = body
                .get("nextPageToken")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if page.is_none() {
                break;
            }
        }
        Ok(calendars)
    }

    async fn sync_events(&self, request: SyncRequest) -> Result<SyncPage, AppError> {
        let mut changes = Vec::new();
        let mut page: Option<String> = None;
        let incremental = request.sync_token.is_some();
        let next_sync = loop {
            let mut url = format!(
                "https://www.googleapis.com/calendar/v3/calendars/{}/events?maxResults=250&singleEvents=true&showDeleted=true",
                urlencoding::encode(&request.calendar_id)
            );
            if let Some(token) = &request.sync_token {
                url.push_str("&syncToken=");
                url.push_str(&urlencoding::encode(token));
            } else {
                url.push_str("&timeMin=");
                url.push_str(&urlencoding::encode(&request.from.to_rfc3339()));
                url.push_str("&timeMax=");
                url.push_str(&urlencoding::encode(&request.to.to_rfc3339()));
            }
            if let Some(token) = &page {
                url.push_str("&pageToken=");
                url.push_str(&urlencoding::encode(token));
            }
            let body = self.send(reqwest::Method::GET, &url, None).await?;
            for item in body.get("items").and_then(|v| v.as_array()).into_iter().flatten() {
                if let Some(change) = parse_event(&request.calendar_id, &request.calendar_color, item) {
                    changes.push(change);
                }
            }
            page = body.get("nextPageToken").and_then(|v| v.as_str()).map(str::to_string);
            if page.is_none() {
                break body
                    .get("nextSyncToken")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
            }
        };
        Ok(SyncPage {
            changes,
            next_sync_token: next_sync,
            incremental,
        })
    }
}

#[async_trait]
impl TaskPort for GoogleApi {
    async fn list_tasks(&self) -> Result<Vec<TaskItem>, AppError> {
        let url = "https://tasks.googleapis.com/tasks/v1/lists/%40default/tasks?showCompleted=true&showHidden=false&maxResults=100";
        let body = self.send(reqwest::Method::GET, url, None).await?;
        Ok(body
            .get("items")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|item| parse_task(item, "@default"))
            .collect())
    }

    async fn add_task(&self, title: &str) -> Result<TaskItem, AppError> {
        let body = self
            .send(
                reqwest::Method::POST,
                "https://tasks.googleapis.com/tasks/v1/lists/%40default/tasks",
                Some(serde_json::json!({ "title": title })),
            )
            .await?;
        parse_task(&body, "@default").ok_or_else(|| AppError::message("Google no devolvió la tarea."))
    }

    async fn set_done(&self, task: &TaskItem, done: bool) -> Result<TaskItem, AppError> {
        let status = if done { "completed" } else { "needsAction" };
        let url = format!(
            "https://tasks.googleapis.com/tasks/v1/lists/{}/tasks/{}",
            urlencoding::encode(&task.list_id),
            urlencoding::encode(&task.id)
        );
        let body = self
            .send(
                reqwest::Method::PATCH,
                &url,
                Some(serde_json::json!({ "status": status })),
            )
            .await?;
        parse_task(&body, &task.list_id).ok_or_else(|| AppError::message("Google no devolvió la tarea."))
    }
}

pub fn parse_calendar(value: &serde_json::Value) -> Option<RemoteCalendar> {
    let id = value.get("id")?.as_str()?.to_string();
    let title = value
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("Calendario")
        .to_string();
    let primary = value.get("primary").and_then(|v| v.as_bool()).unwrap_or(false);
    let subtitle = if primary {
        id.clone()
    } else if id.contains('@') {
        "Compartido".into()
    } else {
        "Calendario".into()
    };
    Some(RemoteCalendar {
        id,
        title,
        color: value
            .get("backgroundColor")
            .and_then(|v| v.as_str())
            .unwrap_or("#0a84ff")
            .to_string(),
        subtitle,
        google_selected: value.get("selected").and_then(|v| v.as_bool()).unwrap_or(true),
    })
}

pub fn parse_event(calendar_id: &str, color: &str, value: &serde_json::Value) -> Option<EventChange> {
    let id = value.get("id")?.as_str()?.to_string();
    let event_type = value.get("eventType").and_then(|v| v.as_str()).unwrap_or("default");
    let cancelled = value.get("status").and_then(|v| v.as_str()) == Some("cancelled") || event_type == "workingLocation";
    let (start, all_day) = parse_when(value.get("start")?)?;
    let end = value
        .get("end")
        .and_then(parse_when)
        .map(|(end, _)| end)
        .unwrap_or_else(|| start + Duration::hours(1));
    let declined = cancelled
        || value
            .get("attendees")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .any(|person| {
                person.get("self").and_then(|v| v.as_bool()) == Some(true)
                    && person.get("responseStatus").and_then(|v| v.as_str()) == Some("declined")
            });
    Some(EventChange {
        deleted: cancelled,
        event: crate::domain::CalendarEvent {
            id,
            calendar_id: calendar_id.to_string(),
            calendar_color: color.to_string(),
            title: value
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("(Sin título)")
                .to_string(),
            start,
            end,
            all_day,
            location: value.get("location").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            description: value
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            declined,
            conference_url: conference_url(value),
            event_type: value.get("eventType").and_then(|v| v.as_str()).unwrap_or("default").to_string(),
        },
    })
}

fn conference_url(value: &serde_json::Value) -> String {
    let entries = value
        .pointer("/conferenceData/entryPoints")
        .and_then(|v| v.as_array());
    if let Some(entries) = entries {
        let video = entries.iter().find(|entry| {
            entry.get("entryPointType").and_then(|v| v.as_str()) == Some("video")
        });
        if let Some(uri) = video.and_then(|entry| entry.get("uri").and_then(|v| v.as_str())) {
            if !uri.is_empty() {
                return uri.to_string();
            }
        }
    }
    value
        .get("hangoutLink")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

pub fn parse_task(value: &serde_json::Value, list_id: &str) -> Option<TaskItem> {
    let id = value.get("id")?.as_str()?.to_string();
    let due = value
        .get("due")
        .and_then(|v| v.as_str())
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map(|dt| dt.with_timezone(&Utc).date_naive());
    Some(TaskItem {
        id,
        title: value.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        due,
        done: value.get("status").and_then(|v| v.as_str()) == Some("completed"),
        list_id: value
            .get("listId")
            .and_then(|v| v.as_str())
            .unwrap_or(list_id)
            .to_string(),
        updated_at: value
            .get("updated")
            .and_then(|v| v.as_str())
            .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
            .map(|dt| dt.with_timezone(&Local))
            .unwrap_or_else(|| DateTime::UNIX_EPOCH.with_timezone(&Local)),
        completed_at: value
            .get("completed")
            .and_then(|v| v.as_str())
            .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
            .map(|dt| dt.with_timezone(&Local)),
    })
}


fn parse_when(value: &serde_json::Value) -> Option<(DateTime<Local>, bool)> {
    if let Some(date) = value.get("date").and_then(|v| v.as_str()) {
        let naive = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
        let local = naive
            .and_hms_opt(0, 0, 0)?
            .and_local_timezone(Local)
            .earliest()?;
        return Some((local, true));
    }
    let text = value.get("dateTime").and_then(|v| v.as_str())?;
    Some((parse_timestamp(text)?, false))
}

fn parse_timestamp(text: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|dt| dt.with_timezone(&Local))
}


pub struct GeminiAssistant {
    http: reqwest::Client,
    key: String,
}

impl GeminiAssistant {
    pub fn new(http: reqwest::Client, key: impl Into<String>) -> Self {
        Self {
            http,
            key: key.into(),
        }
    }
}

#[async_trait]
impl AssistantPort for GeminiAssistant {
    async fn complete(&self, system: &str, prompt: &str) -> Result<String, AppError> {
        gemini_complete(&self.http, self.key.trim(), system, prompt).await
    }

    fn has_key(&self) -> bool {
        !self.key.trim().is_empty()
    }
}

async fn gemini_complete(
    http: &reqwest::Client,
    key: &str,
    system: &str,
    prompt: &str,
) -> Result<String, AppError> {
    let url = "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent";
    let response = http
        .post(url)
        .header("x-goog-api-key", key)
        .json(&serde_json::json!({
            "systemInstruction": { "parts": [{ "text": system }] },
            "contents": [{ "role": "user", "parts": [{ "text": prompt }] }],
            "generationConfig": { "maxOutputTokens": 400, "temperature": 0.4 }
        }))
        .send()
        .await
        .map_err(|err| AppError::message(format!("Gemini no respondió: {err}")))?;
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|err| AppError::message(err.to_string()))?;
    if !status.is_success() {
        let message = body
            .pointer("/error/message")
            .and_then(|v| v.as_str())
            .unwrap_or("Gemini rechazó la solicitud");
        return Err(AppError::message(message.to_string()));
    }
    body.pointer("/candidates/0/content/parts/0/text")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AppError::message("Gemini no devolvió texto."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_timed_event_and_a_declined_attendee() {
        let value = serde_json::json!({
            "id": "evt",
            "summary": "Daily",
            "description": "https://meet.google.com/abc-defg-hij",
            "start": { "dateTime": "2026-10-29T15:00:00Z" },
            "end": { "dateTime": "2026-10-29T15:30:00Z" },
            "attendees": [{ "self": true, "responseStatus": "declined" }]
        });
        let change = parse_event("work", "#0a84ff", &value).unwrap();
        assert!(change.event.declined);
        assert_eq!(change.event.title, "Daily");
        assert!(!change.event.all_day);
    }

    #[test]
    fn a_working_location_is_dropped() {
        let value = serde_json::json!({
            "id": "loc",
            "eventType": "workingLocation",
            "summary": "Office",
            "start": { "date": "2026-10-30" },
            "end": { "date": "2026-10-31" }
        });
        let change = parse_event("work", "#fff", &value).unwrap();
        assert!(change.deleted);
        assert_eq!(change.event.event_type, "workingLocation");
    }

    #[test]
    fn the_meet_link_is_kept_when_the_description_is_empty() {
        let value = serde_json::json!({
            "id": "board",
            "summary": "Reunión Junta Directiva",
            "start": { "dateTime": "2026-10-30T21:00:00Z" },
            "end": { "dateTime": "2026-10-30T22:00:00Z" },
            "hangoutLink": "https://meet.google.com/abc-defg-hij",
            "conferenceData": {
                "entryPoints": [
                    { "entryPointType": "video", "uri": "https://meet.google.com/abc-defg-hij" }
                ]
            }
        });
        let change = parse_event("work", "#fff", &value).unwrap();
        assert!(!change.deleted);
        assert_eq!(change.event.description, "");
        assert_eq!(change.event.conference_url, "https://meet.google.com/abc-defg-hij");
    }


    #[test]
    fn parses_task_due_as_utc_date() {
        let value = serde_json::json!({
            "id": "t1",
            "title": "Enviar minuta",
            "status": "needsAction",
            "due": "2026-10-24T00:00:00.000Z"
        });
        let task = parse_task(&value, "@default").unwrap();
        assert_eq!(task.due, Some(NaiveDate::from_ymd_opt(2026, 10, 24).unwrap()));
        assert!(!task.done);
    }

}
