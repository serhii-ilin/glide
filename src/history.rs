//! Unified session history: document loads and `pushState`/`replaceState` entries.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SESSION_ENTRY_MAX: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryEntryKind {
    /// Full navigation that loaded (or will load) a new document.
    Document,
    /// `history.pushState` / `replaceState` without reloading.
    Session,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<Value>,
    pub kind: HistoryEntryKind,
    /// When true, back/forward to this entry loads the document again.
    #[serde(default)]
    pub loads_document: bool,
}

impl HistoryEntry {
    pub fn document(url: impl Into<String>) -> Self {
        Self::document_load(url, true)
    }

    pub fn document_load(url: impl Into<String>, loads_document: bool) -> Self {
        Self {
            url: url.into(),
            state: None,
            kind: HistoryEntryKind::Document,
            loads_document,
        }
    }

    pub fn session(url: impl Into<String>, state: Option<Value>) -> Self {
        Self {
            url: url.into(),
            state,
            kind: HistoryEntryKind::Session,
            loads_document: false,
        }
    }

    pub fn is_session(&self) -> bool {
        self.kind == HistoryEntryKind::Session
    }
}

/// Result of moving back or forward in the stack.
#[derive(Debug, Clone, PartialEq)]
pub enum HistoryStep {
    SameDocument {
        url: String,
        state: Option<Value>,
        hash_changed: bool,
    },
    Document {
        url: String,
    },
}

/// JSON returned to the JS worker for history API calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRpcSnapshot {
    pub index: usize,
    pub length: usize,
    pub state: Option<Value>,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub navigate: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hash_changed: bool,
}

fn is_false(v: &bool) -> bool {
    !*v
}

#[derive(Debug)]
pub enum HistoryError {
    DataClone(String),
}

impl HistoryError {
    pub fn message(&self) -> &str {
        match self {
            Self::DataClone(msg) => msg,
        }
    }
}

#[derive(Debug, Clone)]
pub struct History {
    entries: Vec<HistoryEntry>,
    index: usize,
}

impl History {
    pub fn new(url: String) -> Self {
        Self::from_parts(vec![HistoryEntry::document_load(url, false)], 0)
    }

    pub fn from_urls(entries: Vec<String>, index: usize) -> Self {
        let entries = entries.into_iter().map(HistoryEntry::document).collect();
        Self::from_parts(entries, index)
    }

    pub fn from_parts(entries: Vec<HistoryEntry>, index: usize) -> Self {
        let index = if entries.is_empty() {
            0
        } else {
            index.min(entries.len() - 1)
        };
        Self { entries, index }
    }

    pub fn current(&self) -> HistoryEntry {
        self.entries[self.index].clone()
    }

    pub fn current_url(&self) -> String {
        self.current().url
    }

    pub fn push_document(&mut self, url: String) {
        let entry = HistoryEntry::document_load(url, true);
        if self.entries[self.index].url == entry.url
            && self.entries[self.index].kind == HistoryEntryKind::Document
            && self.entries[self.index].loads_document
        {
            self.entries.truncate(self.index + 1);
            return;
        }
        self.entries.truncate(self.index + 1);
        self.entries.push(entry);
        self.index = self.entries.len() - 1;
        self.trim_to_max();
    }

    pub fn push_state(&mut self, url: String, state: Value) -> Result<(), HistoryError> {
        let state = clone_history_state(Some(state))?;
        self.entries.truncate(self.index + 1);
        self.entries.push(HistoryEntry::session(url, state));
        self.index = self.entries.len() - 1;
        self.trim_to_max();
        Ok(())
    }

    pub fn replace_state(&mut self, url: Option<String>, state: Value) -> Result<(), HistoryError> {
        let state = clone_history_state(Some(state))?;
        let entry = &mut self.entries[self.index];
        if let Some(url) = url {
            entry.url = url;
        }
        entry.state = state;
        entry.kind = HistoryEntryKind::Session;
        Ok(())
    }

    pub fn back(&mut self) -> Option<HistoryStep> {
        if self.index == 0 {
            return None;
        }
        let old = self.current_url();
        self.index -= 1;
        Some(self.step_after_traverse(&old))
    }

    pub fn forward(&mut self) -> Option<HistoryStep> {
        if self.index + 1 >= self.entries.len() {
            return None;
        }
        let old = self.current_url();
        self.index += 1;
        Some(self.step_after_traverse(&old))
    }

    pub fn go(&mut self, delta: i32) -> Option<HistoryStep> {
        if delta == 0 {
            return None;
        }
        let target = self.index as i32 + delta;
        if target < 0 || target as usize >= self.entries.len() {
            return None;
        }
        let old = self.current_url();
        self.index = target as usize;
        Some(self.step_after_traverse(&old))
    }

    fn step_after_traverse(&self, previous_url: &str) -> HistoryStep {
        let cur = self.current();
        let hash_changed = fragment_of(&cur.url) != fragment_of(previous_url)
            && origin_path_search_of(&cur.url) == origin_path_search_of(previous_url);
        if cur.is_session() || !cur.loads_document {
            HistoryStep::SameDocument {
                url: cur.url,
                state: cur.state,
                hash_changed,
            }
        } else {
            HistoryStep::Document { url: cur.url }
        }
    }

    pub fn replace_current_url(&mut self, url: String) {
        self.entries[self.index].url = url;
    }

    pub fn snapshot(&self) -> HistoryRpcSnapshot {
        let cur = self.current();
        HistoryRpcSnapshot {
            index: self.index,
            length: self.len(),
            state: cur.state.clone(),
            url: cur.url,
            navigate: None,
            hash_changed: false,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len().max(1)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn history_index(&self) -> usize {
        self.index
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn entry_urls(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.url.clone()).collect()
    }

    /// Rewind the stack after a cancelled navigation (index and trailing entries).
    pub fn restore_to_index(&mut self, index: usize) {
        self.index = index;
        self.entries.truncate(index + 1);
    }

    fn trim_to_max(&mut self) {
        while self.entries.len() > SESSION_ENTRY_MAX {
            self.entries.remove(0);
            self.index = self.index.saturating_sub(1);
        }
    }
}

pub fn clone_history_state(state: Option<Value>) -> Result<Option<Value>, HistoryError> {
    match state {
        None => Ok(None),
        Some(v) => {
            let serialized = serde_json::to_string(&v)
                .map_err(|e| HistoryError::DataClone(format!("state could not be cloned: {e}")))?;
            serde_json::from_str(&serialized)
                .map_err(|e| HistoryError::DataClone(format!("state could not be cloned: {e}")))
        }
    }
}

fn fragment_of(url: &str) -> Option<&str> {
    url.split('#').nth(1)
}

fn origin_path_search_of(url: &str) -> &str {
    url.split('#').next().unwrap_or(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_history_has_one_entry() {
        let mut h = History::new("https://example.com/".into());
        assert_eq!(h.current_url(), "https://example.com/");
        assert!(h.back().is_none());
        assert!(h.forward().is_none());
    }

    #[test]
    fn back_at_first_entry_returns_none() {
        let mut h = History::new("https://example.com/".into());
        assert!(h.back().is_none());
        assert_eq!(h.current_url(), "https://example.com/");
    }

    #[test]
    fn forward_at_last_entry_returns_none() {
        let mut h = History::new("https://example.com/".into());
        h.push_document("https://b.test/".into());
        assert_eq!(h.current_url(), "https://b.test/");
        assert!(h.forward().is_none());
    }

    #[test]
    fn push_after_back_truncates_forward_entries() {
        let mut h = History::new("https://a/".into());
        h.push_document("https://b/".into());
        h.push_document("https://c/".into());
        assert!(h.back().is_some());
        assert_eq!(h.current_url(), "https://b/");
        h.push_document("https://d/".into());
        assert_eq!(h.current_url(), "https://d/");
        assert!(h.forward().is_none());
    }

    #[test]
    fn push_same_document_url_does_not_duplicate() {
        let mut h = History::new("https://a/".into());
        h.push_document("https://b/".into());
        h.push_document("https://b/".into());
        assert_eq!(h.current_url(), "https://b/");
        assert!(h.forward().is_none());
        assert!(matches!(h.back(), Some(HistoryStep::SameDocument { .. })));
    }

    #[test]
    fn replace_current_keeps_index_and_forward_stack() {
        let mut h = History::new("https://a/".into());
        h.push_document("https://b/".into());
        h.push_document("https://c/".into());
        assert!(h.back().is_some());
        h.replace_current_url("https://b-final/".into());
        assert_eq!(h.current_url(), "https://b-final/");
        assert!(h.forward().is_some());
    }

    #[test]
    fn back_then_forward_round_trips() {
        let mut h = History::new("https://a/".into());
        h.push_document("https://b/".into());
        assert!(h.back().is_some());
        assert!(h.forward().is_some());
        assert_eq!(h.current_url(), "https://b/");
    }

    #[test]
    fn restore_to_index_drops_forward_entries() {
        let mut h = History::new("https://a/".into());
        h.push_document("https://b/".into());
        h.restore_to_index(0);
        assert_eq!(h.entry_urls(), vec!["https://a/"]);
        assert_eq!(h.history_index(), 0);
    }

    #[test]
    fn from_parts_restores_position() {
        let mut h = History::from_parts(
            vec![
                HistoryEntry::document("https://a/"),
                HistoryEntry::document("https://b/"),
            ],
            1,
        );
        assert_eq!(h.current_url(), "https://b/");
        assert_eq!(h.index(), 1);
        assert!(h.back().is_some());
    }

    #[test]
    fn push_state_creates_session_entry() {
        let mut h = History::new("https://example.test/".into());
        h.push_state(
            "https://example.test/one".into(),
            serde_json::json!({ "n": 1 }),
        )
        .expect("push");
        assert_eq!(h.len(), 2);
        assert!(h.current().is_session());
        assert_eq!(h.current().state, Some(serde_json::json!({ "n": 1 })));
    }

    #[test]
    fn session_back_is_same_document() {
        let mut h = History::new("https://example.test/".into());
        h.push_state(
            "https://example.test/p".into(),
            serde_json::json!({ "x": 1 }),
        )
        .expect("push");
        let step = h.back().expect("back");
        assert!(matches!(step, HistoryStep::SameDocument { .. }));
    }

    #[test]
    fn push_state_truncates_forward_stack() {
        let mut h = History::new("https://a/".into());
        h.push_state("https://a/1".into(), serde_json::json!(1))
            .unwrap();
        h.push_state("https://a/2".into(), serde_json::json!(2))
            .unwrap();
        h.back();
        h.push_state("https://a/3".into(), serde_json::json!(3))
            .unwrap();
        assert_eq!(h.len(), 3);
        assert!(h.forward().is_none());
    }

    #[test]
    fn session_stack_caps_at_max() {
        let mut h = History::new("https://a/".into());
        for i in 0..200 {
            h.push_state(format!("https://a/{i}"), serde_json::json!(i))
                .unwrap();
        }
        assert!(h.len() <= SESSION_ENTRY_MAX);
    }

    #[test]
    fn hash_only_change_sets_hash_changed() {
        let mut h = History::new("https://a/page".into());
        h.push_state("https://a/page#one".into(), serde_json::json!(1))
            .unwrap();
        let step = h.back().expect("back");
        assert!(matches!(
            step,
            HistoryStep::SameDocument {
                hash_changed: true,
                ..
            }
        ));
    }
}
