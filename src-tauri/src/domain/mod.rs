pub mod briefing;
pub mod error;
pub mod meeting;
pub mod pomodoro;
pub mod ports;
pub mod preferences;
pub mod records;
pub mod timefmt;

pub use briefing::{assistant_prompt, local_summary, should_prepare_summary, suggestions, DayContext, Suggestion};
pub use error::AppError;
pub use meeting::{open_message, open_target, MeetingKind, MeetingLink};
pub use pomodoro::{FocusInterval, PhaseEnd, PomodoroMode, PomodoroState};
pub use ports::{
    AssistantPort, CalendarPort, Clock, RemoteCalendar, SecretPort, SyncPage, SyncRequest, TaskPort,
};
pub use preferences::{
    apply_pref_patch, calendar_visible, migrate_chat_ui, scopes_for, set_calendar_visible, PrefEffect,
    Preferences,
};
pub use records::{
    apply_page, build_agenda_with, create_note, event_link, note_join, pending_join_alerts, sort_tasks,
    task_done_today, task_due_label, Account, CalendarEvent, EventChange, JoinAlert, Note, TaskItem, TodayRow,
    WorkLog,
};
