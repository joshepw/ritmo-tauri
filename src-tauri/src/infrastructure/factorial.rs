use std::time::Duration as StdDuration;

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone};
use serde_json::Value;

use crate::domain::{AppError, CalendarEvent};

const API: &str = "https://api.factorialhr.com/api/2026-10-01/resources";
const COLOR: &str = "#ff5a36";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Employee {
    pub id: String,
    pub name: String,
    pub email: String,
    pub login_email: String,
    pub birthday: Option<NaiveDate>,
    pub terminated_on: Option<NaiveDate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leave {
    pub id: String,
    pub employee_id: String,
    pub start_on: NaiveDate,
    pub finish_on: Option<NaiveDate>,
    pub approved: Option<bool>,
    pub leave_type: String,
    pub half_day: bool,
}

#[derive(Clone, Debug)]
pub struct Agenda {
    pub events: Vec<CalendarEvent>,
    pub employee_id: Option<String>,
}

pub enum ClockResult {
    Saved,
    OpenShift,
    UnknownEmployee,
}

pub async fn check_key(http: &reqwest::Client, key: &str) -> Result<(), AppError> {
    let _ = get_json(http, key, "/employees/employees", &[("only_active", "true"), ("limit", "1")]).await?;
    Ok(())
}

pub async fn load(
    http: &reqwest::Client,
    key: &str,
    account_email: Option<&str>,
    today: NaiveDate,
    horizon: NaiveDate,
) -> Result<Agenda, AppError> {
    let employees = employees(http, key).await?;
    let leaves = leaves(http, key, today, horizon).await?;
    let employee_id = match_employee(&employees, account_email);
    let events = agenda_events(&employees, &leaves, employee_id.as_deref(), today, horizon);
    Ok(Agenda { events, employee_id })
}

pub async fn clock_interval(
    http: &reqwest::Client,
    key: &str,
    employee_id: &str,
    start: DateTime<Local>,
    end: DateTime<Local>,
) -> Result<ClockResult, AppError> {
    if employee_id.is_empty() {
        return Ok(ClockResult::UnknownEmployee);
    }
    let open = get_json(
        http,
        key,
        "/attendance/open_shifts",
        &[("employee_ids[]", employee_id)],
    )
    .await?;
    if !rows(&open).is_empty() {
        return Ok(ClockResult::OpenShift);
    }
    let body = serde_json::json!({
        "employee_id": employee_id,
        "date": start.date_naive().format("%Y-%m-%d").to_string(),
        "clock_in": start.to_rfc3339(),
        "clock_out": end.to_rfc3339(),
        "observations": "Ritmo",
    });
    let response = http
        .post(format!("{API}/attendance/shifts"))
        .header("x-api-key", key)
        .header("accept", "application/json")
        .timeout(StdDuration::from_secs(20))
        .json(&body)
        .send()
        .await
        .map_err(|_| AppError::message("No se pudo contactar con Factorial."))?;
    let _ = read_json(response).await?;
    Ok(ClockResult::Saved)
}

pub fn match_employee(employees: &[Employee], account_email: Option<&str>) -> Option<String> {
    let email = account_email?.trim().to_ascii_lowercase();
    if email.is_empty() {
        return None;
    }
    employees
        .iter()
        .find(|employee| employee.login_email.eq_ignore_ascii_case(&email))
        .or_else(|| {
            employees
                .iter()
                .find(|employee| employee.email.eq_ignore_ascii_case(&email))
        })
        .map(|employee| employee.id.clone())
}

pub fn agenda_events(
    employees: &[Employee],
    leaves: &[Leave],
    me: Option<&str>,
    today: NaiveDate,
    horizon: NaiveDate,
) -> Vec<CalendarEvent> {
    let mut events = Vec::new();
    let mut day = today;
    while day <= horizon {
        for employee in employees {
            if employee.terminated_on.is_some_and(|ended| ended <= day) {
                continue;
            }
            if employee.birthday.is_some_and(|birthday| birthday.month() == day.month() && birthday.day() == day.day())
            {
                let mine = me == Some(employee.id.as_str());
                events.push(all_day(
                    format!("factorial:birthday:{}:{day}", employee.id),
                    if mine {
                        "Tu cumpleaños".into()
                    } else {
                        format!("Cumpleaños de {}", employee.name)
                    },
                    "Cumpleaños".into(),
                    day,
                ));
            }
        }
        for leave in leaves {
            if leave.approved != Some(true) || leave.start_on > day {
                continue;
            }
            if leave.finish_on.is_some_and(|finish| finish < day) {
                continue;
            }
            let employee = employees.iter().find(|employee| employee.id == leave.employee_id);
            if employee.is_some_and(|employee| employee.terminated_on.is_some_and(|ended| ended <= day)) {
                continue;
            }
            let name = employee.map(|employee| employee.name.as_str()).unwrap_or("Compañero");
            let mine = me == Some(leave.employee_id.as_str());
            let mut detail = if leave.leave_type.is_empty() {
                "Ausencia".into()
            } else {
                leave.leave_type.clone()
            };
            if leave.half_day {
                detail = format!("{detail} · medio día");
            }
            events.push(all_day(
                format!("factorial:leave:{}:{day}", leave.id),
                if mine { "Ausente".into() } else { name.into() },
                detail,
                day,
            ));
        }
        day += Duration::days(1);
    }
    events
}

pub fn parse_employees(body: &str) -> Result<Vec<Employee>, AppError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|_| AppError::message("Factorial devolvió una respuesta ilegible."))?;
    Ok(rows(&value)
        .into_iter()
        .filter_map(employee_from)
        .collect())
}

pub fn parse_leaves(body: &str) -> Result<Vec<Leave>, AppError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|_| AppError::message("Factorial devolvió una respuesta ilegible."))?;
    Ok(rows(&value).into_iter().filter_map(leave_from).collect())
}

async fn employees(http: &reqwest::Client, key: &str) -> Result<Vec<Employee>, AppError> {
    let pages = pages(http, key, "/employees/employees", &[("only_active", "true")]).await?;
    Ok(pages.iter().filter_map(employee_from).collect())
}

async fn leaves(
    http: &reqwest::Client,
    key: &str,
    today: NaiveDate,
    horizon: NaiveDate,
) -> Result<Vec<Leave>, AppError> {
    let from = today.format("%Y-%m-%d").to_string();
    let to = horizon.format("%Y-%m-%d").to_string();
    let filtered = pages(
        http,
        key,
        "/timeoff/leaves",
        &[("from", from.as_str()), ("to", to.as_str())],
    )
    .await;
    let pages = match filtered {
        Ok(pages) => pages,
        Err(_) => pages(http, key, "/timeoff/leaves", &[]).await?,
    };
    Ok(pages.iter().filter_map(leave_from).collect())
}

async fn pages(
    http: &reqwest::Client,
    key: &str,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Vec<Value>, AppError> {
    let mut collected = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..10 {
        let mut pairs: Vec<(&str, String)> = query
            .iter()
            .map(|(name, value)| (*name, (*value).to_string()))
            .collect();
        pairs.push(("limit", "100".into()));
        if let Some(cursor) = &cursor {
            pairs.push(("after_id", cursor.clone()));
        }
        let refs: Vec<(&str, &str)> = pairs.iter().map(|(name, value)| (*name, value.as_str())).collect();
        let page = get_json(http, key, path, &refs).await?;
        let next = next_cursor(&page);
        collected.extend(rows(&page).into_iter().cloned());
        match next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    Ok(collected)
}

async fn get_json(
    http: &reqwest::Client,
    key: &str,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Value, AppError> {
    let response = http
        .get(format!("{API}{path}"))
        .header("x-api-key", key)
        .header("accept", "application/json")
        .timeout(StdDuration::from_secs(20))
        .query(query)
        .send()
        .await
        .map_err(|_| AppError::message("No se pudo contactar con Factorial."))?;
    read_json(response).await
}

async fn read_json(response: reqwest::Response) -> Result<Value, AppError> {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(AppError::message("Factorial rechazó la clave."));
    }
    if !status.is_success() {
        return Err(AppError::message(format!("Factorial respondió {status}.")));
    }
    if body.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&body).map_err(|_| AppError::message("Factorial devolvió una respuesta ilegible."))
}

fn rows(value: &Value) -> Vec<&Value> {
    if let Some(rows) = value.as_array() {
        return rows.iter().collect();
    }
    value
        .get("data")
        .and_then(Value::as_array)
        .map(|rows| rows.iter().collect())
        .unwrap_or_default()
}

fn next_cursor(value: &Value) -> Option<String> {
    let meta = value.get("meta")?;
    if !meta.get("has_next_page").and_then(Value::as_bool).unwrap_or(false) {
        return None;
    }
    meta.get("end_cursor")
        .and_then(Value::as_str)
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_string)
}

fn employee_from(value: &Value) -> Option<Employee> {
    let id = json_id(value.get("id")?)?;
    let name = value
        .get("full_name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            let first = value.get("first_name").and_then(Value::as_str).unwrap_or("");
            let last = value.get("last_name").and_then(Value::as_str).unwrap_or("");
            let name = format!("{first} {last}").trim().to_string();
            if name.is_empty() { None } else { Some(name) }
        })
        .unwrap_or_else(|| "Compañero".into());
    Some(Employee {
        id,
        name,
        email: value.get("email").and_then(Value::as_str).unwrap_or("").trim().to_string(),
        login_email: value
            .get("login_email")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        birthday: date_field(value, "birthday_on"),
        terminated_on: date_field(value, "terminated_on"),
    })
}

fn leave_from(value: &Value) -> Option<Leave> {
    Some(Leave {
        id: json_id(value.get("id")?)?,
        employee_id: json_id(value.get("employee_id")?)?,
        start_on: date_field(value, "start_on")?,
        finish_on: date_field(value, "finish_on"),
        approved: value.get("approved").and_then(Value::as_bool),
        leave_type: value
            .get("leave_type_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        half_day: value
            .get("half_day")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
    })
}

fn date_field(value: &Value, name: &str) -> Option<NaiveDate> {
    let text = value.get(name)?.as_str()?;
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

fn json_id(value: &Value) -> Option<String> {
    match value {
        Value::String(id) if !id.is_empty() => Some(id.clone()),
        Value::Number(id) => Some(id.to_string()),
        _ => None,
    }
}

fn all_day(id: String, title: String, description: String, day: NaiveDate) -> CalendarEvent {
    let start = local_midnight(day);
    let end = local_midnight(day + Duration::days(1));
    CalendarEvent {
        id,
        calendar_id: "factorial".into(),
        calendar_color: COLOR.into(),
        title,
        start,
        end,
        all_day: true,
        location: String::new(),
        description,
        declined: false,
        conference_url: String::new(),
        event_type: "factorial".into(),
    }
}

fn local_midnight(day: NaiveDate) -> DateTime<Local> {
    let naive = day.and_hms_opt(0, 0, 0).unwrap_or_default();
    Local.from_local_datetime(&naive).earliest().unwrap_or_else(Local::now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn birthdays_and_absences_keep_the_owner_apart() {
        let employees = parse_employees(
            r#"{"data":[
                {"id":1,"full_name":"Ana","login_email":"ana@ritmo.test","email":"ana@empresa.test","birthday_on":"1990-11-05"},
                {"id":"2","full_name":"Luis","login_email":"luis@ritmo.test","birthday_on":"1988-11-06","terminated_on":"2026-11-01"}
            ]}"#,
        )
        .unwrap();
        let leaves = parse_leaves(
            r#"{"data":[
                {"id":9,"employee_id":1,"start_on":"2026-11-05","finish_on":"2026-11-05","approved":true,"leave_type_name":"Vacaciones"},
                {"id":10,"employee_id":2,"start_on":"2026-11-05","finish_on":"2026-11-06","approved":false,"leave_type_name":"Permiso"},
                {"id":11,"employee_id":2,"start_on":"2026-11-06","finish_on":"2026-11-06","approved":true,"leave_type_name":"Permiso","half_day":"beginning_of_day"}
            ]}"#,
        )
        .unwrap();
        let me = match_employee(&employees, Some("Ana@ritmo.test"));
        assert_eq!(me.as_deref(), Some("1"));
        let today = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        let events = agenda_events(&employees, &leaves, me.as_deref(), today, today + Duration::days(1));
        let titles: Vec<_> = events.iter().map(|event| event.title.as_str()).collect();
        assert!(titles.contains(&"Tu cumpleaños"));
        assert!(titles.contains(&"Ausente"));
        assert!(!titles.iter().any(|title| title.contains("Luis")));
        assert!(events.iter().all(|event| event.all_day && event.event_type == "factorial"));
        let pending = events.iter().find(|event| event.title == "Ausente").unwrap();
        assert_eq!(pending.description, "Vacaciones");
    }
}
