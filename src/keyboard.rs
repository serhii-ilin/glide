//! Keyboard routing for Glide: browser shortcuts, macOS text-editing keybindings,
//! and forwarding keys into the page `<iframe>` when it has focus.
//!
//! Blitz deliberately handles `Backspace` only through Apple's standard keybinding
//! callbacks on macOS (see `blitz-dom` `TextInputData::apply_keypress_event`). Some
//! embedder setups never receive those callbacks reliably, and keyboard events are not
//! forwarded into iframe sub-documents. Glide closes both gaps here.

use blitz_dom::BaseDocument;
use blitz_shell::View;
#[cfg(target_os = "macos")]
use blitz_traits::SmolStr;
use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
use keyboard_types::{Code, Key, Modifiers, NamedKey};
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::keyboard::ModifiersState;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::chrome::NavCommand;
use anyrender::WindowRenderer;

/// High-level browser shortcuts Glide handles before Blitz sees the key event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserShortcut {
    Back,
    Forward,
    Reload,
    FocusAddressBar,
    CloseWindow,
    Quit,
    Scroll(ScrollAction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAction {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

/// Classify a pressed key (with modifiers) as a browser-level shortcut.
///
/// Text inputs keep typing keys; shortcuts that would steal editor commands (copy,
/// paste, select-all, undo) are not intercepted here—Blitz handles those.
pub fn browser_shortcut(
    code: KeyCode,
    modifiers: ModifiersState,
    text_input_focused: bool,
) -> Option<BrowserShortcut> {
    let primary = modifiers.control_key() || modifiers.meta_key();
    let alt = modifiers.alt_key();
    let shift = modifiers.shift_key();

    match code {
        KeyCode::KeyL if primary && !shift => return Some(BrowserShortcut::FocusAddressBar),
        KeyCode::KeyR if primary && !shift => return Some(BrowserShortcut::Reload),
        KeyCode::KeyW if primary && !shift => return Some(BrowserShortcut::CloseWindow),
        KeyCode::KeyQ if primary && !shift => return Some(BrowserShortcut::Quit),
        KeyCode::BracketLeft if primary && !shift => return Some(BrowserShortcut::Back),
        KeyCode::BracketRight if primary && !shift => return Some(BrowserShortcut::Forward),
        KeyCode::F5 if !primary && !alt => return Some(BrowserShortcut::Reload),
        KeyCode::ArrowLeft if alt && !primary => return Some(BrowserShortcut::Back),
        KeyCode::ArrowRight if alt && !primary => return Some(BrowserShortcut::Forward),
        _ if text_input_focused => return None,
        KeyCode::Space if !primary && !alt && shift => {
            return Some(BrowserShortcut::Scroll(ScrollAction::PageUp));
        }
        KeyCode::Space if !primary && !alt && !shift => {
            return Some(BrowserShortcut::Scroll(ScrollAction::Down));
        }
        KeyCode::PageUp if !primary => return Some(BrowserShortcut::Scroll(ScrollAction::PageUp)),
        KeyCode::PageDown if !primary => {
            return Some(BrowserShortcut::Scroll(ScrollAction::PageDown));
        }
        KeyCode::Home if !primary => return Some(BrowserShortcut::Scroll(ScrollAction::Home)),
        KeyCode::End if !primary => return Some(BrowserShortcut::Scroll(ScrollAction::End)),
        KeyCode::ArrowUp if !primary && !alt => {
            return Some(BrowserShortcut::Scroll(ScrollAction::Up));
        }
        KeyCode::ArrowDown if !primary && !alt => {
            return Some(BrowserShortcut::Scroll(ScrollAction::Down));
        }
        _ => {}
    }
    None
}

#[cfg(target_os = "macos")]
pub fn macos_standard_keybinding(code: KeyCode, modifiers: ModifiersState) -> Option<&'static str> {
    if modifiers.control_key() || modifiers.meta_key() {
        match code {
            KeyCode::KeyZ if !modifiers.shift_key() => return None, // undo not implemented in Blitz
            KeyCode::KeyA | KeyCode::KeyC | KeyCode::KeyV | KeyCode::KeyX
                if !modifiers.shift_key() =>
            {
                return None; // clipboard / select-all via KeyDown in Blitz
            }
            KeyCode::ArrowLeft if modifiers.shift_key() => {
                return Some("moveToBeginningOfLineAndModifySelection:");
            }
            KeyCode::ArrowLeft if !modifiers.shift_key() => return Some("moveToBeginningOfLine:"),
            KeyCode::ArrowRight if modifiers.shift_key() => {
                return Some("moveToEndOfLineAndModifySelection:");
            }
            KeyCode::ArrowRight if !modifiers.shift_key() => return Some("moveToEndOfLine:"),
            _ => {}
        }
    }

    if modifiers.alt_key() && !modifiers.control_key() && !modifiers.meta_key() {
        match code {
            KeyCode::ArrowLeft if modifiers.shift_key() => {
                return Some("moveWordBackwardAndModifySelection:");
            }
            KeyCode::ArrowLeft if !modifiers.shift_key() => return Some("moveWordBackward:"),
            KeyCode::ArrowRight if modifiers.shift_key() => {
                return Some("moveWordForwardAndModifySelection:");
            }
            KeyCode::ArrowRight if !modifiers.shift_key() => return Some("moveWordForward:"),
            KeyCode::Backspace if !modifiers.shift_key() => return Some("deleteWordBackward:"),
            KeyCode::Delete if !modifiers.shift_key() => return Some("deleteWordForward:"),
            _ => {}
        }
    }

    if modifiers.shift_key()
        && !modifiers.control_key()
        && !modifiers.meta_key()
        && !modifiers.alt_key()
    {
        match code {
            KeyCode::ArrowLeft => return Some("moveLeftAndModifySelection:"),
            KeyCode::ArrowRight => return Some("moveRightAndModifySelection:"),
            KeyCode::ArrowUp => return Some("moveUpAndModifySelection:"),
            KeyCode::ArrowDown => return Some("moveDownAndModifySelection:"),
            _ => {}
        }
    }

    if !modifiers.control_key()
        && !modifiers.meta_key()
        && !modifiers.alt_key()
        && !modifiers.shift_key()
    {
        match code {
            KeyCode::Backspace => return Some("deleteBackward:"),
            KeyCode::Delete => return Some("deleteForward:"),
            KeyCode::ArrowLeft => return Some("moveLeft:"),
            KeyCode::ArrowRight => return Some("moveRight:"),
            KeyCode::ArrowUp => return Some("moveUp:"),
            KeyCode::ArrowDown => return Some("moveDown:"),
            KeyCode::Home => return Some("moveToBeginningOfLine:"),
            KeyCode::End => return Some("moveToEndOfLine:"),
            KeyCode::Enter | KeyCode::NumpadEnter => return Some("insertNewline:"),
            _ => {}
        }
    }

    None
}

fn winit_modifiers_to_kbt(modifiers: ModifiersState) -> Modifiers {
    let mut out = Modifiers::default();
    if modifiers.control_key() {
        out.insert(Modifiers::CONTROL);
    }
    if modifiers.alt_key() {
        out.insert(Modifiers::ALT);
    }
    if modifiers.shift_key() {
        out.insert(Modifiers::SHIFT);
    }
    if modifiers.meta_key() {
        out.insert(Modifiers::META);
    }
    out
}

fn winit_key_to_blitz(event: &KeyEvent, modifiers: ModifiersState) -> BlitzKeyEvent {
    let physical_code = match event.physical_key {
        PhysicalKey::Code(code) => code,
        PhysicalKey::Unidentified(_) => Code::Unidentified,
    };
    let logical_key = match &event.logical_key {
        winit::keyboard::Key::Named(n) => Key::Named(*n),
        winit::keyboard::Key::Character(c) => Key::Character(c.to_string()),
        winit::keyboard::Key::Unidentified(_) => Key::Named(NamedKey::Unidentified),
        winit::keyboard::Key::Dead(_) => Key::Named(NamedKey::Dead),
    };
    BlitzKeyEvent {
        key: logical_key,
        code: physical_code,
        modifiers: winit_modifiers_to_kbt(modifiers),
        location: event.location,
        is_auto_repeating: event.repeat,
        is_composing: false,
        state: if event.state.is_pressed() {
            KeyState::Pressed
        } else {
            KeyState::Released
        },
        text: event.text.clone(),
    }
}

fn node_is_text_input(doc: &BaseDocument, node_id: blitz_traits::node_id::NodeId) -> bool {
    doc.get_node(node_id)
        .and_then(|node| node.element_data())
        .is_some_and(|el| el.text_input_data().is_some())
}

/// Whether a text control in the chrome or page iframe currently has focus.
pub fn text_input_focused(chrome: &BaseDocument) -> bool {
    if chrome
        .get_focussed_node_id()
        .is_some_and(|id| node_is_text_input(chrome, id))
    {
        return true;
    }
    page_iframe_id(chrome).is_some_and(|page_id| {
        page_iframe_accepts_keys(chrome) && page_subdoc_has_text_focus(chrome, page_id)
    })
}

fn page_iframe_id(chrome: &BaseDocument) -> Option<blitz_traits::node_id::NodeId> {
    chrome.get_element_by_id("page")
}

fn page_subdoc_has_text_focus(
    chrome: &BaseDocument,
    page_id: blitz_traits::node_id::NodeId,
) -> bool {
    let Some(sub) = chrome.get_node(page_id).and_then(|n| n.subdoc()) else {
        return false;
    };
    let inner = sub.inner();
    inner
        .get_focussed_node_id()
        .is_some_and(|id| node_is_text_input(&inner, id))
}

fn page_iframe_accepts_keys(chrome: &BaseDocument) -> bool {
    page_iframe_id(chrome).is_some_and(|page_id| chrome.get_focussed_node_id() == Some(page_id))
}

/// Deliver editing/viewport keyboard handling before Blitz's default `KeyboardInput` path.
///
/// Returns `true` when the caller should skip the default winit keyboard handler for this event.
pub fn dispatch_text_keys<Rend: WindowRenderer>(view: &mut View<Rend>, event: &KeyEvent) -> bool {
    let modifiers = view.keyboard_modifiers.state();

    let iframe_handled = {
        let mut guard = view.doc.inner_mut();
        let chrome = &mut *guard;
        if !page_iframe_accepts_keys(chrome) {
            None
        } else if let PhysicalKey::Code(code) = event.physical_key {
            if event.state.is_pressed() && browser_shortcut(code, modifiers, false).is_some() {
                Some(false)
            } else if let Some(page_id) = page_iframe_id(chrome)
                && let Some(sub) = chrome
                    .get_node_mut(page_id)
                    .and_then(|node| node.subdoc_mut())
            {
                let mut handled = false;
                #[cfg(target_os = "macos")]
                if event.state.is_pressed()
                    && let PhysicalKey::Code(code) = event.physical_key
                    && let Some(selector) = macos_standard_keybinding(code, modifiers)
                {
                    sub.handle_ui_event(UiEvent::AppleStandardKeybinding(SmolStr::new(selector)));
                    handled = true;
                }
                if !handled {
                    let blitz_key = winit_key_to_blitz(event, modifiers);
                    let ui = if event.state.is_pressed() {
                        UiEvent::KeyDown(blitz_key)
                    } else {
                        UiEvent::KeyUp(blitz_key)
                    };
                    sub.handle_ui_event(ui);
                }
                Some(true)
            } else {
                Some(false)
            }
        } else {
            None
        }
    };

    if let Some(true) = iframe_handled {
        return true;
    }
    if matches!(iframe_handled, Some(false)) {
        return false;
    }

    #[cfg(target_os = "macos")]
    {
        let selector = {
            let mut guard = view.doc.inner_mut();
            let chrome = &mut *guard;
            if !event.state.is_pressed() {
                return false;
            }
            let PhysicalKey::Code(code) = event.physical_key else {
                return false;
            };
            let focused_input = chrome
                .get_focussed_node_id()
                .is_some_and(|id| node_is_text_input(chrome, id));
            if !focused_input {
                return false;
            }
            macos_standard_keybinding(code, modifiers).map(SmolStr::new)
        };
        if let Some(selector) = selector {
            view.doc
                .handle_ui_event(UiEvent::AppleStandardKeybinding(selector));
            return true;
        }
    }

    false
}

pub fn scroll_page<Rend: WindowRenderer>(view: &mut View<Rend>, action: ScrollAction) -> bool {
    let mut chrome = view.doc.inner_mut();
    let chrome = &mut *chrome;
    let Some(page_id) = page_iframe_id(chrome) else {
        return scroll_chrome_viewport(chrome, action);
    };
    let Some(sub) = chrome
        .get_node_mut(page_id)
        .and_then(|node| node.subdoc_mut())
    else {
        return scroll_chrome_viewport(chrome, action);
    };
    let mut inner = sub.inner_mut();
    let doc = &mut *inner;
    let page_h = doc.viewport().window_size.1 as f64;
    let step = (page_h * 0.9).max(120.0);
    let sy = match action {
        ScrollAction::Up => -step,
        ScrollAction::Down => step,
        ScrollAction::PageUp => -step,
        ScrollAction::PageDown => step,
        ScrollAction::Home | ScrollAction::End => 0.0,
    };
    if matches!(action, ScrollAction::Home) {
        doc.scroll_viewport_by(0.0, -doc.viewport_scroll().y);
    } else if matches!(action, ScrollAction::End) {
        doc.scroll_viewport_by(0.0, 10_000_000.0);
    } else {
        doc.scroll_viewport_by(0.0, sy);
    }
    true
}

fn scroll_chrome_viewport(chrome: &mut BaseDocument, action: ScrollAction) -> bool {
    let page_h = chrome.viewport().window_size.1 as f64;
    let step = (page_h * 0.9).max(120.0);
    if matches!(action, ScrollAction::Home) {
        chrome.scroll_viewport_by(0.0, -chrome.viewport_scroll().y);
        return true;
    }
    if matches!(action, ScrollAction::End) {
        chrome.scroll_viewport_by(0.0, 10_000_000.0);
        return true;
    }
    let sy = match action {
        ScrollAction::Up => -step,
        ScrollAction::Down => step,
        ScrollAction::PageUp => -step,
        ScrollAction::PageDown => step,
        ScrollAction::Home | ScrollAction::End => 0.0,
    };
    chrome.scroll_viewport_by(0.0, sy);
    true
}

pub fn nav_from_shortcut(shortcut: BrowserShortcut) -> Option<NavCommand> {
    match shortcut {
        BrowserShortcut::Back => Some(NavCommand::Back),
        BrowserShortcut::Forward => Some(NavCommand::Forward),
        BrowserShortcut::Reload => Some(NavCommand::Reload),
        BrowserShortcut::FocusAddressBar | BrowserShortcut::CloseWindow | BrowserShortcut::Quit => {
            None
        }
        BrowserShortcut::Scroll(_) => None,
    }
}

pub fn try_browser_shortcut(
    view: &mut View<impl WindowRenderer>,
    event: &WindowEvent,
) -> Option<BrowserShortcut> {
    let WindowEvent::KeyboardInput { event, .. } = event else {
        return None;
    };
    if event.state != ElementState::Pressed {
        return None;
    }
    let PhysicalKey::Code(code) = event.physical_key else {
        return None;
    };
    let chrome = view.doc.inner();
    let text_focused = text_input_focused(&chrome);
    browser_shortcut(code, view.keyboard_modifiers.state(), text_focused)
}

/// Focus the chrome address field (`#url`).
pub fn focus_address_bar<Rend: WindowRenderer>(view: &mut View<Rend>) {
    let mut doc = view.doc.inner_mut();
    if let Some(id) = doc.get_element_by_id("url") {
        doc.set_focus_to(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::ModifiersState;

    fn mods(ctrl: bool, alt: bool, shift: bool, meta: bool) -> ModifiersState {
        let mut m = ModifiersState::empty();
        if ctrl {
            m |= ModifiersState::CONTROL;
        }
        if alt {
            m |= ModifiersState::ALT;
        }
        if shift {
            m |= ModifiersState::SHIFT;
        }
        if meta {
            m |= ModifiersState::META;
        }
        m
    }

    #[test]
    fn browser_chrome_shortcuts_work_in_text_fields() {
        assert_eq!(
            browser_shortcut(KeyCode::KeyL, mods(true, false, false, false), true),
            Some(BrowserShortcut::FocusAddressBar)
        );
        assert!(browser_shortcut(KeyCode::Space, ModifiersState::empty(), true).is_none());
    }

    #[test]
    fn reload_shortcut() {
        assert_eq!(
            browser_shortcut(KeyCode::KeyR, mods(true, false, false, false), false),
            Some(BrowserShortcut::Reload)
        );
        assert_eq!(
            browser_shortcut(KeyCode::F5, ModifiersState::empty(), false),
            Some(BrowserShortcut::Reload)
        );
    }

    #[test]
    fn history_shortcuts() {
        #[cfg(target_os = "macos")]
        let primary = mods(false, false, false, true);
        #[cfg(not(target_os = "macos"))]
        let primary = mods(true, false, false, false);

        assert_eq!(
            browser_shortcut(KeyCode::BracketLeft, primary, false),
            Some(BrowserShortcut::Back)
        );
        assert_eq!(
            browser_shortcut(KeyCode::ArrowLeft, mods(false, true, false, false), false),
            Some(BrowserShortcut::Back)
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_backspace_deletes_backward() {
        assert_eq!(
            macos_standard_keybinding(KeyCode::Backspace, ModifiersState::empty()),
            Some("deleteBackward:")
        );
        assert_eq!(
            macos_standard_keybinding(KeyCode::Delete, ModifiersState::empty()),
            Some("deleteForward:")
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_option_arrows_move_by_word() {
        assert_eq!(
            macos_standard_keybinding(KeyCode::ArrowLeft, mods(false, true, false, false)),
            Some("moveWordBackward:")
        );
    }
}
