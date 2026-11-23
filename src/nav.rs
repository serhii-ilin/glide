//! Pending navigation vs. iframe URL observations (redirects, stop, link clicks).

use std::sync::{Arc, Mutex};

use crate::history::{History, HistoryStep};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraverseOutcome {
    SameDocument,
    Document,
    None,
}

#[derive(Debug, Clone)]
struct PendingNav {
    index_before: usize,
}

/// Correlates commanded navigations with what the page iframe reports.
#[derive(Debug, Clone)]
pub struct NavTracker {
    history: Arc<Mutex<History>>,
    expecting: Option<String>,
    last_seen: Option<String>,
    shown: String,
    pending: Option<PendingNav>,
    last_traverse: Option<HistoryStep>,
}

impl NavTracker {
    pub fn new(start: String) -> Self {
        Self::from_shared(Arc::new(Mutex::new(History::new(start.clone()))), start)
    }

    pub fn from_history(history: History, start: String) -> Self {
        Self::from_shared(Arc::new(Mutex::new(history)), start)
    }

    pub fn from_shared(history: Arc<Mutex<History>>, start: String) -> Self {
        Self {
            history,
            expecting: Some(start.clone()),
            last_seen: None,
            shown: start,
            pending: None,
            last_traverse: None,
        }
    }

    pub fn history_arc(&self) -> Arc<Mutex<History>> {
        Arc::clone(&self.history)
    }

    fn lock_history(&self) -> std::sync::MutexGuard<'_, History> {
        self.history.lock().expect("history lock")
    }

    pub fn shown(&self) -> &str {
        &self.shown
    }

    pub fn expecting(&self) -> Option<&str> {
        self.expecting.as_deref()
    }

    pub fn take_last_traverse(&mut self) -> Option<HistoryStep> {
        self.last_traverse.take()
    }

    pub fn navigate(&mut self, kind: NavKind, url: Option<String>) -> Option<String> {
        let (index_before, target, traverse) = {
            let mut history = self.lock_history();
            let index_before = history.history_index();
            match kind {
                NavKind::Go => {
                    let u = url?;
                    history.push_document(u.clone());
                    (index_before, u, None)
                }
                NavKind::Back => {
                    let step = history.back()?;
                    let url = match &step {
                        HistoryStep::Document { url } => url.clone(),
                        HistoryStep::SameDocument { url, .. } => url.clone(),
                    };
                    (index_before, url, Some(step))
                }
                NavKind::Forward => {
                    let step = history.forward()?;
                    let url = match &step {
                        HistoryStep::Document { url } => url.clone(),
                        HistoryStep::SameDocument { url, .. } => url.clone(),
                    };
                    (index_before, url, Some(step))
                }
                NavKind::Reload => (index_before, history.current_url(), None),
            }
        };
        self.last_traverse = traverse;
        self.pending = Some(PendingNav { index_before });
        self.expecting = Some(target.clone());
        self.shown = target.clone();
        Some(target)
    }

    pub fn traverse_outcome(&self) -> TraverseOutcome {
        match &self.last_traverse {
            Some(HistoryStep::SameDocument { .. }) => TraverseOutcome::SameDocument,
            Some(HistoryStep::Document { .. }) => TraverseOutcome::Document,
            None => TraverseOutcome::None,
        }
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
            self.lock_history().replace_current_url(actual.to_string());
            self.expecting = Some(actual.to_string());
            self.last_seen = Some(actual.to_string());
            if actual != self.shown {
                self.shown = actual.to_string();
            }
            return ObserveEffect::Redirect;
        }

        if self.last_seen.as_deref() != Some(actual) {
            self.lock_history().push_document(actual.to_string());
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
        self.last_traverse = None;
        self.shown = url.to_string();
    }

    pub fn stop(&mut self, displayed: &str) {
        self.expecting = None;
        if let Some(pending) = self.pending.take() {
            self.lock_history().restore_to_index(pending.index_before);
        }
        self.last_traverse = None;
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
        assert_eq!(nav.lock_history().current_url(), "B");
        assert_eq!(nav.observe("A"), ObserveEffect::Unchanged);
        nav.stop("A");
        assert_eq!(nav.lock_history().current_url(), "A");
        assert_eq!(nav.lock_history().entry_urls(), &["A"]);
        assert_eq!(nav.lock_history().history_index(), 0);
        assert_eq!(nav.shown(), "A");
    }

    #[test]
    fn back_then_stop_keeps_forward_entry() {
        let mut nav = track("A");
        nav.observe("A");
        nav.navigate(NavKind::Go, Some("B".into()));
        nav.observe("B");
        nav.navigate(NavKind::Back, None);
        assert_eq!(nav.lock_history().history_index(), 0);
        assert_eq!(nav.observe("B"), ObserveEffect::Unchanged);
        nav.stop("B");
        assert_eq!(nav.lock_history().entry_urls(), &["A", "B"]);
        assert_eq!(nav.lock_history().history_index(), 1);
        assert_eq!(nav.lock_history().current_url(), "B");
        assert_eq!(nav.shown(), "B");
    }

    #[test]
    fn go_then_redirect_replaces_entry() {
        let mut nav = track("A");
        nav.observe("A");
        nav.navigate(NavKind::Go, Some("B".into()));
        assert_eq!(nav.observe("A"), ObserveEffect::Unchanged);
        assert_eq!(nav.observe("B'"), ObserveEffect::Redirect);
        assert_eq!(nav.lock_history().entry_urls(), &["A", "B'"]);
        assert_eq!(nav.lock_history().current_url(), "B'");
    }

    #[test]
    fn go_completes_normally() {
        let mut nav = track("A");
        nav.observe("A");
        nav.navigate(NavKind::Go, Some("B".into()));
        assert_eq!(nav.observe("A"), ObserveEffect::Unchanged);
        assert_eq!(nav.observe("B"), ObserveEffect::Completed);
        assert_eq!(nav.lock_history().entry_urls(), &["A", "B"]);
        assert!(nav.expecting().is_none());
    }

    #[test]
    fn link_click_pushes() {
        let mut nav = track("A");
        nav.observe("A");
        nav.navigate(NavKind::Go, Some("B".into()));
        nav.observe("B");
        assert_eq!(nav.observe("C"), ObserveEffect::LinkNavigation);
        assert_eq!(nav.lock_history().entry_urls(), &["A", "B", "C"]);
        assert_eq!(nav.lock_history().current_url(), "C");
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
        assert_eq!(nav.lock_history().entry_urls(), &["A", "B"]);
        assert_eq!(nav.lock_history().history_index(), 0);
        assert_eq!(nav.lock_history().current_url(), "A");
    }

    #[test]
    fn chrome_back_after_push_state_is_same_document_traversal() {
        let mut nav = track("https://ex/");
        nav.observe("https://ex/");
        nav.lock_history()
            .push_state("https://ex/a".into(), serde_json::json!({ "n": 1 }))
            .expect("push");
        nav.shown = "https://ex/a".into();
        nav.navigate(NavKind::Back, None);
        assert_eq!(nav.traverse_outcome(), TraverseOutcome::SameDocument);
    }
}
