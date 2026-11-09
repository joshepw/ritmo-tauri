use std::collections::BTreeMap;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::domain::preferences::Preferences;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FocusInterval {
    pub started_at: DateTime<Local>,
    pub ended_at: DateTime<Local>,
    pub task_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PomodoroMode {
    Focus,
    Break,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PomodoroState {
    pub mode: PomodoroMode,
    pub running: bool,
    pub remaining_secs: u32,
    pub session: u8,
    pub focus_task_id: Option<String>,
    pub completed: BTreeMap<String, u32>,
    pub long_break: bool,
    /// Wall clock of the current focus, from the moment play is pressed.
    #[serde(default)]
    pub focus_started_at: Option<DateTime<Local>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhaseEnd {
    FocusEnded,
    BreakEnded,
}

impl PomodoroState {
    pub fn fresh(prefs: &Preferences) -> Self {
        let mut state = Self {
            mode: PomodoroMode::Focus,
            running: false,
            remaining_secs: 0,
            session: 1,
            focus_task_id: None,
            completed: BTreeMap::new(),
            long_break: false,
            focus_started_at: None,
        };
        state.remaining_secs = state.duration_secs(prefs);
        state
    }

    pub fn duration_secs(&self, prefs: &Preferences) -> u32 {
        let minutes = match self.mode {
            PomodoroMode::Focus => prefs.focus_minutes.clamp(5, 90),
            PomodoroMode::Break if self.long_break => prefs.long_break_minutes.clamp(5, 45),
            PomodoroMode::Break => prefs.short_break_minutes.clamp(1, 30),
        };
        minutes * 60
    }

    pub fn sync_duration(&mut self, prefs: &Preferences) {
        let duration = self.duration_secs(prefs);
        if !self.running || self.remaining_secs > duration {
            if !self.running {
                self.remaining_secs = duration;
            } else {
                self.remaining_secs = duration;
            }
        }
    }

    pub fn tick(&mut self, prefs: &Preferences) -> Option<PhaseEnd> {
        if !self.running {
            return None;
        }
        if self.remaining_secs > 1 {
            self.remaining_secs -= 1;
            return None;
        }
        self.remaining_secs = 0;
        Some(match self.mode {
            PomodoroMode::Focus => self.finish_focus(prefs),
            PomodoroMode::Break => self.finish_break(prefs),
        })
    }

    pub fn set_running(&mut self, running: bool) {
        self.running = running;
    }

    pub fn reset(&mut self, prefs: &Preferences) {
        self.running = false;
        self.focus_started_at = None;
        self.remaining_secs = self.duration_secs(prefs);
    }

    pub fn begin_focus(&mut self, now: DateTime<Local>) {
        if self.mode == PomodoroMode::Focus && self.focus_started_at.is_none() {
            self.focus_started_at = Some(now);
        }
    }

    pub fn close_focus(&mut self, now: DateTime<Local>) -> Option<FocusInterval> {
        let started_at = self.focus_started_at.take()?;
        if now <= started_at {
            return None;
        }
        Some(FocusInterval {
            started_at,
            ended_at: now,
            task_id: self.focus_task_id.clone(),
        })
    }

    pub fn set_mode(&mut self, mode: PomodoroMode, prefs: &Preferences) {
        if self.mode == mode {
            return;
        }
        if self.mode == PomodoroMode::Focus {
            self.focus_started_at = None;
        }
        self.mode = mode;
        if mode == PomodoroMode::Focus {
            self.long_break = false;
        }
        self.running = false;
        self.remaining_secs = self.duration_secs(prefs);
    }

    pub fn skip_break(&mut self, prefs: &Preferences) {
        if self.mode == PomodoroMode::Break {
            self.finish_break(prefs);
        }
    }

    pub fn set_focus_task(&mut self, task_id: Option<String>) {
        self.focus_task_id = task_id;
    }

    fn finish_focus(&mut self, prefs: &Preferences) -> PhaseEnd {
        if let Some(id) = &self.focus_task_id {
            *self.completed.entry(id.clone()).or_insert(0) += 1;
        }
        self.long_break = self.session >= 4;
        self.mode = PomodoroMode::Break;
        self.remaining_secs = self.duration_secs(prefs);
        self.running = prefs.auto_start_break;
        PhaseEnd::FocusEnded
    }

    fn finish_break(&mut self, prefs: &Preferences) -> PhaseEnd {
        self.mode = PomodoroMode::Focus;
        self.session = if self.long_break {
            1
        } else {
            (self.session % 4) + 1
        };
        self.long_break = false;
        self.remaining_secs = self.duration_secs(prefs);
        self.running = false;
        PhaseEnd::BreakEnded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefs() -> Preferences {
        let mut prefs = Preferences::default();
        prefs.focus_minutes = 25;
        prefs.short_break_minutes = 5;
        prefs.long_break_minutes = 15;
        prefs.auto_start_break = true;
        prefs
    }

    #[test]
    fn fourth_focus_starts_a_long_break() {
        let prefs = prefs();
        let mut state = PomodoroState::fresh(&prefs);
        state.session = 4;
        state.remaining_secs = 1;
        state.running = true;
        state.focus_task_id = Some("task-1".into());
        assert_eq!(state.tick(&prefs), Some(PhaseEnd::FocusEnded));
        assert!(state.long_break);
        assert_eq!(state.mode, PomodoroMode::Break);
        assert_eq!(state.remaining_secs, 15 * 60);
        assert_eq!(state.completed.get("task-1"), Some(&1));
        state.remaining_secs = 1;
        assert_eq!(state.tick(&prefs), Some(PhaseEnd::BreakEnded));
        assert_eq!(state.session, 1);
        assert_eq!(state.mode, PomodoroMode::Focus);
        assert!(!state.running);
    }

    #[test]
    fn short_break_advances_the_session() {
        let prefs = prefs();
        let mut state = PomodoroState::fresh(&prefs);
        state.remaining_secs = 1;
        state.running = true;
        state.tick(&prefs);
        assert_eq!(state.session, 1);
        assert_eq!(state.remaining_secs, 5 * 60);
        state.remaining_secs = 1;
        state.tick(&prefs);
        assert_eq!(state.session, 2);
    }

    #[test]
    fn pausing_closes_the_focus_and_the_next_play_starts_another() {
        use chrono::TimeZone;
        let prefs = prefs();
        let mut state = PomodoroState::fresh(&prefs);
        let start = Local.with_ymd_and_hms(2026, 11, 5, 10, 0, 0).unwrap();
        state.focus_task_id = Some("clickup:abc".into());
        state.set_running(true);
        state.begin_focus(start);
        let end = start + chrono::Duration::minutes(12);
        let closed = state.close_focus(end).unwrap();
        assert_eq!(closed.started_at, start);
        assert_eq!(closed.ended_at, end);
        assert_eq!(closed.task_id.as_deref(), Some("clickup:abc"));
        assert!(state.close_focus(end).is_none());
        state.begin_focus(end);
        assert_eq!(state.focus_started_at, Some(end));
    }
}
