//! DevTools panel shell state: dock side, size, active tab, and chrome commands.

use blitz_dom::BaseDocument;
use serde::{Deserialize, Serialize};

use crate::chrome::{apply_devtools_state, focus_devtools_panel, stage_layout_axis_px};
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
            size_px: clamp_panel_size(size_px, i32::MAX / 2),
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

    pub fn resize_clamped(&mut self, size_px: i32, stage_axis_px: i32) {
        self.size_px = clamp_panel_size(size_px, stage_axis_px);
    }
}

/// Clamp panel width/height in logical pixels before `--ui` scaling.
/// Window-level DevTools UI state (not per-tab).
#[derive(Debug, Clone, Default)]
pub struct DevToolsSession {
    pub state: DevToolsState,
}

impl DevToolsSession {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            state: DevToolsState::from_settings(settings.devtools_dock, settings.devtools_size_px),
        }
    }

    pub fn sync_chrome(&self, doc: &mut BaseDocument) {
        apply_devtools_state(doc, &self.state);
    }

    pub fn toggle(&mut self, doc: &mut BaseDocument) {
        self.state.toggle_open();
        self.sync_chrome(doc);
        if self.state.open {
            focus_devtools_panel(doc);
        }
    }

    pub fn close(&mut self, doc: &mut BaseDocument) {
        if self.state.open {
            self.state.close();
            self.sync_chrome(doc);
        }
    }

    pub fn set_dock(&mut self, dock: DevToolsDock, doc: &mut BaseDocument) {
        self.state.set_dock(dock);
        if self.state.open {
            if let Some(stage) = stage_layout_axis_px(doc, self.state.dock) {
                self.state
                    .resize_clamped(self.state.size_px, stage - SPLITTER_PX);
            }
            self.sync_chrome(doc);
        }
    }

    pub fn set_tab(&mut self, tab: DevToolsTab, doc: &mut BaseDocument) {
        self.state.set_tab(tab);
        if self.state.open {
            self.sync_chrome(doc);
        }
    }

    pub fn resize_from_drag(&mut self, doc: &mut BaseDocument, size_px: i32) -> Option<i32> {
        if !self.state.open {
            return None;
        }
        let stage = stage_layout_axis_px(doc, self.state.dock)?;
        self.state
            .resize_clamped(size_px, stage.saturating_sub(SPLITTER_PX));
        self.sync_chrome(doc);
        Some(self.state.size_px)
    }

    pub fn settings_fields(&self) -> (DevToolsDock, i32) {
        (self.state.dock, self.state.size_px)
    }
}

pub fn clamp_panel_size(size_px: i32, stage_axis_px: i32) -> i32 {
    let stage_axis_px = stage_axis_px.max(MIN_PANEL_PX);
    let max_px = ((stage_axis_px as f32) * MAX_PANEL_RATIO).floor() as i32;
    let max_px = max_px.max(MIN_PANEL_PX);
    size_px.clamp(MIN_PANEL_PX, max_px)
}

#[cfg(test)]
mod tests {
    use super::*;

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
