//! Record UI events from page documents for dispatch into V8.

use blitz_traits::events::UiEvent;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct JsTap {
    enabled: Arc<AtomicBool>,
    pending: Arc<Mutex<Vec<UiEvent>>>,
}

impl JsTap {
    pub fn new_enabled() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
            pending: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn new_disabled() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(false)),
            pending: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn record_ui_event(&self, event: &UiEvent) {
        if !self.enabled() {
            return;
        }
        self.pending.lock().expect("js tap").push(event.clone());
    }

    pub fn drain(&self) -> Vec<UiEvent> {
        if !self.enabled() {
            return Vec::new();
        }
        std::mem::take(&mut *self.pending.lock().expect("js tap"))
    }
}
