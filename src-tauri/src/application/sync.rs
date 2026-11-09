use std::collections::HashMap;

use chrono::{DateTime, Duration, Local, NaiveTime};

use crate::domain::{
    apply_page, calendar_visible, AppError, CalendarPort, Preferences, RemoteCalendar, SyncRequest,
};

pub async fn merge_today<C: CalendarPort>(
    calendar: &C,
    events: &mut Vec<crate::domain::CalendarEvent>,
    prefs: &Preferences,
    now: DateTime<Local>,
) -> Result<(), AppError> {
    let calendars = calendar.list_calendars().await?;
    let start = now
        .date_naive()
        .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap())
        .and_local_timezone(Local)
        .earliest()
        .unwrap_or(now);
    let end = start + Duration::days(1);
    for remote in calendars {
        if !calendar_visible(remote.google_selected, &remote.id, prefs) {
            continue;
        }
        let request = SyncRequest {
            calendar_id: remote.id.clone(),
            calendar_color: remote.color.clone(),
            sync_token: None,
            from: start,
            to: end,
        };
        let page = match calendar.sync_events(request).await {
            Ok(page) => page,
            Err(_) => continue,
        };
        apply_page(events, &remote.id, true, page.changes);
    }
    Ok(())
}

pub async fn pull_calendars<C: CalendarPort>(
    calendar: &C,
    events: &mut Vec<crate::domain::CalendarEvent>,
    tokens: &mut HashMap<String, String>,
    prefs: &Preferences,
    now: DateTime<Local>,
) -> Result<Vec<RemoteCalendar>, AppError> {
    let calendars = calendar.list_calendars().await?;
    let start = now
        .date_naive()
        .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap())
        .and_local_timezone(Local)
        .earliest()
        .unwrap_or(now)
        - Duration::days(1);
    let end = start + Duration::days(i64::from(prefs.range_days) + 2);
    for remote in &calendars {
        if !calendar_visible(remote.google_selected, &remote.id, prefs) {
            continue;
        }
        let mut request = SyncRequest {
            calendar_id: remote.id.clone(),
            calendar_color: remote.color.clone(),
            sync_token: tokens.get(&remote.id).cloned(),
            from: start,
            to: end,
        };
        let page = match calendar.sync_events(request.clone()).await {
            Err(AppError::SyncTokenExpired) => {
                tokens.remove(&remote.id);
                request.sync_token = None;
                calendar.sync_events(request).await?
            }
            other => other?,
        };
        let incremental = page.incremental;
        if let Some(token) = page.next_sync_token.clone() {
            tokens.insert(remote.id.clone(), token);
        }
        apply_page(events, &remote.id, incremental, page.changes);
    }
    Ok(calendars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::TimeZone;
    use std::sync::Mutex;

    use crate::domain::{CalendarEvent, EventChange, SyncPage};

    struct Fake {
        calls: Mutex<Vec<Option<String>>>,
    }

    #[async_trait]
    impl CalendarPort for Fake {
        async fn list_calendars(&self) -> Result<Vec<RemoteCalendar>, AppError> {
            Ok(vec![RemoteCalendar {
                id: "work".into(),
                title: "Trabajo".into(),
                color: "#0a84ff".into(),
                subtitle: "a@b.c".into(),
                google_selected: true,
            }])
        }

        async fn sync_events(&self, request: SyncRequest) -> Result<SyncPage, AppError> {
            self.calls.lock().unwrap().push(request.sync_token.clone());
            if request.sync_token.is_some() {
                return Err(AppError::SyncTokenExpired);
            }
            let start = Local.with_ymd_and_hms(2026, 10, 29, 15, 0, 0).unwrap();
            Ok(SyncPage {
                incremental: false,
                next_sync_token: Some("next".into()),
                changes: vec![EventChange {
                    deleted: false,
                    event: CalendarEvent {
                        id: "evt".into(),
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
                    },
                }],
            })
        }
    }

    #[tokio::test]
    async fn expired_sync_token_falls_back_to_a_full_window() {
        let fake = Fake {
            calls: Mutex::new(Vec::new()),
        };
        let mut events = Vec::new();
        let mut tokens = HashMap::from([("work".into(), "old".into())]);
        let now = Local.with_ymd_and_hms(2026, 10, 29, 13, 0, 0).unwrap();
        pull_calendars(&fake, &mut events, &mut tokens, &Preferences::default(), now)
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(tokens.get("work").map(String::as_str), Some("next"));
        let calls = fake.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].as_deref(), Some("old"));
        assert!(calls[1].is_none());
    }
}
