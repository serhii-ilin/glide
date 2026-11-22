//! DevTools Console tab: per-tab ring buffer and chrome rendering.

use std::collections::VecDeque;

use blitz_dom::BaseDocument;

use crate::js::console_feed::ConsoleWireEvent;

pub const CONSOLE_CAPACITY: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleLevel {
    Log,
    Info,
    Warn,
    Error,
    Debug,
    Uncaught,
    UnhandledRejection,
}

impl ConsoleLevel {
    pub fn from_wire(level: &str) -> Self {
        match level {
            "error" => Self::Error,
            "warn" | "warning" => Self::Warn,
            "info" => Self::Info,
            "debug" | "trace" => Self::Debug,
            "uncaught" => Self::Uncaught,
            "rejection" => Self::UnhandledRejection,
            _ => Self::Log,
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            Self::Log => "level-log",
            Self::Info => "level-info",
            Self::Warn => "level-warn",
            Self::Error | Self::Uncaught | Self::UnhandledRejection => "level-error",
            Self::Debug => "level-debug",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Log => "LOG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Debug => "DEBUG",
            Self::Uncaught => "EXCEPTION",
            Self::UnhandledRejection => "REJECTION",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleEntry {
    pub level: ConsoleLevel,
    pub message: String,
    pub source: Option<String>,
    pub line: Option<usize>,
    pub timestamp_ms: u64,
}

impl ConsoleEntry {
    pub fn from_wire(wire: ConsoleWireEvent, timestamp_ms: u64) -> Self {
        Self {
            level: ConsoleLevel::from_wire(&wire.level),
            message: wire.message,
            source: wire.source,
            line: wire.line,
            timestamp_ms,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConsoleJournal {
    entries: VecDeque<ConsoleEntry>,
    generation: u64,
}

impl ConsoleJournal {
    pub fn push(&mut self, entry: ConsoleEntry) {
        if self.entries.len() >= CONSOLE_CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
        self.generation = self.generation.saturating_add(1);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.generation = self.generation.saturating_add(1);
    }

    pub fn entries(&self) -> impl Iterator<Item = &ConsoleEntry> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
}

pub fn format_timestamp(ms: u64) -> String {
    let total_secs = ms / 1000;
    let millis = ms % 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins:02}:{secs:02}.{millis:03}")
}

fn escape_console_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn render_devtools_console(doc: &mut BaseDocument, journal: &ConsoleJournal) {
    let Some(log_id) = doc.get_element_by_id("devtools-console-log") else {
        return;
    };
    let mut lines = String::new();
    for entry in journal.entries() {
        let level = entry.level.label();
        let class = entry.level.css_class();
        let ts = format_timestamp(entry.timestamp_ms);
        let source = entry
            .source
            .as_deref()
            .map(|s| {
                if let Some(line) = entry.line {
                    format!("{s}:{line}")
                } else {
                    s.to_string()
                }
            })
            .unwrap_or_default();
        let msg = escape_console_text(&entry.message);
        let source_html = if source.is_empty() {
            String::new()
        } else {
            format!(
                r#"<span class="console-source">{}</span>"#,
                escape_console_text(&source)
            )
        };
        lines.push_str(&format!(
            r#"<div class="devtools-console-line {class}"><span class="console-ts">{ts}</span><span class="console-level">{level}</span>{source_html}<span class="console-msg">{msg}</span></div>
"#
        ));
    }
    let mut m = doc.mutate();
    let children: Vec<_> = m.child_ids(log_id).into_iter().collect();
    for child in children {
        m.remove_and_drop_node(child);
    }
    if lines.is_empty() {
        let placeholder = m.create_text_node("No messages");
        m.append_children(log_id, &[placeholder]);
    } else {
        crate::parse::parse_inner_html_into_mutator(&mut m, log_id, &lines, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use blitz_dom::DocumentConfig;
    use blitz_traits::shell::{ColorScheme, Viewport};

    use crate::chrome::chrome_document;
    use crate::html::GlideHtmlProvider;
    use crate::js::console_feed::ConsoleWireEvent;
    use crate::parse;

    #[test]
    fn ring_buffer_drops_oldest_at_capacity() {
        let mut journal = ConsoleJournal::default();
        for i in 0..CONSOLE_CAPACITY + 3 {
            journal.push(ConsoleEntry {
                level: ConsoleLevel::Log,
                message: format!("m{i}"),
                source: None,
                line: None,
                timestamp_ms: i as u64,
            });
        }
        assert_eq!(journal.len(), CONSOLE_CAPACITY);
        let first = journal.entries().next().expect("entry");
        assert_eq!(first.message, "m3");
    }

    #[test]
    fn render_console_shows_level_message_and_source() {
        let mut journal = ConsoleJournal::default();
        journal.push(ConsoleEntry {
            level: ConsoleLevel::Warn,
            message: "watch out".into(),
            source: Some("inline".into()),
            line: None,
            timestamp_ms: 1234,
        });
        let html = chrome_document("about:blank", true, false);
        let config = DocumentConfig {
            viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(GlideHtmlProvider::default())),
            ..DocumentConfig::default()
        };
        let mut doc = parse::document_from_html(&html, config, false);
        doc.resolve(0.0);
        render_devtools_console(&mut doc, &journal);
        let log_id = doc.get_element_by_id("devtools-console-log").expect("log");
        let text = doc
            .get_node(log_id)
            .map(|n| n.text_content())
            .unwrap_or_default();
        assert!(text.contains("WARN"));
        assert!(text.contains("watch out"));
        assert!(text.contains("inline"));
    }

    #[test]
    fn from_wire_maps_rejection_level() {
        let entry = ConsoleEntry::from_wire(
            ConsoleWireEvent {
                level: "rejection".into(),
                message: "nope".into(),
                source: None,
                line: None,
            },
            0,
        );
        assert_eq!(entry.level, ConsoleLevel::UnhandledRejection);
    }
}
