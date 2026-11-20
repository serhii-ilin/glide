//! Page zoom level: step size, clamping, and chrome label text.

use blitz_dom::BaseDocument;

const MIN_PERCENT: i32 = 30;
const MAX_PERCENT: i32 = 300;
const STEP_PERCENT: i32 = 10;

/// Per-window page zoom (not persisted across restarts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageZoom {
    percent: i32,
}

impl Default for PageZoom {
    fn default() -> Self {
        Self::new()
    }
}

impl PageZoom {
    pub fn new() -> Self {
        Self { percent: 100 }
    }

    pub fn level(&self) -> f32 {
        self.percent as f32 / 100.0
    }

    pub fn zoom_in(&mut self) {
        self.set_percent(self.percent + STEP_PERCENT);
    }

    pub fn zoom_out(&mut self) {
        self.set_percent(self.percent - STEP_PERCENT);
    }

    pub fn reset(&mut self) {
        self.percent = 100;
    }

    fn set_percent(&mut self, percent: i32) {
        self.percent = percent.clamp(MIN_PERCENT, MAX_PERCENT);
    }

    /// Counter-scale factor for chrome UI so the toolbar stays the same visual size.
    pub fn ui_inverse_scale(&self) -> f32 {
        1.0 / self.level()
    }

    pub fn apply_to_chrome(&self, chrome: &mut BaseDocument) {
        let level = self.level();
        {
            let mut vp = chrome.viewport_mut();
            vp.set_zoom(level);
        }
        crate::chrome::set_ui_scale(chrome, self.ui_inverse_scale());
        crate::chrome::set_zoom_label(chrome, self.label().as_deref());
    }

    /// Percent label for the chrome indicator; hidden at 100%.
    pub fn label(&self) -> Option<String> {
        if self.percent == 100 {
            None
        } else {
            Some(format!("{}%", self.percent))
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
        assert_eq!(z.percent, 110);
        z.zoom_in();
        assert_eq!(z.percent, 120);
    }

    #[test]
    fn zoom_clamps_at_bounds() {
        let mut z = PageZoom::new();
        for _ in 0..50 {
            z.zoom_out();
        }
        assert_eq!(z.percent, MIN_PERCENT);
        for _ in 0..50 {
            z.zoom_in();
        }
        assert_eq!(z.percent, MAX_PERCENT);
    }

    #[test]
    fn reset_returns_to_one() {
        let mut z = PageZoom::new();
        z.zoom_in();
        z.zoom_in();
        z.reset();
        assert_eq!(z.percent, 100);
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
