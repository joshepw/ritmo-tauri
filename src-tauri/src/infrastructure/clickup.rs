use std::collections::HashMap;
use std::time::Duration as StdDuration;

use chrono::{DateTime, Local};
use serde_json::Value;

use crate::domain::{AppError, TaskItem};

pub const TASK_PREFIX: &str = "clickup:";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub team_id: String,
    pub user_id: String,
}

pub fn is_clickup_task(id: &str) -> bool {
    id.starts_with(TASK_PREFIX)
}

pub fn raw_task_id(id: &str) -> Option<&str> {
    id.strip_prefix(TASK_PREFIX).filter(|id| !id.is_empty())
}

pub async fn check_key(http: &reqwest::Client, token: &str) -> Result<(), AppError> {
    let _ = get_json(http, token, "/user", &[]).await?;
    Ok(())
}

pub async fn workspace(http: &reqwest::Client, token: &str) -> Result<Workspace, AppError> {
    let user = get_json(http, token, "/user", &[]).await?;
    let user_id = json_id(user.get("user").and_then(|user| user.get("id")).unwrap_or(&Value::Null))
        .ok_or_else(|| AppError::message("ClickUp no dijo qué usuario eres."))?;
    let teams = get_json(http, token, "/team", &[]).await?;
    let team_id = teams
        .get("teams")
        .and_then(Value::as_array)
        .and_then(|teams| teams.first())
        .and_then(|team| json_id(team.get("id").unwrap_or(&Value::Null)))
        .ok_or_else(|| AppError::message("ClickUp no devolvió un espacio de trabajo."))?;
    Ok(Workspace { team_id, user_id })
}

pub async fn assigned_tasks(
    http: &reqwest::Client,
    token: &str,
    workspace: &Workspace,
) -> Result<Vec<TaskItem>, AppError> {
    let spaces = space_names(http, token, &workspace.team_id).await.unwrap_or_default();
    let mut tasks = Vec::new();
    for page in 0..5 {
        let page_label = page.to_string();
        let body = get_json(
            http,
            token,
            &format!("/team/{}/task", workspace.team_id),
            &[
                ("assignees[]", workspace.user_id.as_str()),
                ("include_closed", "false"),
                ("subtasks", "true"),
                ("page", page_label.as_str()),
            ],
        )
        .await?;
        let batch = body.get("tasks").and_then(Value::as_array).cloned().unwrap_or_default();
        let count = batch.len();
        tasks.extend(batch.iter().filter_map(|task| task_from(task, &spaces)));
        if count < 100 {
            break;
        }
    }
    Ok(tasks)
}

pub async fn complete_task(http: &reqwest::Client, token: &str, task_id: &str) -> Result<(), AppError> {
    let Some(raw) = raw_task_id(task_id) else {
        return Err(AppError::message("No encontré esa tarea."));
    };
    let task = get_json(http, token, &format!("/task/{raw}"), &[]).await?;
    let list_id = task
        .get("list")
        .and_then(|list| list.get("id"))
        .and_then(json_id)
        .ok_or_else(|| AppError::message("ClickUp no dijo en qué lista está la tarea."))?;
    let list = get_json(http, token, &format!("/list/{list_id}"), &[]).await?;
    let status = closed_status_name(&list)
        .ok_or_else(|| AppError::message("Esa lista de ClickUp no tiene un estado de completado."))?;
    let response = http
        .put(format!("https://api.clickup.com/api/v2/task/{raw}"))
        .header("Authorization", token)
        .header("accept", "application/json")
        .timeout(StdDuration::from_secs(20))
        .json(&serde_json::json!({ "status": status }))
        .send()
        .await
        .map_err(|_| AppError::message("No se pudo contactar con ClickUp."))?;
    let _ = read_json(response).await?;
    Ok(())
}

pub async fn log_time(
    http: &reqwest::Client,
    token: &str,
    team_id: &str,
    task_id: &str,
    start: DateTime<Local>,
    end: DateTime<Local>,
) -> Result<(), AppError> {
    let Some(task_id) = raw_task_id(task_id) else {
        return Ok(());
    };
    let body = serde_json::json!({
        "tid": task_id,
        "start": start.timestamp_millis(),
        "end": end.timestamp_millis(),
        "description": "Ritmo",
    });
    let response = http
        .post(format!("https://api.clickup.com/api/v2/team/{team_id}/time_entries"))
        .header("Authorization", token)
        .header("accept", "application/json")
        .timeout(StdDuration::from_secs(20))
        .json(&body)
        .send()
        .await
        .map_err(|_| AppError::message("No se pudo contactar con ClickUp."))?;
    let _ = read_json(response).await?;
    Ok(())
}

pub fn parse_tasks(body: &str) -> Result<Vec<TaskItem>, AppError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|_| AppError::message("ClickUp devolvió una respuesta ilegible."))?;
    Ok(value
        .get("tasks")
        .and_then(Value::as_array)
        .map(|tasks| tasks.iter().filter_map(|task| task_from(task, &HashMap::new())).collect())
        .unwrap_or_default())
}

async fn get_json(
    http: &reqwest::Client,
    token: &str,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Value, AppError> {
    let response = http
        .get(format!("https://api.clickup.com/api/v2{path}"))
        .header("Authorization", token)
        .header("accept", "application/json")
        .timeout(StdDuration::from_secs(20))
        .query(query)
        .send()
        .await
        .map_err(|_| AppError::message("No se pudo contactar con ClickUp."))?;
    read_json(response).await
}

async fn read_json(response: reqwest::Response) -> Result<Value, AppError> {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(AppError::message("ClickUp rechazó la clave."));
    }
    if !status.is_success() {
        return Err(AppError::message(format!("ClickUp respondió {status}.")));
    }
    if body.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&body).map_err(|_| AppError::message("ClickUp devolvió una respuesta ilegible."))
}

async fn space_names(
    http: &reqwest::Client,
    token: &str,
    team_id: &str,
) -> Result<HashMap<String, String>, AppError> {
    let body = get_json(http, token, &format!("/team/{team_id}/space"), &[("archived", "false")]).await?;
    let mut names = HashMap::new();
    for space in body.get("spaces").and_then(Value::as_array).into_iter().flatten() {
        let Some(id) = space.get("id").and_then(json_id) else {
            continue;
        };
        let Some(name) = text_name(space.get("name")) else {
            continue;
        };
        names.insert(id, name);
    }
    Ok(names)
}

fn task_from(value: &Value, spaces: &HashMap<String, String>) -> Option<TaskItem> {
    let id = json_id(value.get("id")?)?;
    let status = value.get("status").and_then(|status| status.get("type")).and_then(Value::as_str);
    if status == Some("closed") {
        return None;
    }
    let title = value.get("name").and_then(Value::as_str).unwrap_or("").trim().to_string();
    if title.is_empty() {
        return None;
    }
    Some(TaskItem {
        id: format!("{TASK_PREFIX}{id}"),
        title,
        due: due_date(value.get("due_date")),
        done: false,
        list_id: place_name(value, spaces),
        updated_at: clickup_time(value, "date_updated")
            .or_else(|| clickup_time(value, "date_created"))
            .unwrap_or_else(Local::now),
        completed_at: None,
    })
}

fn place_name(value: &Value, spaces: &HashMap<String, String>) -> String {
    if let Some(name) = container_name(value.get("folder")).or_else(|| container_name(value.get("project"))) {
        return name;
    }
    let space_id = value
        .get("space")
        .and_then(|space| space.get("id"))
        .and_then(json_id)
        .unwrap_or_default();
    if let Some(name) = spaces.get(&space_id) {
        return name.clone();
    }
    if let Some(name) = text_name(value.get("space").and_then(|space| space.get("name"))) {
        return name;
    }
    text_name(value.get("list").and_then(|list| list.get("name"))).unwrap_or_default()
}

fn container_name(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if value.get("hidden").and_then(Value::as_bool).unwrap_or(false) {
        return None;
    }
    text_name(value.get("name")).filter(|name| !name.eq_ignore_ascii_case("hidden"))
}

fn text_name(value: Option<&Value>) -> Option<String> {
    let name = value?.as_str()?.trim();
    if name.is_empty() { None } else { Some(name.to_string()) }
}

fn clickup_time(value: &Value, field: &str) -> Option<DateTime<Local>> {
    let millis = match value.get(field)? {
        Value::String(text) => text.parse::<i64>().ok()?,
        Value::Number(number) => number.as_i64()?,
        _ => return None,
    };
    DateTime::from_timestamp_millis(millis).map(|time| time.with_timezone(&Local))
}

fn due_date(value: Option<&Value>) -> Option<chrono::NaiveDate> {
    let millis = match value? {
        Value::String(text) => text.parse::<i64>().ok()?,
        Value::Number(number) => number.as_i64()?,
        _ => return None,
    };
    DateTime::from_timestamp_millis(millis).map(|time| time.with_timezone(&Local).date_naive())
}

fn closed_status_name(list: &Value) -> Option<String> {
    let statuses = list.get("statuses").and_then(Value::as_array)?;
    status_named(statuses, "closed").or_else(|| status_named(statuses, "done"))
}

fn status_named(statuses: &[Value], kind: &str) -> Option<String> {
    statuses.iter().find_map(|status| {
        if status.get("type").and_then(Value::as_str) != Some(kind) {
            return None;
        }
        let name = status.get("status").and_then(Value::as_str)?.trim();
        if name.is_empty() { None } else { Some(name.to_string()) }
    })
}

fn json_id(value: &Value) -> Option<String> {
    match value {
        Value::String(id) if !id.is_empty() => Some(id.clone()),
        Value::Number(id) => Some(id.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assigned_tasks_show_the_folder_instead_of_the_list() {
        let tasks = parse_tasks(
            r#"{"tasks":[
                {"id":"abc","name":"Informe","due_date":"1762340400000","status":{"type":"open"},"list":{"name":"Backlog"},"folder":{"name":"Sitio web","hidden":false}},
                {"id":"done","name":"Cerrada","status":{"type":"closed"},"list":{"name":"Backlog"},"folder":{"name":"Sitio web"}}
            ]}"#,
        )
        .unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "clickup:abc");
        assert_eq!(tasks[0].title, "Informe");
        assert_eq!(tasks[0].list_id, "Sitio web");
        assert!(tasks[0].due.is_some());
        assert!(is_clickup_task(&tasks[0].id));
        assert_eq!(raw_task_id("task-1"), None);
    }

    #[test]
    fn a_list_without_folder_uses_the_space() {
        let task = serde_json::json!({
            "id": "abc",
            "name": "Informe",
            "status": { "type": "open" },
            "list": { "name": "Backlog" },
            "folder": { "name": "hidden", "hidden": true },
            "space": { "id": "9" }
        });
        let spaces = HashMap::from([("9".into(), "Cliente".into())]);
        assert_eq!(task_from(&task, &spaces).unwrap().list_id, "Cliente");
    }

    #[test]
    fn a_list_closes_with_its_own_completed_status() {
        let named = serde_json::json!({
            "statuses": [
                { "status": "por hacer", "type": "open" },
                { "status": "hecho", "type": "done" },
                { "status": "cerrada", "type": "closed" }
            ]
        });
        assert_eq!(closed_status_name(&named).as_deref(), Some("cerrada"));
        let only_done = serde_json::json!({
            "statuses": [{ "status": "listo", "type": "done" }]
        });
        assert_eq!(closed_status_name(&only_done).as_deref(), Some("listo"));
    }
}
