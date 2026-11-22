//! DevTools panel shell state: dock side, size, active tab, and chrome commands.

pub mod computed;
pub mod console;
pub mod elements;
pub mod highlight;
pub mod inspect;
pub mod repl;

use blitz_dom::BaseDocument;
use serde::{Deserialize, Serialize};

use crate::chrome::{
    DevToolsFocusRestore, apply_devtools_state, capture_devtools_focus_restore,
    focus_devtools_panel, restore_devtools_focus, stage_axis_budget_logical_px,
};
use crate::profile::Settings;

pub const MIN_PANEL_PX: i32 = 120;
pub const DEFAULT_PANEL_PX: i32 = 280;
pub const MAX_PANEL_RATIO: f32 = 0.7;
pub const SPLITTER_PX: i32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DevToolsDock {
    #[default]
    Bottom,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DevToolsTab {
    #[default]
    Console,
    Elements,
    Network,
}

impl DevToolsTab {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "console" => Some(Self::Console),
            "elements" => Some(Self::Elements),
            "network" => Some(Self::Network),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Console => "console",
            Self::Elements => "elements",
            Self::Network => "network",
        }
    }
}

/// User-visible DevTools chrome state (persisted dock/size; `open` is never saved).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevToolsState {
    pub open: bool,
    pub dock: DevToolsDock,
    pub size_px: i32,
    pub tab: DevToolsTab,
}

impl Default for DevToolsState {
    fn default() -> Self {
        Self {
            open: false,
            dock: DevToolsDock::default(),
            size_px: DEFAULT_PANEL_PX,
            tab: DevToolsTab::default(),
        }
    }
}

impl DevToolsState {
    pub fn from_settings(dock: DevToolsDock, size_px: i32) -> Self {
        Self {
            open: false,
            dock,
            size_px: size_px.max(MIN_PANEL_PX),
            tab: DevToolsTab::default(),
        }
    }

    pub fn toggle_open(&mut self) {
        self.open = !self.open;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn set_dock(&mut self, dock: DevToolsDock) {
        self.dock = dock;
    }

    pub fn set_tab(&mut self, tab: DevToolsTab) {
        self.tab = tab;
    }

    pub fn reclamp_using_stage(&mut self, doc: &BaseDocument) {
        let Some(budget) = stage_axis_budget_logical_px(doc, self.dock) else {
            return;
        };
        self.size_px = clamp_panel_size(self.size_px, budget);
    }

    pub fn resize_clamped(&mut self, size_px: i32, stage_budget_logical_px: i32) {
        self.size_px = clamp_panel_size(size_px, stage_budget_logical_px);
    }
}

/// Map splitter drag in CSS pixels to logical panel size (panel is the trailing flex item).
pub fn panel_size_from_drag(
    start_size_logical: i32,
    start_axis_css: f32,
    axis_css: f32,
    page_zoom: f32,
) -> i32 {
    let delta_logical = (axis_css - start_axis_css) * page_zoom;
    (start_size_logical as f32 - delta_logical).round() as i32
}

/// Window-level DevTools UI state (not per-tab).
#[derive(Debug, Clone, Default)]
pub struct DevToolsSession {
    pub state: DevToolsState,
    focus_restore: Option<DevToolsFocusRestore>,
    console_render_generation: u64,
    console_rendered_len: usize,
    console_rendered_total: u64,
    console_last_tab: Option<crate::tabs::TabId>,
    elements_last_tab: Option<crate::tabs::TabId>,
    elements_last_stamp: Option<elements::PageTreeStamp>,
    elements_last_ui_generation: u64,
    elements_last_tree_html: String,
    elements_last_detail_html: String,
    elements_last_computed_html: String,
    elements_last_highlight: Option<highlight::HighlightRect>,
    pub inspect_mode: bool,
    pub repl_ui: repl::ConsoleReplUi,
}

impl DevToolsSession {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            state: DevToolsState::from_settings(settings.devtools_dock, settings.devtools_size_px),
            focus_restore: None,
            ..Default::default()
        }
    }

    pub fn sync_chrome(&self, doc: &mut BaseDocument) {
        apply_devtools_state(doc, &self.state);
    }

    pub fn toggle(&mut self, doc: &mut BaseDocument) {
        if self.state.open {
            self.close(doc);
            return;
        }
        self.focus_restore = Some(capture_devtools_focus_restore(doc));
        self.state.toggle_open();
        self.state.reclamp_using_stage(doc);
        self.sync_chrome(doc);
        focus_devtools_panel(doc);
    }

    pub fn close(&mut self, doc: &mut BaseDocument) {
        if !self.state.open {
            return;
        }
        self.exit_inspect_mode(doc);
        self.state.close();
        self.sync_chrome(doc);
        highlight::apply_inspect_highlight(doc, None);
        self.elements_last_highlight = None;
        if let Some(saved) = self.focus_restore.take() {
            restore_devtools_focus(doc, saved);
        }
    }

    pub fn exit_inspect_mode(&mut self, doc: &mut BaseDocument) {
        if !self.inspect_mode {
            return;
        }
        self.inspect_mode = false;
        crate::chrome::set_inspect_toggle_active(doc, false);
        inspect::exit_inspect_visuals(doc);
    }

    pub fn toggle_inspect_mode(&mut self, doc: &mut BaseDocument) {
        if self.inspect_mode {
            self.exit_inspect_mode(doc);
            return;
        }
        if !self.state.open {
            self.focus_restore = Some(capture_devtools_focus_restore(doc));
            self.state.open = true;
            self.state.reclamp_using_stage(doc);
        }
        self.state.tab = DevToolsTab::Elements;
        self.inspect_mode = true;
        self.sync_chrome(doc);
        crate::chrome::set_inspect_toggle_active(doc, true);
    }

    pub fn update_inspect_hover(&mut self, chrome: &mut BaseDocument, x: f32, y: f32) -> bool {
        if !self.inspect_mode {
            return false;
        }
        if !inspect::chrome_pointer_targets_page(chrome, x, y) {
            inspect::exit_inspect_visuals(chrome);
            return true;
        }
        let Some(hit) = inspect::page_hit_at_chrome_point(chrome, x, y) else {
            inspect::exit_inspect_visuals(chrome);
            return true;
        };
        let Some(element) = inspect::inspectable_element_for_hit(chrome, hit) else {
            inspect::exit_inspect_visuals(chrome);
            return true;
        };
        let label = elements::with_page_document(chrome, |page| {
            inspect::inspect_element_label(page, element)
        });
        let rect = elements::with_page_document(chrome, |page| {
            inspect::highlight_for_page_node(chrome, page, element)
        })
        .flatten();
        if let Some(r) = rect {
            highlight::apply_inspect_highlight(chrome, Some(r));
            inspect::apply_inspect_label(chrome, label.as_deref());
        } else {
            inspect::exit_inspect_visuals(chrome);
        }
        true
    }

    pub fn set_dock(&mut self, dock: DevToolsDock, doc: &mut BaseDocument) {
        self.state.set_dock(dock);
        if self.state.open {
            self.state.reclamp_using_stage(doc);
            self.sync_chrome(doc);
        }
    }

    pub fn set_tab(&mut self, tab: DevToolsTab, doc: &mut BaseDocument) {
        self.exit_inspect_mode(doc);
        self.state.set_tab(tab);
        if self.state.open {
            self.sync_chrome(doc);
            if tab != DevToolsTab::Elements {
                highlight::apply_inspect_highlight(doc, None);
                self.elements_last_highlight = None;
            }
        }
    }

    pub fn sync_elements_pane(
        &mut self,
        chrome: &mut BaseDocument,
        tab: &mut crate::tabs::Tab,
    ) -> bool {
        let tab_changed = self.elements_last_tab != Some(tab.id);
        if tab_changed {
            self.exit_inspect_mode(chrome);
            self.elements_last_stamp = None;
            self.elements_last_ui_generation = u64::MAX;
            self.elements_last_tree_html.clear();
            self.elements_last_detail_html.clear();
            self.elements_last_computed_html.clear();
            self.elements_last_highlight = None;
            self.elements_last_tab = Some(tab.id);
        }
        if !(self.state.open && self.state.tab == DevToolsTab::Elements) {
            highlight::apply_inspect_highlight(chrome, None);
            self.elements_last_highlight = None;
            return false;
        }
        chrome.resolve(0.0);
        let _ = crate::find::with_page_document_mut(chrome, |page| {
            page.resolve(0.0);
        });
        let plan = match elements::with_page_document(chrome, |page| {
            if tab.elements.expanded.is_empty() {
                tab.elements.apply_default_expansion(page);
            }
            let stamp = elements::page_tree_stamp(page, tab.elements_epoch);
            elements::prepare_elements_render(
                page,
                &mut tab.elements,
                stamp,
                &self.elements_last_stamp,
                self.elements_last_ui_generation,
                tab_changed,
            )
        }) {
            Some(Some(plan)) => plan,
            _ => {
                highlight::apply_inspect_highlight(chrome, None);
                self.elements_last_highlight = None;
                return false;
            }
        };
        let mut painted = false;
        if plan.tree_html != self.elements_last_tree_html
            || plan.detail_html != self.elements_last_detail_html
            || plan.computed_html != self.elements_last_computed_html
        {
            painted = elements::paint_elements_render(chrome, &plan);
            if painted {
                self.elements_last_stamp = Some(plan.stamp);
                self.elements_last_ui_generation = tab.elements.ui_generation;
                self.elements_last_tree_html = plan.tree_html.clone();
                self.elements_last_detail_html = plan.detail_html.clone();
                self.elements_last_computed_html = plan.computed_html.clone();
            }
        }
        let highlight = if self.inspect_mode {
            None
        } else {
            elements::with_page_document(chrome, |page| {
                highlight::highlight_for_selected(chrome, page, tab.elements.selected)
            })
            .flatten()
        };
        let highlight_changed = highlight != self.elements_last_highlight;
        if highlight_changed {
            highlight::apply_inspect_highlight(chrome, highlight);
            self.elements_last_highlight = highlight;
        }
        painted || highlight_changed
    }

    pub fn sync_console_pane(
        &mut self,
        doc: &mut BaseDocument,
        journal: &console::ConsoleJournal,
        tab_id: crate::tabs::TabId,
    ) -> bool {
        let tab_changed = self.console_last_tab != Some(tab_id);
        if tab_changed {
            self.console_render_generation = u64::MAX;
            self.console_rendered_len = 0;
            self.console_rendered_total = 0;
            self.console_last_tab = Some(tab_id);
            self.repl_ui.reset_after_submit();
        }
        if self.state.open && self.state.tab == DevToolsTab::Console {
            console::render_devtools_console(
                doc,
                journal,
                &mut self.console_render_generation,
                &mut self.console_rendered_len,
                &mut self.console_rendered_total,
                tab_changed,
            )
        } else {
            false
        }
    }

    /// Reclamp after layout changes (open panel, window resize). Returns whether size changed.
    pub fn reclamp_open_panel(&mut self, doc: &mut BaseDocument) -> bool {
        if !self.state.open {
            return false;
        }
        let before = self.state.size_px;
        self.state.reclamp_using_stage(doc);
        if before != self.state.size_px {
            self.sync_chrome(doc);
            true
        } else {
            false
        }
    }

    pub fn resize_from_drag(&mut self, doc: &mut BaseDocument, size_px: i32) -> Option<i32> {
        if !self.state.open {
            return None;
        }
        let budget = stage_axis_budget_logical_px(doc, self.state.dock)?;
        self.state.resize_clamped(size_px, budget);
        self.sync_chrome(doc);
        Some(self.state.size_px)
    }

    pub fn settings_fields(&self) -> (DevToolsDock, i32) {
        (self.state.dock, self.state.size_px)
    }
}

/// Clamp panel width/height in logical pixels (the `N` in `calc(Npx * var(--ui))`).
pub fn clamp_panel_size(size_px: i32, stage_axis_logical_px: i32) -> i32 {
    let stage_axis_logical_px = stage_axis_logical_px.max(MIN_PANEL_PX);
    let max_px = ((stage_axis_logical_px as f32) * MAX_PANEL_RATIO).floor() as i32;
    let max_px = max_px.max(MIN_PANEL_PX);
    size_px.clamp(MIN_PANEL_PX, max_px)
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

    fn parse_chrome(html: &str) -> BaseDocument {
        let config = DocumentConfig {
            viewport: Some(Viewport::new(1100, 760, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(GlideHtmlProvider::default())),
            ..DocumentConfig::default()
        };
        parse::document_from_html(html, config, false)
    }

    #[test]
    fn clamp_panel_size_respects_bounds() {
        assert_eq!(clamp_panel_size(50, 1000), MIN_PANEL_PX);
        assert_eq!(clamp_panel_size(10_000, 1000), 700);
        assert_eq!(clamp_panel_size(400, 1000), 400);
    }

    #[test]
    fn clamp_panel_size_handles_small_stage() {
        assert_eq!(clamp_panel_size(200, 100), MIN_PANEL_PX);
    }

    #[test]
    fn from_settings_does_not_clamp_oversized_without_stage() {
        let state = DevToolsState::from_settings(DevToolsDock::Bottom, 5000);
        assert_eq!(state.size_px, 5000);
    }

    #[test]
    fn oversized_panel_size_reclamped_when_opened() {
        let mut doc = parse_chrome(&chrome_document("about:blank", true, false));
        doc.resolve(0.0);
        let mut state = DevToolsState::from_settings(DevToolsDock::Bottom, 5000);
        state.open = true;
        state.reclamp_using_stage(&doc);
        let budget = crate::chrome::stage_axis_budget_logical_px(&doc, DevToolsDock::Bottom)
            .expect("stage laid out");
        let max_expected = ((budget as f32) * MAX_PANEL_RATIO).floor() as i32;
        assert!(
            state.size_px <= max_expected,
            "size {} should be <= {max_expected}",
            state.size_px
        );
        assert!(state.size_px >= MIN_PANEL_PX);
    }

    #[test]
    fn panel_size_from_drag_shrinks_when_splitter_moves_down() {
        assert_eq!(panel_size_from_drag(280, 100.0, 120.0, 1.0), 260);
        assert_eq!(panel_size_from_drag(280, 100.0, 120.0, 2.0), 240);
    }

    #[test]
    fn panel_size_from_drag_grows_when_splitter_moves_up() {
        assert_eq!(panel_size_from_drag(280, 120.0, 100.0, 1.0), 300);
    }

    #[test]
    fn devtools_tab_parse_accepts_known_names() {
        assert_eq!(DevToolsTab::parse("console"), Some(DevToolsTab::Console));
        assert_eq!(DevToolsTab::parse("elements"), Some(DevToolsTab::Elements));
        assert_eq!(DevToolsTab::parse("network"), Some(DevToolsTab::Network));
        assert!(DevToolsTab::parse("sources").is_none());
    }

    #[test]
    fn resize_clamped_updates_size() {
        let mut state = DevToolsState::default();
        state.resize_clamped(500, 800);
        assert_eq!(state.size_px, 500);
        state.resize_clamped(900, 800);
        assert_eq!(state.size_px, 560);
    }

    #[test]
    fn toggle_open_flips_flag() {
        let mut state = DevToolsState::default();
        assert!(!state.open);
        state.toggle_open();
        assert!(state.open);
        state.toggle_open();
        assert!(!state.open);
    }

    #[test]
    fn from_settings_starts_closed() {
        let state = DevToolsState::from_settings(DevToolsDock::Right, 320);
        assert!(!state.open);
        assert_eq!(state.dock, DevToolsDock::Right);
        assert_eq!(state.size_px, 320);
    }

    #[test]
    fn sync_elements_pane_includes_computed_styles() {
        let html = include_str!("../../tests/fixtures/devtools-computed-sample.html");
        let mut page = parse::document_from_html(html, DocumentConfig::default(), true);
        page.resolve(0.0);
        let mut tab = crate::tabs::Tab::new(1, "about:blank".into());
        tab.elements.apply_default_expansion(&page);
        let p = page.get_element_by_id("box").expect("box");
        tab.elements
            .set_selected_handle(Some(elements::node_to_handle(p)));
        let stamp = elements::page_tree_stamp(&page, tab.elements_epoch);
        let plan = elements::prepare_elements_render(
            &page,
            &mut tab.elements,
            stamp,
            &None,
            u64::MAX,
            true,
        )
        .expect("plan");
        assert!(plan.computed_html.contains("display"));
        assert!(plan.computed_html.contains("flex"));
        assert!(plan.computed_html.contains("devtools-computed-row"));
    }

    #[test]
    fn sync_elements_pane_renders_tree() {
        let page = parse::document_from_html(
            r#"<!DOCTYPE html><html><body><p id="t" class="ok">hi</p></body></html>"#,
            DocumentConfig::default(),
            true,
        );
        let mut chrome = parse_chrome(&chrome_document("about:blank", true, false));
        chrome.resolve(0.0);
        let mut tab = crate::tabs::Tab::new(1, "about:blank".into());
        tab.elements.apply_default_expansion(&page);
        let stamp = elements::page_tree_stamp(&page, tab.elements_epoch);
        let plan = elements::prepare_elements_render(
            &page,
            &mut tab.elements,
            stamp,
            &None,
            u64::MAX,
            true,
        )
        .expect("plan");
        assert!(plan.tree_html.contains("&lt;p"));
        assert!(elements::paint_elements_render(&mut chrome, &plan));
        let tree = chrome
            .get_element_by_id("devtools-elements-tree")
            .expect("tree");
        assert!(
            chrome
                .get_node(tree)
                .map(|n| !n.children.is_empty())
                .unwrap_or(false)
        );
    }

    #[test]
    fn inspect_exits_when_devtools_tab_changes() {
        use blitz_dom::local_name;

        let mut chrome = parse_chrome(&chrome_document("about:blank", true, false));
        chrome.resolve(0.0);
        let mut session = DevToolsSession {
            inspect_mode: true,
            state: DevToolsState {
                open: true,
                tab: DevToolsTab::Elements,
                ..Default::default()
            },
            ..Default::default()
        };
        crate::chrome::set_inspect_toggle_active(&mut chrome, true);
        session.set_tab(DevToolsTab::Console, &mut chrome);
        assert!(!session.inspect_mode);
        let btn = chrome
            .get_element_by_id("devtools-inspect-toggle")
            .expect("toggle");
        let class = chrome
            .get_node(btn)
            .and_then(|n| n.attr(local_name!("class")))
            .unwrap_or_default();
        assert!(!class.contains("active"));
    }

    #[test]
    fn highlight_hidden_when_devtools_tab_leaves_elements() {
        use blitz_dom::local_name;

        let mut chrome = parse_chrome(&chrome_document("about:blank", true, false));
        chrome.resolve(0.0);
        let mut session = DevToolsSession {
            state: DevToolsState {
                open: true,
                tab: DevToolsTab::Elements,
                ..Default::default()
            },
            ..Default::default()
        };
        highlight::apply_inspect_highlight(
            &mut chrome,
            Some(highlight::HighlightRect {
                left: 12.0,
                top: 34.0,
                width: 40.0,
                height: 20.0,
            }),
        );
        session.set_tab(DevToolsTab::Console, &mut chrome);
        let hl = chrome.get_element_by_id("inspect-hl").expect("hl");
        let style = chrome
            .get_node(hl)
            .and_then(|n| n.attr(local_name!("style")))
            .unwrap_or_default();
        assert!(style.contains("display:none"), "{style}");
    }

    #[test]
    fn sync_console_pane_resets_repl_ui_on_tab_change() {
        let html = chrome_document("about:blank", true, false);
        let mut doc = parse_chrome(&html);
        doc.resolve(0.0);
        let journal = console::ConsoleJournal::default();
        let mut session = DevToolsSession {
            console_last_tab: Some(1),
            state: DevToolsState {
                open: true,
                tab: DevToolsTab::Console,
                ..Default::default()
            },
            ..Default::default()
        };
        session.repl_ui.test_set_cursor(Some(0), "draft");
        session.sync_console_pane(&mut doc, &journal, 2);
        assert!(session.repl_ui.test_cursor_cleared());
    }
}
