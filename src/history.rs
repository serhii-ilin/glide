//! In-memory back/forward stack for the page iframe URL.

#[derive(Debug, Clone)]
pub struct History {
    entries: Vec<String>,
    index: usize,
}

impl History {
    pub fn new(url: String) -> Self {
        Self::from_parts(vec![url], 0)
    }

    pub fn from_parts(entries: Vec<String>, index: usize) -> Self {
        let index = if entries.is_empty() {
            0
        } else {
            index.min(entries.len() - 1)
        };
        Self { entries, index }
    }

    pub fn current(&self) -> String {
        self.entries[self.index].clone()
    }

    pub fn push(&mut self, url: String) {
        if self.entries[self.index] == url {
            self.entries.truncate(self.index + 1);
            return;
        }
        self.entries.truncate(self.index + 1);
        self.entries.push(url);
        self.index = self.entries.len() - 1;
    }

    pub fn back(&mut self) -> Option<String> {
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        Some(self.current())
    }

    pub fn forward(&mut self) -> Option<String> {
        if self.index + 1 >= self.entries.len() {
            return None;
        }
        self.index += 1;
        Some(self.current())
    }

    pub fn replace_current(&mut self, url: String) {
        self.entries[self.index] = url;
    }

    pub fn history_index(&self) -> usize {
        self.index
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Rewind the stack after a cancelled navigation (index and trailing entries).
    pub fn restore_to_index(&mut self, index: usize) {
        self.index = index;
        self.entries.truncate(index + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::History;

    #[test]
    fn new_history_has_one_entry() {
        let mut h = History::new("https://example.com/".into());
        assert_eq!(h.current(), "https://example.com/");
        assert!(h.back().is_none());
        assert!(h.forward().is_none());
    }

    #[test]
    fn back_at_first_entry_returns_none() {
        let mut h = History::new("https://example.com/".into());
        assert!(h.back().is_none());
        assert_eq!(h.current(), "https://example.com/");
    }

    #[test]
    fn forward_at_last_entry_returns_none() {
        let mut h = History::new("https://example.com/".into());
        h.push("https://b.test/".into());
        assert_eq!(h.current(), "https://b.test/");
        assert!(h.forward().is_none());
    }

    #[test]
    fn push_after_back_truncates_forward_entries() {
        let mut h = History::new("https://a/".into());
        h.push("https://b/".into());
        h.push("https://c/".into());
        assert!(h.back().is_some());
        assert_eq!(h.current(), "https://b/");
        h.push("https://d/".into());
        assert_eq!(h.current(), "https://d/");
        assert!(h.forward().is_none());
        assert!(h.back().is_some());
        assert_eq!(h.current(), "https://b/");
    }

    #[test]
    fn push_same_url_does_not_duplicate() {
        let mut h = History::new("https://a/".into());
        h.push("https://b/".into());
        h.push("https://b/".into());
        assert_eq!(h.current(), "https://b/");
        assert!(h.forward().is_none());
        assert_eq!(h.back().as_deref(), Some("https://a/"));
        assert_eq!(h.forward().as_deref(), Some("https://b/"));
    }

    #[test]
    fn replace_current_keeps_index_and_forward_stack() {
        let mut h = History::new("https://a/".into());
        h.push("https://b/".into());
        h.push("https://c/".into());
        assert_eq!(h.back().as_deref(), Some("https://b/"));
        h.replace_current("https://b-final/".into());
        assert_eq!(h.current(), "https://b-final/");
        assert_eq!(h.forward().as_deref(), Some("https://c/"));
    }

    #[test]
    fn back_then_forward_round_trips() {
        let mut h = History::new("https://a/".into());
        h.push("https://b/".into());
        assert_eq!(h.back().as_deref(), Some("https://a/"));
        assert_eq!(h.forward().as_deref(), Some("https://b/"));
        assert_eq!(h.current(), "https://b/");
    }

    #[test]
    fn restore_to_index_drops_forward_entries() {
        let mut h = History::new("https://a/".into());
        h.push("https://b/".into());
        h.restore_to_index(0);
        assert_eq!(h.entries(), &["https://a/"]);
        assert_eq!(h.history_index(), 0);
    }

    #[test]
    fn from_parts_restores_position() {
        let mut h = History::from_parts(vec!["https://a/".into(), "https://b/".into()], 1);
        assert_eq!(h.current(), "https://b/");
        assert_eq!(h.index(), 1);
        assert!(h.back().is_some());
    }
}
