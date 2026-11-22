//! DevTools Console tab: per-tab ring buffer and chrome rendering.

use std::collections::VecDeque;

use blitz_dom::BaseDocument;

use crate::js::console_feed::ConsoleWireEvent;
use crate::tabs::TabManager;

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
    pub promise_id: Option<u64>,
}

impl ConsoleEntry {
    pub fn from_wire(wire: ConsoleWireEvent) -> Self {
        Self {
            level: ConsoleLevel::from_wire(&wire.level),
            message: wire.message,
            source: wire.source,
            line: wire.line,
            timestamp_ms: wire.timestamp_ms,
            promise_id: wire.promise_id,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConsoleJournal {
    entries: VecDeque<ConsoleEntry>,
    generation: u64,
    total_pushed: u64,
}

impl ConsoleJournal {
    pub fn push(&mut self, entry: ConsoleEntry) {
        if self.entries.len() >= CONSOLE_CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
        self.total_pushed = self.total_pushed.saturating_add(1);
        self.generation = self.generation.saturating_add(1);
    }

    pub fn revoke_rejection(&mut self, promise_id: u64) -> bool {
        let Some(index) = self
            .entries
            .iter()
            .position(|e| e.promise_id == Some(promise_id))
        else {
            return false;
        };
        self.entries.remove(index);
        self.generation = self.generation.saturating_add(1);
        true
    }

    pub fn total_pushed(&self) -> u64 {
        self.total_pushed
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

/// Route wire events to the tab that produced them; drop stale page epochs.
pub fn apply_console_wire(tabs: &mut TabManager, events: &[ConsoleWireEvent]) -> bool {
    let mut changed = false;
    for ev in events {
        if ev.level == "rejection-revoke" {
            let Some(promise_id) = ev.promise_id else {
                continue;
            };
            let Some(tab) = tabs.tab_mut(ev.tab_id) else {
                continue;
            };
            if tab.console_epoch != ev.page_epoch {
                continue;
            }
            if tab.console.revoke_rejection(promise_id) {
                changed = true;
            }
            continue;
        }
        let Some(tab) = tabs.tab_mut(ev.tab_id) else {
            continue;
        };
        if tab.console_epoch != ev.page_epoch {
            continue;
        }
        tab.console.push(ConsoleEntry::from_wire(ev.clone()));
        changed = true;
    }
    changed
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

pub(crate) fn observe_navigation_console_effect(
    effect: crate::nav::ObserveEffect,
    tab: &mut crate::tabs::Tab,
    js: &mut crate::js::JsHost,
    reset: &mut impl FnMut(&mut crate::tabs::Tab, &mut crate::js::JsHost),
) {
    if matches!(
        effect,
        crate::nav::ObserveEffect::LinkNavigation | crate::nav::ObserveEffect::Redirect
    ) {
        reset(tab, js);
    }
}

pub(crate) fn bump_tab_console_epoch_for_switch(
    tab: &mut crate::tabs::Tab,
    js: &mut crate::js::JsHost,
) {
    tab.console_epoch = tab.console_epoch.saturating_add(1);
    crate::js::console_feed::discard_pending_for_tab(tab.id);
    js.set_console_route(tab.id, tab.console_epoch);
}

fn full_render_log(
    doc: &mut BaseDocument,
    log_id: blitz_traits::node_id::NodeId,
    entries: &[&ConsoleEntry],
) {
    let mut m = doc.mutate();
    let children: Vec<_> = m.child_ids(log_id).into_iter().collect();
    for child in children {
        m.remove_and_drop_node(child);
    }
    if entries.is_empty() {
        let placeholder = m.create_text_node("No messages");
        m.append_children(log_id, &[placeholder]);
        return;
    }
    let mut lines = String::new();
    for entry in entries {
        lines.push_str(&entry_line_html(entry));
    }
    crate::parse::parse_inner_html_into_mutator(&mut m, log_id, &lines, false);
}

fn append_render_log(
    doc: &mut BaseDocument,
    log_id: blitz_traits::node_id::NodeId,
    entries: &[&ConsoleEntry],
) {
    let mut m = doc.mutate();
    let mut lines = String::new();
    for entry in entries {
        lines.push_str(&entry_line_html(entry));
    }
    crate::parse::parse_inner_html_into_mutator(&mut m, log_id, &lines, false);
}

fn pure_append(
    journal: &ConsoleJournal,
    entries_len: usize,
    last_len: usize,
    last_total_pushed: u64,
) -> bool {
    if last_len == 0 || entries_len <= last_len {
        return false;
    }
    let added = entries_len - last_len;
    journal.total_pushed().saturating_sub(last_total_pushed) == added as u64
}

fn entry_line_html(entry: &ConsoleEntry) -> String {
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
    format!(
        r#"<div class="devtools-console-line {class}"><span class="console-ts">{ts}</span><span class="console-level">{level}</span>{source_html}<span class="console-msg">{msg}</span></div>
"#
    )
}

pub fn render_devtools_console(
    doc: &mut BaseDocument,
    journal: &ConsoleJournal,
    last_generation: &mut u64,
    last_rendered_len: &mut usize,
    last_rendered_total: &mut u64,
    force_full: bool,
) -> bool {
    let generation = journal.generation();
    if generation == *last_generation && !force_full {
        return false;
    }
    let Some(log_id) = doc.get_element_by_id("devtools-console-log") else {
        return false;
    };
    let entries: Vec<_> = journal.entries().cloned().collect();
    let entries_refs: Vec<_> = entries.iter().collect();

    if force_full {
        full_render_log(doc, log_id, &entries_refs);
        *last_generation = generation;
        *last_rendered_len = entries.len();
        *last_rendered_total = journal.total_pushed();
        return true;
    }

    if entries.is_empty() {
        full_render_log(doc, log_id, &[]);
        *last_generation = generation;
        *last_rendered_len = 0;
        *last_rendered_total = journal.total_pushed();
        return true;
    }

    let append_only = pure_append(
        journal,
        entries.len(),
        *last_rendered_len,
        *last_rendered_total,
    );
    if append_only {
        append_render_log(doc, log_id, &entries_refs[*last_rendered_len..]);
    } else {
        full_render_log(doc, log_id, &entries_refs);
    }
    *last_generation = generation;
    *last_rendered_len = entries.len();
    *last_rendered_total = journal.total_pushed();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use blitz_dom::DocumentConfig;
    use blitz_traits::shell::{ColorScheme, Viewport};

    use crate::chrome::chrome_document;
    use crate::html::GlideHtmlProvider;
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
                promise_id: None,
            });
        }
        assert_eq!(journal.len(), CONSOLE_CAPACITY);
        let first = journal.entries().next().expect("entry");
        assert_eq!(first.message, "m3");
    }

    #[test]
    fn wire_routes_to_tab_not_active() {
        let mut tabs = TabManager::new();
        let a = tabs.add("https://a.test/");
        let _b = tabs.add("https://b.test/");
        assert!(tabs.select(a));
        let ev = ConsoleWireEvent {
            tab_id: _b,
            page_epoch: 1,
            timestamp_ms: 1,
            level: "log".into(),
            message: "on-b".into(),
            source: None,
            line: None,
            promise_id: None,
        };
        apply_console_wire(&mut tabs, &[ev]);
        let tab_b = tabs.tab_mut(_b).expect("tab b");
        assert_eq!(tab_b.console.len(), 1);
        let tab_a = tabs.active().expect("active a");
        assert!(tab_a.console.is_empty());
    }

    #[test]
    fn stale_epoch_events_are_dropped() {
        let mut tabs = TabManager::new();
        let id = tabs.add("https://a.test/");
        if let Some(tab) = tabs.tab_mut(id) {
            tab.console_epoch = 2;
        }
        let ev = ConsoleWireEvent {
            tab_id: id,
            page_epoch: 1,
            timestamp_ms: 0,
            level: "log".into(),
            message: "old".into(),
            source: None,
            line: None,
            promise_id: None,
        };
        apply_console_wire(&mut tabs, &[ev]);
        let tab = tabs.tab_mut(id).expect("tab");
        assert!(tab.console.is_empty());
    }

    #[test]
    fn rejection_revoke_removes_matching_row() {
        let mut journal = ConsoleJournal::default();
        journal.push(ConsoleEntry {
            level: ConsoleLevel::UnhandledRejection,
            message: "nope".into(),
            source: None,
            line: None,
            timestamp_ms: 0,
            promise_id: Some(42),
        });
        assert!(journal.revoke_rejection(42));
        assert!(journal.is_empty());
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
            promise_id: None,
        });
        let html = chrome_document("about:blank", true, false);
        let config = DocumentConfig {
            viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(GlideHtmlProvider::default())),
            ..DocumentConfig::default()
        };
        let mut doc = parse::document_from_html(&html, config, false);
        doc.resolve(0.0);
        let mut last_gen = 0;
        let mut last_len = 0;
        let mut last_total = 0;
        render_devtools_console(
            &mut doc,
            &journal,
            &mut last_gen,
            &mut last_len,
            &mut last_total,
            true,
        );
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
    fn render_escapes_angle_brackets_as_text_only() {
        let mut journal = ConsoleJournal::default();
        journal.push(ConsoleEntry {
            level: ConsoleLevel::Log,
            message: r#"<img src=x onerror=alert(1)>"#.into(),
            source: None,
            line: None,
            timestamp_ms: 0,
            promise_id: None,
        });
        let html = chrome_document("about:blank", true, false);
        let config = DocumentConfig {
            viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(GlideHtmlProvider::default())),
            ..DocumentConfig::default()
        };
        let mut doc = parse::document_from_html(&html, config, false);
        doc.resolve(0.0);
        let mut last_gen = 0;
        let mut last_len = 0;
        let mut last_total = 0;
        render_devtools_console(
            &mut doc,
            &journal,
            &mut last_gen,
            &mut last_len,
            &mut last_total,
            true,
        );
        let log_id = doc.get_element_by_id("devtools-console-log").expect("log");
        let text = doc
            .get_node(log_id)
            .map(|n| n.text_content())
            .unwrap_or_default();
        assert!(
            text.contains("<img src=x onerror=alert(1)>"),
            "expected literal angle brackets in output, got: {text}"
        );
    }

    #[test]
    fn from_wire_maps_rejection_level() {
        let entry = ConsoleEntry::from_wire(ConsoleWireEvent {
            tab_id: 1,
            page_epoch: 1,
            timestamp_ms: 0,
            level: "rejection".into(),
            message: "nope".into(),
            source: None,
            line: None,
            promise_id: Some(9),
        });
        assert_eq!(entry.level, ConsoleLevel::UnhandledRejection);
    }

    fn sample_doc() -> BaseDocument {
        let html = chrome_document("about:blank", true, false);
        let config = DocumentConfig {
            viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(GlideHtmlProvider::default())),
            ..DocumentConfig::default()
        };
        let mut doc = parse::document_from_html(&html, config, false);
        doc.resolve(0.0);
        doc
    }

    fn log_text(doc: &BaseDocument) -> String {
        let log_id = doc.get_element_by_id("devtools-console-log").expect("log");
        doc.get_node(log_id)
            .map(|n| n.text_content())
            .unwrap_or_default()
    }

    #[test]
    fn render_after_501_pushes_shows_ring_tail() {
        let mut journal = ConsoleJournal::default();
        let mut doc = sample_doc();
        let mut last_gen = 0;
        let mut last_len = 0;
        let mut last_total = 0;
        for i in 0..=CONSOLE_CAPACITY {
            journal.push(ConsoleEntry {
                level: ConsoleLevel::Log,
                message: format!("m{i}"),
                source: None,
                line: None,
                timestamp_ms: i as u64,
                promise_id: None,
            });
            render_devtools_console(
                &mut doc,
                &journal,
                &mut last_gen,
                &mut last_len,
                &mut last_total,
                false,
            );
        }
        let text = log_text(&doc);
        assert!(text.contains("m1"), "{text}");
        assert!(text.contains(&format!("m{CONSOLE_CAPACITY}")), "{text}");
        assert!(!text.contains("m0"), "{text}");
        assert_eq!(journal.len(), CONSOLE_CAPACITY);
    }

    #[test]
    fn clear_then_one_line_drops_no_messages_placeholder() {
        let mut journal = ConsoleJournal::default();
        journal.push(ConsoleEntry {
            level: ConsoleLevel::Log,
            message: "before".into(),
            source: None,
            line: None,
            timestamp_ms: 1,
            promise_id: None,
        });
        let mut doc = sample_doc();
        let mut last_gen = 0;
        let mut last_len = 0;
        let mut last_total = 0;
        render_devtools_console(
            &mut doc,
            &journal,
            &mut last_gen,
            &mut last_len,
            &mut last_total,
            true,
        );
        journal.clear();
        render_devtools_console(
            &mut doc,
            &journal,
            &mut last_gen,
            &mut last_len,
            &mut last_total,
            false,
        );
        journal.push(ConsoleEntry {
            level: ConsoleLevel::Log,
            message: "after".into(),
            source: None,
            line: None,
            timestamp_ms: 2,
            promise_id: None,
        });
        render_devtools_console(
            &mut doc,
            &journal,
            &mut last_gen,
            &mut last_len,
            &mut last_total,
            false,
        );
        let text = log_text(&doc);
        assert!(!text.contains("No messages"), "{text}");
        assert!(text.contains("after"), "{text}");
        assert!(!text.contains("before"), "{text}");
    }

    #[test]
    fn tab_switch_full_render_shows_only_new_journal() {
        let mut journal_a = ConsoleJournal::default();
        journal_a.push(ConsoleEntry {
            level: ConsoleLevel::Log,
            message: "tab-a".into(),
            source: None,
            line: None,
            timestamp_ms: 0,
            promise_id: None,
        });
        let mut journal_b = ConsoleJournal::default();
        journal_b.push(ConsoleEntry {
            level: ConsoleLevel::Log,
            message: "tab-b".into(),
            source: None,
            line: None,
            timestamp_ms: 0,
            promise_id: None,
        });
        let mut doc = sample_doc();
        let mut last_gen = u64::MAX;
        let mut last_len = 0;
        let mut last_total = 0;
        render_devtools_console(
            &mut doc,
            &journal_a,
            &mut last_gen,
            &mut last_len,
            &mut last_total,
            true,
        );
        assert!(log_text(&doc).contains("tab-a"));
        render_devtools_console(
            &mut doc,
            &journal_b,
            &mut last_gen,
            &mut last_len,
            &mut last_total,
            true,
        );
        let text = log_text(&doc);
        assert!(text.contains("tab-b"), "{text}");
        assert!(!text.contains("tab-a"), "{text}");
    }

    #[test]
    fn tab_switch_epoch_drop_keeps_journal_rows() {
        let mut tabs = crate::tabs::TabManager::new();
        let id = tabs.add("https://a.test/");
        let tab = tabs.tab_mut(id).expect("tab");
        tab.console.push(ConsoleEntry {
            level: ConsoleLevel::Log,
            message: "kept".into(),
            source: None,
            line: None,
            timestamp_ms: 0,
            promise_id: None,
        });
        let old_epoch = tab.console_epoch;
        let mut js = crate::js::JsHost::new(false, crate::js::JsTap::new_disabled(), None);
        let tab = tabs.tab_mut(id).expect("tab");
        bump_tab_console_epoch_for_switch(tab, &mut js);
        let stale = ConsoleWireEvent {
            tab_id: id,
            page_epoch: old_epoch,
            timestamp_ms: 1,
            level: "log".into(),
            message: "stale".into(),
            source: None,
            line: None,
            promise_id: None,
        };
        let fresh = ConsoleWireEvent {
            tab_id: id,
            page_epoch: tab.console_epoch,
            timestamp_ms: 2,
            level: "log".into(),
            message: "fresh".into(),
            source: None,
            line: None,
            promise_id: None,
        };
        apply_console_wire(&mut tabs, &[stale, fresh]);
        let tab = tabs.tab_mut(id).expect("tab");
        assert_eq!(tab.console.len(), 2);
        let messages: Vec<_> = tab.console.entries().map(|e| e.message.as_str()).collect();
        assert!(messages.contains(&"kept"));
        assert!(messages.contains(&"fresh"));
        assert!(!messages.contains(&"stale"));
    }

    #[test]
    fn link_navigation_observation_clears_journal() {
        let mut tab = crate::tabs::Tab::new(3, "https://ex.test".into());
        tab.console.push(ConsoleEntry {
            level: ConsoleLevel::Log,
            message: "gone".into(),
            source: None,
            line: None,
            timestamp_ms: 0,
            promise_id: None,
        });
        let mut js = crate::js::JsHost::new(false, crate::js::JsTap::new_disabled(), None);
        let mut reset = |t: &mut crate::tabs::Tab, host: &mut crate::js::JsHost| {
            t.console.clear();
            t.console_epoch = t.console_epoch.saturating_add(1);
            host.set_console_route(t.id, t.console_epoch);
        };
        for effect in [
            crate::nav::ObserveEffect::LinkNavigation,
            crate::nav::ObserveEffect::Redirect,
        ] {
            tab.console.push(ConsoleEntry {
                level: ConsoleLevel::Log,
                message: "gone".into(),
                source: None,
                line: None,
                timestamp_ms: 0,
                promise_id: None,
            });
            let epoch_before = tab.console_epoch;
            observe_navigation_console_effect(effect, &mut tab, &mut js, &mut reset);
            assert!(tab.console.is_empty(), "{effect:?}");
            assert_eq!(
                tab.console_epoch,
                epoch_before.saturating_add(1),
                "{effect:?}"
            );
        }
    }

    #[test]
    fn reselect_active_tab_keeps_epoch_and_applies_following_wire() {
        let mut tabs = crate::tabs::TabManager::new();
        let id = tabs.add("https://a.test/");
        let epoch = tabs.tab_mut(id).expect("tab").console_epoch;
        assert!(!tabs.select(id));
        assert_eq!(tabs.tab_mut(id).expect("tab").console_epoch, epoch);
        let wire = ConsoleWireEvent {
            tab_id: id,
            page_epoch: epoch,
            timestamp_ms: 1,
            level: "log".into(),
            message: "still".into(),
            source: None,
            line: None,
            promise_id: None,
        };
        apply_console_wire(&mut tabs, &[wire]);
        let tab = tabs.tab_mut(id).expect("tab");
        assert_eq!(tab.console.len(), 1);
        assert_eq!(tab.console.entries().next().expect("row").message, "still");
    }

    #[test]
    fn same_url_tab_switch_routes_new_lines_to_active_tab() {
        let _lock = crate::js::console_feed::test_console_serial_lock();
        crate::js::console_feed::drain_pending();
        crate::js::console_feed::set_worker_route(0, 1);
        crate::js::console_feed::publish("log", "for-tab-a", None, None);
        crate::js::console_feed::set_worker_route(1, 1);
        crate::js::console_feed::publish("log", "for-tab-b", None, None);
        let batch = crate::js::console_feed::drain_pending();
        let mut tabs = crate::tabs::TabManager::new();
        let tab_a = tabs.add("https://same.test/");
        let tab_b = tabs.add("https://same.test/");
        assert!(tabs.select(tab_a));
        assert!(tabs.select(tab_b));
        apply_console_wire(&mut tabs, &batch);
        assert!(
            tabs.tab_mut(tab_a)
                .expect("tab a")
                .console
                .entries()
                .any(|e| e.message == "for-tab-a")
        );
        assert!(
            tabs.tab_mut(tab_b)
                .expect("tab b")
                .console
                .entries()
                .any(|e| e.message == "for-tab-b")
        );
    }

    #[test]
    fn revoke_removes_only_one_matching_rejection() {
        let mut journal = ConsoleJournal::default();
        for id in [1_u64, 2] {
            journal.push(ConsoleEntry {
                level: ConsoleLevel::UnhandledRejection,
                message: format!("r{id}"),
                source: None,
                line: None,
                timestamp_ms: id,
                promise_id: Some(id),
            });
        }
        assert!(journal.revoke_rejection(1));
        assert_eq!(journal.len(), 1);
        let left = journal.entries().next().expect("entry");
        assert_eq!(left.promise_id, Some(2));
    }
}
