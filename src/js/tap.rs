//! Record UI events from page documents for dispatch into V8.

use blitz_traits::events::UiEvent;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct JsTap {
    enabled: bool,
    pending: Arc<Mutex<Vec<UiEvent>>>,
}

impl JsTap {
    pub fn new_enabled() -> Self {
        Self {
            enabled: true,
            pending: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn new_disabled() -> Self {
        Self::default()
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn record_ui_event(&self, event: &UiEvent) {
        if !self.enabled {
            return;
        }
        self.pending.lock().expect("js tap").push(event.clone());
    }

    pub fn drain(&self) -> Vec<UiEvent> {
        if !self.enabled {
            return Vec::new();
        }
        std::mem::take(&mut *self.pending.lock().expect("js tap"))
    }
}