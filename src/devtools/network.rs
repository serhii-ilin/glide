//! DevTools Network tab: per-tab request log and chrome rendering.

use std::collections::VecDeque;

use crate::tabs::TabId;
use blitz_dom::BaseDocument;

pub const NETWORK_CAPACITY: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkResourceKind {
    Document,
    Stylesheet,
    Script,
    Image,
    Font,
    Fetch,
    Xhr,
    Other,
}

impl NetworkResourceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Stylesheet => "stylesheet",
            Self::Script => "script",
            Self::Image => "image",
            Self::Font => "font",
            Self::Fetch => "fetch",
            Self::Xhr => "xhr",
            Self::Other => "other",
        }
    }

    pub fn from_url_and_initiator(url: &str, initiator: NetworkInitiator) -> Self {
        if initiator == NetworkInitiator::Fetch {
            return Self::Fetch;
        }
        if initiator == NetworkInitiator::Xhr {
            return Self::Xhr;
        }
        let path = url.rsplit_once('?').map(|(p, _)| p).unwrap_or(url);
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".css") {
            return Self::Stylesheet;
        }
        if lower.ends_with(".js") || lower.ends_with(".mjs") {
            return Self::Script;
        }
        if lower.ends_with(".woff")
            || lower.ends_with(".woff2")
            || lower.ends_with(".ttf")
            || lower.ends_with(".otf")
        {
            return Self::Font;
        }
        if lower.ends_with(".png")
            || lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".gif")
            || lower.ends_with(".webp")
            || lower.ends_with(".svg")
        {
            return Self::Image;
        }
        match initiator {
            NetworkInitiator::Document => Self::Document,
            NetworkInitiator::Subresource => Self::Other,
            NetworkInitiator::Fetch | NetworkInitiator::Xhr => Self::Fetch,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkInitiator {
    Document,
    Subresource,
    Fetch,
    Xhr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkEntry {
    pub id: u64,
    pub method: String,
    pub url: String,
    pub kind: NetworkResourceKind,
    pub status: Option<u16>,
    pub error: Option<String>,
    pub started_ms: u64,
    pub duration_ms: u64,
    pub size_bytes: Option<u64>,
    pub initiator: NetworkInitiator,
    pub response_headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkPanelState {
    pub selected: Option<u64>,
    pub filter: String,
}

impl NetworkPanelState {
    pub fn reset_for_navigation(&mut self) {
        self.selected = None;
    }
}

#[derive(Debug, Clone, Default)]
pub struct NetworkJournal {
    entries: VecDeque<NetworkEntry>,
    generation: u64,
    next_id: u64,
}

impl NetworkJournal {
    pub fn push_completed(&mut self, mut entry: NetworkEntry) {
        entry.id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        if self.entries.len() >= NETWORK_CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
        self.generation = self.generation.saturating_add(1);
    }

    pub fn clear(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.entries.clear();
        self.generation = self.generation.saturating_add(1);
    }

    /// After in-document navigation: drop prior page loads, keep the committed document row.
    pub fn prune_for_link_navigation(&mut self, committed_url: &str) {
        let had = self.entries.len();
        self.entries
            .retain(|e| e.initiator == NetworkInitiator::Document && e.url == committed_url);
        if had != self.entries.len() {
            self.generation = self.generation.saturating_add(1);
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn entries(&self) -> impl Iterator<Item = &NetworkEntry> {
        self.entries.iter()
    }

    pub fn entry_by_id(&self, id: u64) -> Option<&NetworkEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

/// Wire payload from the network layer or JS worker (tab-scoped like console).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkWireEvent {
    pub tab_id: TabId,
    pub network_epoch: u64,
    pub method: String,
    pub url: String,
    pub initiator: NetworkInitiator,
    pub status: Option<u16>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub size_bytes: Option<u64>,
    pub response_headers: Vec<(String, String)>,
    pub started_ms: u64,
}

pub fn apply_network_wire(tabs: &mut crate::tabs::TabManager, events: &[NetworkWireEvent]) -> bool {
    let mut changed = false;
    for ev in events {
        let Some(tab) = tabs.tab_mut(ev.tab_id) else {
            continue;
        };
        if ev.network_epoch > tab.network_epoch {
            tab.network_epoch = ev.network_epoch;
        } else if ev.network_epoch < tab.network_epoch {
            continue;
        }
        let kind = NetworkResourceKind::from_url_and_initiator(&ev.url, ev.initiator);
        tab.network.push_completed(NetworkEntry {
            id: 0,
            method: ev.method.clone(),
            url: ev.url.clone(),
            kind,
            status: ev.status,
            error: ev.error.clone(),
            started_ms: ev.started_ms,
            duration_ms: ev.duration_ms,
            size_bytes: ev.size_bytes,
            initiator: ev.initiator,
            response_headers: ev.response_headers.clone(),
        });
        changed = true;
    }
    changed
}

pub(crate) fn bump_tab_network_epoch_for_navigation(tab: &mut crate::tabs::Tab) {
    tab.network_epoch = tab.network_epoch.saturating_add(1);
    tab.network.clear();
    tab.network_panel.reset_for_navigation();
    crate::network_feed::discard_pending_for_tab(tab.id);
}

pub(crate) fn observe_navigation_network_effect(
    effect: crate::nav::ObserveEffect,
    tab: &mut crate::tabs::Tab,
    committed_url: &str,
) {
    match effect {
        crate::nav::ObserveEffect::Completed | crate::nav::ObserveEffect::Unchanged => {}
        crate::nav::ObserveEffect::LinkNavigation | crate::nav::ObserveEffect::Redirect => {
            tab.network.prune_for_link_navigation(committed_url);
            crate::network_feed::discard_pending_below_epoch(tab.id, tab.network_epoch);
        }
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn format_size(size: Option<u64>) -> String {
    match size {
        Some(n) if n < 1024 => format!("{n} B"),
        Some(n) => format!("{:.1} KiB", n as f64 / 1024.0),
        None => "—".to_string(),
    }
}

fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.2} s", ms as f64 / 1000.0)
    }
}

fn display_name(url: &str) -> String {
    url.rsplit('/').next().unwrap_or(url).to_string()
}

fn entry_matches_filter(entry: &NetworkEntry, filter: &str) -> bool {
    let f = filter.trim().to_ascii_lowercase();
    if f.is_empty() {
        return true;
    }
    let name = display_name(&entry.url).to_ascii_lowercase();
    entry.url.to_ascii_lowercase().contains(&f) || name.contains(&f)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRenderPlan {
    pub table_html: String,
    pub detail_html: String,
}

pub fn prepare_network_render(
    journal: &NetworkJournal,
    panel: &NetworkPanelState,
    tab_changed: bool,
) -> Option<NetworkRenderPlan> {
    let mut rows = String::new();
    for entry in journal.entries() {
        if !entry_matches_filter(entry, &panel.filter) {
            continue;
        }
        let selected = panel.selected == Some(entry.id);
        let status = entry
            .status
            .map(|s| s.to_string())
            .or_else(|| entry.error.as_ref().map(|e| escape_html(e)))
            .unwrap_or_else(|| "—".to_string());
        let name = escape_html(&display_name(&entry.url));
        rows.push_str(&format!(
            r#"<tr class="devtools-network-row{sel}"><td><a class="devtools-network-row-link" href="glide://devtools-network-select?id={id}">{name}</a></td><td class="status">{status}</td><td>{kind}</td><td class="size">{size}</td><td class="time">{time}</td></tr>
"#,
            sel = if selected { " selected" } else { "" },
            id = entry.id,
            name = name,
            status = status,
            kind = entry.kind.label(),
            size = format_size(entry.size_bytes),
            time = format_duration(entry.duration_ms),
        ));
    }
    let table_html = if rows.is_empty() {
        r#"<table class="devtools-network-table"><thead><tr><th>Name</th><th>Status</th><th>Type</th><th>Size</th><th>Time</th></tr></thead><tbody><tr class="empty"><td colspan="5">No requests</td></tr></tbody></table>"#.to_string()
    } else {
        format!(
            r#"<table class="devtools-network-table"><thead><tr><th>Name</th><th>Status</th><th>Type</th><th>Size</th><th>Time</th></tr></thead><tbody>{rows}</tbody></table>"#
        )
    };
    let detail_html = panel
        .selected
        .and_then(|id| journal.entry_by_id(id))
        .map(detail_for_entry)
        .unwrap_or_else(|| {
            r#"<p class="devtools-network-detail-empty">Select a request to see details</p>"#
                .to_string()
        });
    let _ = tab_changed;
    Some(NetworkRenderPlan {
        table_html,
        detail_html,
    })
}

fn detail_for_entry(entry: &NetworkEntry) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        r#"<dl class="devtools-network-detail"><dt>URL</dt><dd>{url}</dd><dt>Method</dt><dd>{method}</dd><dt>Status</dt><dd>{status}</dd><dt>Type</dt><dd>{kind}</dd><dt>Size</dt><dd>{size}</dd><dt>Time</dt><dd>{time}</dd>"#,
        url = escape_html(&entry.url),
        method = escape_html(&entry.method),
        status = entry
            .status
            .map(|s| s.to_string())
            .or_else(|| entry.error.as_ref().map(|e| escape_html(e)))
            .unwrap_or_else(|| "—".to_string()),
        kind = entry.kind.label(),
        size = format_size(entry.size_bytes),
        time = format_duration(entry.duration_ms),
    ));
    if !entry.response_headers.is_empty() {
        out.push_str("<dt>Response headers</dt><dd><pre class=\"devtools-network-headers\">");
        for (k, v) in &entry.response_headers {
            out.push_str(&escape_html(k));
            out.push_str(": ");
            out.push_str(&escape_html(v));
            out.push('\n');
        }
        out.push_str("</pre></dd>");
    }
    out.push_str("</dl>");
    out
}

pub fn paint_network_render(doc: &mut BaseDocument, plan: &NetworkRenderPlan) -> bool {
    let Some(table_id) = doc.get_element_by_id("devtools-network-table-wrap") else {
        return false;
    };
    let Some(detail_id) = doc.get_element_by_id("devtools-network-detail") else {
        return false;
    };
    let mut m = doc.mutate();
    for child in m.child_ids(table_id) {
        m.remove_and_drop_node(child);
    }
    for child in m.child_ids(detail_id) {
        m.remove_and_drop_node(child);
    }
    crate::parse::parse_inner_html_into_mutator(&mut m, table_id, &plan.table_html, false);
    crate::parse::parse_inner_html_into_mutator(&mut m, detail_id, &plan.detail_html, false);
    true
}

pub fn sync_network_filter_input(doc: &mut BaseDocument, filter: &str) -> bool {
    let Some(input) = doc.get_element_by_id("devtools-network-filter") else {
        return false;
    };
    doc.with_text_input(input, |mut driver| {
        driver.editor.set_text(filter);
        driver.refresh_layout();
    });
    true
}

pub fn read_network_filter_input(doc: &BaseDocument) -> String {
    let Some(id) = doc.get_element_by_id("devtools-network-filter") else {
        return String::new();
    };
    let Some(input) = doc
        .get_node(id)
        .and_then(|n| n.element_data())
        .and_then(|e| e.text_input_data())
    else {
        return String::new();
    };
    input.editor.text().to_string()
}

pub fn render_devtools_network(
    doc: &mut BaseDocument,
    journal: &NetworkJournal,
    panel: &mut NetworkPanelState,
    last_generation: &mut u64,
    last_table_html: &mut String,
    last_detail_html: &mut String,
    tab_changed: bool,
) -> bool {
    let filter = read_network_filter_input(doc);
    if filter != panel.filter {
        panel.filter = filter;
    }
    let Some(plan) = prepare_network_render(journal, panel, tab_changed) else {
        return false;
    };
    if !tab_changed
        && journal.generation() == *last_generation
        && plan.table_html == *last_table_html
        && plan.detail_html == *last_detail_html
    {
        return false;
    }
    let painted = paint_network_render(doc, &plan);
    if painted {
        *last_generation = journal.generation();
        *last_table_html = plan.table_html;
        *last_detail_html = plan.detail_html;
    }
    painted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(id: u64, url: &str, status: u16) -> NetworkEntry {
        NetworkEntry {
            id,
            method: "GET".into(),
            url: url.into(),
            kind: NetworkResourceKind::Document,
            status: Some(status),
            error: None,
            started_ms: 0,
            duration_ms: 12,
            size_bytes: Some(400),
            initiator: NetworkInitiator::Document,
            response_headers: vec![("content-type".into(), "text/html".into())],
        }
    }

    #[test]
    fn journal_stores_entries_in_order() {
        let mut journal = NetworkJournal::default();
        journal.push_completed(sample_entry(0, "file://a.html", 200));
        journal.push_completed(sample_entry(0, "file://b.css", 200));
        let urls: Vec<_> = journal.entries().map(|e| e.url.as_str()).collect();
        assert_eq!(urls, ["file://a.html", "file://b.css"]);
        assert_eq!(journal.entries().next().unwrap().id, 0);
        assert_eq!(journal.entries().nth(1).unwrap().id, 1);
    }

    #[test]
    fn clear_resets_journal() {
        let mut journal = NetworkJournal::default();
        journal.push_completed(sample_entry(0, "data:,x", 200));
        journal.clear();
        assert!(journal.entries().next().is_none());
        assert_eq!(journal.generation(), 2);
    }

    #[test]
    fn filter_matches_name_and_url() {
        let mut journal = NetworkJournal::default();
        journal.push_completed(sample_entry(0, "file:///tmp/foo/bar.png", 200));
        let panel = NetworkPanelState {
            filter: "bar".into(),
            ..Default::default()
        };
        let plan = prepare_network_render(&journal, &panel, false).expect("plan");
        assert!(plan.table_html.contains("bar.png"));
        assert!(!plan.table_html.contains("No requests"));
    }

    #[test]
    fn render_table_includes_columns() {
        let mut journal = NetworkJournal::default();
        journal.push_completed(sample_entry(0, "file://app.js", 200));
        let plan =
            prepare_network_render(&journal, &NetworkPanelState::default(), false).expect("plan");
        assert!(plan.table_html.contains("<th>Status</th>"));
        assert!(plan.table_html.contains("app.js"));
        assert!(plan.table_html.contains("200"));
    }

    #[test]
    fn row_links_use_glide_select_command() {
        let mut journal = NetworkJournal::default();
        journal.push_completed(sample_entry(0, "file://x", 200));
        let plan =
            prepare_network_render(&journal, &NetworkPanelState::default(), false).expect("plan");
        assert!(
            plan.table_html
                .contains("glide://devtools-network-select?id=0")
        );
    }

    #[test]
    fn detail_html_includes_headers() {
        let mut entry = sample_entry(0, "file://x", 200);
        entry.response_headers = vec![("x-demo".into(), "Tom&Jerry".into())];
        let html = detail_for_entry(&entry);
        assert!(html.contains("Tom&amp;Jerry"));
        assert!(!html.contains("&amp;amp;"));
        assert!(!html.contains("Tom&Jerry"));
    }

    #[test]
    fn detail_html_escapes_error_payload() {
        let entry = NetworkEntry {
            id: 0,
            method: "GET".into(),
            url: "file://x".into(),
            kind: NetworkResourceKind::Document,
            status: None,
            error: Some(r#"<a href="glide://devtools-network-clear">"#.into()),
            started_ms: 0,
            duration_ms: 1,
            size_bytes: None,
            initiator: NetworkInitiator::Document,
            response_headers: Vec::new(),
        };
        let html = detail_for_entry(&entry);
        assert!(html.contains("&lt;a href="));
        assert!(!html.contains(r#"<a href="glide://"#));
    }

    #[test]
    fn filter_editor_text_hides_non_matching_rows() {
        let html = crate::chrome::chrome_document("file://page", true, false);
        let mut doc =
            crate::parse::document_from_html(&html, blitz_dom::DocumentConfig::default(), false);
        doc.resolve(1.0);
        crate::chrome::apply_devtools_state(
            &mut doc,
            &crate::devtools::DevToolsState {
                open: true,
                tab: crate::devtools::DevToolsTab::Network,
                size_px: 240,
                ..Default::default()
            },
        );
        doc.resolve(1.0);
        let mut journal = NetworkJournal::default();
        journal.push_completed(NetworkEntry {
            id: 0,
            method: "GET".into(),
            url: "file://a/page.css".into(),
            kind: NetworkResourceKind::Stylesheet,
            status: Some(200),
            error: None,
            started_ms: 0,
            duration_ms: 1,
            size_bytes: Some(10),
            initiator: NetworkInitiator::Subresource,
            response_headers: Vec::new(),
        });
        journal.push_completed(NetworkEntry {
            id: 0,
            method: "GET".into(),
            url: "file://a/app.js".into(),
            kind: NetworkResourceKind::Script,
            status: Some(200),
            error: None,
            started_ms: 0,
            duration_ms: 1,
            size_bytes: Some(10),
            initiator: NetworkInitiator::Subresource,
            response_headers: Vec::new(),
        });
        let Some(input) = doc.get_element_by_id("devtools-network-filter") else {
            panic!("network filter");
        };
        doc.with_text_input(input, |mut driver| {
            driver.editor.set_text("css");
            driver.refresh_layout();
        });
        assert_eq!(read_network_filter_input(&doc), "css");
        let mut panel = NetworkPanelState {
            filter: read_network_filter_input(&doc),
            ..Default::default()
        };
        let plan = prepare_network_render(&journal, &panel, false).expect("plan");
        assert!(plan.table_html.contains("page.css"));
        assert!(!plan.table_html.contains("app.js"));
        doc.with_text_input(input, |mut driver| {
            driver.editor.set_text("");
            driver.refresh_layout();
        });
        assert!(read_network_filter_input(&doc).is_empty());
        panel.filter.clear();
        journal.push_completed(NetworkEntry {
            id: 0,
            method: "GET".into(),
            url: "file://b/extra.js".into(),
            kind: NetworkResourceKind::Script,
            status: Some(200),
            error: None,
            started_ms: 0,
            duration_ms: 1,
            size_bytes: Some(10),
            initiator: NetworkInitiator::Subresource,
            response_headers: Vec::new(),
        });
        let plan = prepare_network_render(&journal, &panel, false).expect("plan");
        assert!(plan.table_html.contains("app.js"));
    }

    #[test]
    fn completed_observation_leaves_journal_unchanged() {
        let mut tab = crate::tabs::Tab::new(1, "https://a.test/".into());
        tab.network
            .push_completed(sample_entry(0, "file://old.css", 200));
        let count = tab.network.entries().count();
        observe_navigation_network_effect(
            crate::nav::ObserveEffect::Completed,
            &mut tab,
            "https://b.test/",
        );
        assert_eq!(tab.network.entries().count(), count);
    }

    #[test]
    fn link_navigation_prune_keeps_document_drops_subresources() {
        let mut tab = crate::tabs::Tab::new(1, "https://a.test/".into());
        let new_doc = "https://b.test/".to_string();
        tab.network.push_completed(NetworkEntry {
            id: 0,
            method: "GET".into(),
            url: "file://old.css".into(),
            kind: NetworkResourceKind::Stylesheet,
            status: Some(200),
            error: None,
            started_ms: 0,
            duration_ms: 1,
            size_bytes: Some(1),
            initiator: NetworkInitiator::Subresource,
            response_headers: Vec::new(),
        });
        tab.network.push_completed(NetworkEntry {
            id: 0,
            method: "GET".into(),
            url: new_doc.clone(),
            kind: NetworkResourceKind::Document,
            status: Some(200),
            error: None,
            started_ms: 0,
            duration_ms: 1,
            size_bytes: Some(100),
            initiator: NetworkInitiator::Document,
            response_headers: Vec::new(),
        });
        observe_navigation_network_effect(
            crate::nav::ObserveEffect::LinkNavigation,
            &mut tab,
            &new_doc,
        );
        let urls: Vec<_> = tab.network.entries().map(|e| e.url.as_str()).collect();
        assert_eq!(urls, [new_doc.as_str()]);
    }

    #[test]
    fn clear_on_navigation_epoch() {
        let mut tab = crate::tabs::Tab::new(1, "about:blank".into());
        tab.network
            .push_completed(sample_entry(0, "file://old", 200));
        bump_tab_network_epoch_for_navigation(&mut tab);
        assert!(tab.network.entries().next().is_none());
        assert_eq!(tab.network_epoch, 2);
    }
}
