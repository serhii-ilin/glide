//! DevTools panel shell state: dock side, size, active tab, and chrome commands.

pub mod console;

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
        self.state.close();
        self.sync_chrome(doc);
        if let Some(saved) = self.focus_restore.take() {
            restore_devtools_focus(doc, saved);
        }
    }

    pub fn set_dock(&mut self, dock: DevToolsDock, doc: &mut BaseDocument) {
        self.state.set_dock(dock);
        if self.state.open {
            self.state.reclamp_using_stage(doc);
            self.sync_chrome(doc);
        }
    }

    pub fn set_tab(&mut self, tab: DevToolsTab, doc: &mut BaseDocument) {
        self.state.set_tab(tab);
        if self.state.open {
            self.sync_chrome(doc);
        }
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
}
