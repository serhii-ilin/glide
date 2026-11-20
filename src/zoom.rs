//! Page zoom level: step size, clamping, and chrome label text.

use blitz_dom::BaseDocument;

const MIN_ZOOM: f32 = 0.3;
const MAX_ZOOM: f32 = 3.0;
const STEP: f32 = 0.1;

/// Per-window page zoom (not persisted across restarts).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageZoom {
    level: f32,
}

impl Default for PageZoom {
    fn default() -> Self {
        Self::new()
    }
}

impl PageZoom {
    pub fn new() -> Self {
        Self { level: 1.0 }
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    pub fn zoom_in(&mut self) {
        self.set_level(self.level + STEP);
    }

    pub fn zoom_out(&mut self) {
        self.set_level(self.level - STEP);
    }

    pub fn reset(&mut self) {
        self.level = 1.0;
    }

    fn set_level(&mut self, level: f32) {
        self.level = level.clamp(MIN_ZOOM, MAX_ZOOM);
    }

    /// Counter-scale factor for chrome UI so the toolbar stays the same visual size.
    pub fn ui_inverse_scale(&self) -> f32 {
        1.0 / self.level
    }

    pub fn apply_to_chrome(&self, chrome: &mut BaseDocument) {
        {
            let mut vp = chrome.viewport_mut();
            vp.set_zoom(self.level);
        }
        crate::chrome::set_ui_scale(chrome, self.ui_inverse_scale());
        crate::chrome::set_zoom_label(chrome, self.label().as_deref());
    }

    /// Percent label for the chrome indicator; hidden at 100%.
    pub fn label(&self) -> Option<String> {
        let pct = (self.level * 100.0).round() as i32;
        if pct == 100 {
            None
        } else {
            Some(format!("{pct}%"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_in_steps_by_ten_percent() {
        let mut z = PageZoom::new();
        z.zoom_in();
        assert!((z.level() - 1.1).abs() < f32::EPSILON);
        z.zoom_in();
        assert!((z.level() - 1.2).abs() < f32::EPSILON);
    }

    #[test]
    fn zoom_clamps_at_bounds() {
        let mut z = PageZoom::new();
        for _ in 0..50 {
            z.zoom_out();
        }
        assert!((z.level() - MIN_ZOOM).abs() < f32::EPSILON);
        for _ in 0..50 {
            z.zoom_in();
        }
        assert!((z.level() - MAX_ZOOM).abs() < f32::EPSILON);
    }

    #[test]
    fn reset_returns_to_one() {
        let mut z = PageZoom::new();
        z.zoom_in();
        z.zoom_in();
        z.reset();
        assert!((z.level() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn label_hidden_at_100() {
        let z = PageZoom::new();
        assert!(z.label().is_none());
        let mut z2 = PageZoom::new();
        z2.zoom_in();
        assert_eq!(z2.label().as_deref(), Some("110%"));
    }
}
