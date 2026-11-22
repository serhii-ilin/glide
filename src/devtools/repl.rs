//! DevTools Console REPL input history and submit helpers.

use blitz_dom::BaseDocument;

use crate::tabs::{Tab, TabManager};

pub const REPL_HISTORY_CAP: usize = 100;

/// Per-tab REPL transcript history (survives Clear).
#[derive(Debug, Clone, Default)]
pub struct ConsoleReplHistory {
    entries: std::collections::VecDeque<String>,
}

impl ConsoleReplHistory {
    pub fn push(&mut self, entry: impl Into<String>) {
        let line = entry.into();
        if line.is_empty() {
            return;
        }
        if self.entries.back().is_some_and(|last| last == &line) {
            return;
        }
        if self.entries.len() >= REPL_HISTORY_CAP {
            self.entries.pop_front();
        }
        self.entries.push_back(line);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn newest_index(&self) -> Option<usize> {
        if self.entries.is_empty() {
            None
        } else {
            Some(self.entries.len() - 1)
        }
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(String::as_str)
    }
}

/// Cursor for Up/Down in the input; independent of tab history storage.
#[derive(Debug, Clone, Default)]
pub struct ConsoleReplUi {
    history_index: Option<usize>,
    draft: String,
}

impl ConsoleReplUi {
    pub fn reset_after_submit(&mut self) {
        self.history_index = None;
        self.draft.clear();
    }

    #[cfg(test)]
    pub fn test_set_cursor(&mut self, index: Option<usize>, draft: impl Into<String>) {
        self.history_index = index;
        self.draft = draft.into();
    }

    #[cfg(test)]
    pub fn test_cursor_cleared(&self) -> bool {
        self.history_index.is_none() && self.draft.is_empty()
    }

    pub fn navigate(
        &mut self,
        history: &ConsoleReplHistory,
        current_input: &str,
        older: bool,
    ) -> Option<String> {
        if history.is_empty() {
            return None;
        }
        if self.history_index.is_none() && !current_input.is_empty() {
            self.draft = current_input.to_string();
        }
        let index = match self.history_index {
            None if older => history.newest_index()?,
            None => return None,
            Some(i) if older => i.checked_sub(1)?,
            Some(i) => {
                let next = i.saturating_add(1);
                if next >= history.len() {
                    self.history_index = None;
                    return Some(if self.draft.is_empty() {
                        String::new()
                    } else {
                        std::mem::take(&mut self.draft)
                    });
                }
                next
            }
        };
        self.history_index = Some(index);
        history.get(index).map(str::to_string)
    }
}

pub fn append_repl_history(tab: &mut Tab, source: &str) {
    tab.console_repl_history.push(source);
}

pub fn record_repl_input_row(tab: &mut Tab, source: &str, timestamp_ms: u64) {
    tab.console.push(crate::devtools::console::ConsoleEntry {
        level: crate::devtools::console::ConsoleLevel::ReplInput,
        message: format!("> {source}"),
        source: None,
        line: None,
        timestamp_ms,
        promise_id: None,
    });
}

pub fn record_repl_error_row(tab: &mut Tab, message: impl Into<String>, timestamp_ms: u64) {
    tab.console.push(crate::devtools::console::ConsoleEntry {
        level: crate::devtools::console::ConsoleLevel::Error,
        message: message.into(),
        source: None,
        line: None,
        timestamp_ms,
        promise_id: None,
    });
}

pub fn submit_repl_expression(
    tabs: &mut TabManager,
    js: &mut crate::js::JsHost,
    source: String,
    timestamp_ms: u64,
) -> bool {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return false;
    }
    let Some(tab) = tabs.active_mut() else {
        return false;
    };
    append_repl_history(tab, trimmed);
    record_repl_input_row(tab, trimmed, timestamp_ms);
    match js.submit_console_eval(trimmed) {
        Ok(()) => true,
        Err(reason) => {
            record_repl_error_row(tab, reason, timestamp_ms);
            true
        }
    }
}

pub fn apply_history_to_input(
    doc: &mut BaseDocument,
    history: &ConsoleReplHistory,
    ui: &mut ConsoleReplUi,
    older: bool,
) -> bool {
    let current = crate::chrome::devtools_console_input_value(doc).unwrap_or_default();
    let Some(next) = ui.navigate(history, &current, older) else {
        return false;
    };
    crate::chrome::set_devtools_console_input_value(doc, &next);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_caps_at_capacity_dropping_oldest() {
        let mut hist = ConsoleReplHistory::default();
        for i in 0..REPL_HISTORY_CAP + 5 {
            hist.push(format!("line-{i}"));
        }
        assert_eq!(hist.len(), REPL_HISTORY_CAP);
        assert_eq!(hist.get(0), Some("line-5"));
    }

    #[test]
    fn history_skips_consecutive_duplicates() {
        let mut hist = ConsoleReplHistory::default();
        hist.push("a");
        hist.push("a");
        hist.push("b");
        assert_eq!(hist.len(), 2);
    }

    #[test]
    fn history_up_down_returns_entries() {
        let mut hist = ConsoleReplHistory::default();
        hist.push("first");
        hist.push("second");
        let mut ui = ConsoleReplUi::default();
        let up1 = ui.navigate(&hist, "", true).expect("up");
        assert_eq!(up1, "second");
        let up2 = ui.navigate(&hist, &up1, true).expect("up2");
        assert_eq!(up2, "first");
        let down = ui.navigate(&hist, &up2, false).expect("down");
        assert_eq!(down, "second");
    }
}
