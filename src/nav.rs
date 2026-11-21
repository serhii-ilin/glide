//! Pending navigation vs. iframe URL observations (redirects, stop, link clicks).

use crate::history::History;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavKind {
    Go,
    Back,
    Forward,
    Reload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserveEffect {
    Unchanged,
    Completed,
    Redirect,
    LinkNavigation,
}

#[derive(Debug, Clone)]
struct PendingNav {
    index_before: usize,
}

/// Correlates commanded navigations with what the page iframe reports.
#[derive(Debug, Clone)]
pub struct NavTracker {
    history: History,
    expecting: Option<String>,
    last_seen: Option<String>,
    shown: String,
    pending: Option<PendingNav>,
}

impl NavTracker {
    pub fn new(start: String) -> Self {
        Self::from_history(History::new(start.clone()), start)
    }

    pub fn from_history(history: History, start: String) -> Self {
        Self {
            history,
            expecting: Some(start.clone()),
            last_seen: None,
            shown: start,
            pending: None,
        }
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn history_mut(&mut self) -> &mut History {
        &mut self.history
    }

    pub fn shown(&self) -> &str {
        &self.shown
    }

    pub fn expecting(&self) -> Option<&str> {
        self.expecting.as_deref()
    }

    pub fn navigate(&mut self, kind: NavKind, url: Option<String>) -> Option<String> {
        let index_before = self.history.history_index();
        let target = match kind {
            NavKind::Go => {
                let u = url?;
                self.history.push(u.clone());
                u
            }
            NavKind::Back => self.history.back()?,
            NavKind::Forward => self.history.forward()?,
            NavKind::Reload => self.history.current(),
        };
        self.pending = Some(PendingNav { index_before });
        self.expecting = Some(target.clone());
        self.shown = target.clone();
        Some(target)
    }

    pub fn observe(&mut self, actual: &str) -> ObserveEffect {
        if let Some(expected) = self.expecting.clone() {
            if actual == expected {
                self.expecting = None;
                self.pending = None;
                self.last_seen = Some(actual.to_string());
                return ObserveEffect::Completed;
            }
            if self.last_seen.as_deref() == Some(actual) {
                return ObserveEffect::Unchanged;
            }
            self.history.replace_current(actual.to_string());
            self.expecting = Some(actual.to_string());
            self.last_seen = Some(actual.to_string());
            if actual != self.shown {
                self.shown = actual.to_string();
            }
            return ObserveEffect::Redirect;
        }

        if self.last_seen.as_deref() != Some(actual) {
            self.history.push(actual.to_string());
            self.last_seen = Some(actual.to_string());
            if actual != self.shown {
                self.shown = actual.to_string();
            }
            return ObserveEffect::LinkNavigation;
        }
        ObserveEffect::Unchanged
    }

    /// Switch to a tab whose tracker was parked while another tab was active.
    /// The newly loaded iframe still reports the old URL until its replacement
    /// arrives, so that URL is remembered as already seen.
    pub fn switch_to(&mut self, url: &str, observed: Option<&str>) {
        self.expecting = Some(url.to_string());
        self.last_seen = observed.map(str::to_string);
        self.pending = None;
        self.shown = url.to_string();
    }

    pub fn stop(&mut self, displayed: &str) {
        self.expecting = None;
        if let Some(pending) = self.pending.take() {
            self.history.restore_to_index(pending.index_before);
        }
        self.shown = displayed.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(start: &str) -> NavTracker {
        NavTracker::new(start.to_string())
    }

    #[test]
    fn go_then_stop_restores_previous_entry() {
        let mut nav = track("A");
        nav.observe("A");
        nav.navigate(NavKind::Go, Some("B".into()));
        assert_eq!(nav.history().current(), "B");
        assert_eq!(nav.observe("A"), ObserveEffect::Unchanged);
        nav.stop("A");
        assert_eq!(nav.history().current(), "A");
        assert_eq!(nav.history().entries(), &["A"]);
        assert_eq!(nav.history().history_index(), 0);
        assert_eq!(nav.shown(), "A");
    }

    #[test]
    fn back_then_stop_keeps_forward_entry() {
        let mut nav = track("A");
        nav.observe("A");
        nav.navigate(NavKind::Go, Some("B".into()));
        nav.observe("B");
        nav.navigate(NavKind::Back, None);
        assert_eq!(nav.history().history_index(), 0);
        assert_eq!(nav.observe("B"), ObserveEffect::Unchanged);
        nav.stop("B");
        assert_eq!(nav.history().entries(), &["A", "B"]);
        assert_eq!(nav.history().history_index(), 1);
        assert_eq!(nav.history().current(), "B");
        assert_eq!(nav.shown(), "B");
    }

    #[test]
    fn go_then_redirect_replaces_entry() {
        let mut nav = track("A");
        nav.observe("A");
        nav.navigate(NavKind::Go, Some("B".into()));
        assert_eq!(nav.observe("A"), ObserveEffect::Unchanged);
        assert_eq!(nav.observe("B'"), ObserveEffect::Redirect);
        assert_eq!(nav.history().entries(), &["A", "B'"]);
        assert_eq!(nav.history().current(), "B'");
    }

    #[test]
    fn go_completes_normally() {
        let mut nav = track("A");
        nav.observe("A");
        nav.navigate(NavKind::Go, Some("B".into()));
        assert_eq!(nav.observe("A"), ObserveEffect::Unchanged);
        assert_eq!(nav.observe("B"), ObserveEffect::Completed);
        assert_eq!(nav.history().entries(), &["A", "B"]);
        assert!(nav.expecting().is_none());
    }

    #[test]
    fn link_click_pushes() {
        let mut nav = track("A");
        nav.observe("A");
        nav.navigate(NavKind::Go, Some("B".into()));
        nav.observe("B");
        assert_eq!(nav.observe("C"), ObserveEffect::LinkNavigation);
        assert_eq!(nav.history().entries(), &["A", "B", "C"]);
        assert_eq!(nav.history().current(), "C");
    }

    #[test]
    fn back_completes() {
        let mut nav = track("A");
        nav.observe("A");
        nav.navigate(NavKind::Go, Some("B".into()));
        nav.observe("B");
        nav.navigate(NavKind::Back, None);
        assert_eq!(nav.observe("B"), ObserveEffect::Unchanged);
        assert_eq!(nav.observe("A"), ObserveEffect::Completed);
        assert_eq!(nav.history().entries(), &["A", "B"]);
        assert_eq!(nav.history().history_index(), 0);
        assert_eq!(nav.history().current(), "A");
    }
}
