//! In-memory back/forward stack for the page iframe URL.

#[derive(Debug, Clone)]
pub struct History {
    entries: Vec<String>,
    index: usize,
}

impl History {
    pub fn new(url: String) -> Self {
        Self {
            entries: vec![url],
            index: 0,
        }
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
}

#[cfg(test)]
mod tests {
    use super::History;

    #[test]
    fn new_history_has_one_entry() {
        let h = History::new("https://example.com/".into());
        assert_eq!(h.current(), "https://example.com/");
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
}
