//! Keyboard routing for Glide: browser shortcuts, macOS text-editing keybindings,
//! and forwarding keys into the page `<iframe>` when it has focus.
//!
//! Blitz deliberately handles `Backspace` only through Apple's standard keybinding
//! callbacks on macOS (see `blitz-dom` `TextInputData::apply_keypress_event`). Some
//! embedder setups never receive those callbacks reliably, and keyboard events are not
//! forwarded into iframe sub-documents. Glide closes both gaps here.

use blitz_dom::{BaseDocument, Document};
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

/// Forward a key into the page iframe when it has focus.
///
/// Returns `Some(true)` when the iframe consumed the event, `Some(false)` when the iframe
/// is focused but the key is a browser shortcut, and `None` when the iframe is not the
/// keyboard target.
pub(crate) fn forward_page_iframe_key(
    chrome: &mut BaseDocument,
    code: KeyCode,
    modifiers: ModifiersState,
    pressed: bool,
    blitz_key: BlitzKeyEvent,
) -> Option<bool> {
    if !page_iframe_accepts_keys(chrome) {
        return None;
    }
    if pressed && browser_shortcut(code, modifiers, false).is_some() {
        return Some(false);
    }
    let Some(page_id) = page_iframe_id(chrome) else {
        return Some(false);
    };
    let Some(sub) = chrome
        .get_node_mut(page_id)
        .and_then(|node| node.subdoc_mut())
    else {
        return Some(false);
    };
    #[cfg(target_os = "macos")]
    if pressed && let Some(selector) = macos_standard_keybinding(code, modifiers) {
        sub.handle_ui_event(UiEvent::AppleStandardKeybinding(SmolStr::new(selector)));
        return Some(true);
    }
    let ui = if pressed {
        UiEvent::KeyDown(blitz_key)
    } else {
        UiEvent::KeyUp(blitz_key)
    };
    sub.handle_ui_event(ui);
    Some(true)
}

/// Apply a macOS standard keybinding to the focused chrome text control, if any.
#[cfg(target_os = "macos")]
pub(crate) fn apply_macos_chrome_keybinding(
    chrome: &mut BaseDocument,
    code: KeyCode,
    modifiers: ModifiersState,
) -> bool {
    let focused_input = chrome
        .get_focussed_node_id()
        .is_some_and(|id| node_is_text_input(chrome, id));
    if !focused_input {
        return false;
    }
    let Some(selector) = macos_standard_keybinding(code, modifiers) else {
        return false;
    };
    chrome.handle_ui_event(UiEvent::AppleStandardKeybinding(SmolStr::new(selector)));
    true
}

/// Deliver editing/viewport keyboard handling before Blitz's default `KeyboardInput` path.
///
/// Returns `true` when the caller should skip the default winit keyboard handler for this event.
pub fn dispatch_text_keys<Rend: WindowRenderer>(view: &mut View<Rend>, event: &KeyEvent) -> bool {
    let modifiers = view.keyboard_modifiers.state();

    let iframe_handled = {
        let mut guard = view.doc.inner_mut();
        let chrome = &mut *guard;
        if let PhysicalKey::Code(code) = event.physical_key {
            let blitz_key = winit_key_to_blitz(event, modifiers);
            forward_page_iframe_key(chrome, code, modifiers, event.state.is_pressed(), blitz_key)
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

    if !event.state.is_pressed() {
        return false;
    }
    let PhysicalKey::Code(code) = event.physical_key else {
        return false;
    };
    #[cfg(target_os = "macos")]
    {
        let mut guard = view.doc.inner_mut();
        let chrome = &mut *guard;
        apply_macos_chrome_keybinding(chrome, code, modifiers)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (view, code, modifiers);
        false
    }
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
    use std::sync::Arc;

    use blitz_dom::{Document, DocumentConfig, qual_name};
    use blitz_traits::shell::{ColorScheme, Viewport};
    use keyboard_types::{Code, Key, Modifiers, NamedKey};
    use winit::keyboard::ModifiersState;

    use crate::chrome::{NavCommand, chrome_document};
    use crate::html::GlideHtmlProvider;
    use crate::parse;

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

    fn primary_mods() -> ModifiersState {
        #[cfg(target_os = "macos")]
        {
            mods(false, false, false, true)
        }
        #[cfg(not(target_os = "macos"))]
        {
            mods(true, false, false, false)
        }
    }

    struct ShortcutCase {
        row: &'static str,
        code: KeyCode,
        modifiers: ModifiersState,
        text_focused: bool,
        expected: Option<BrowserShortcut>,
    }

    fn readme_shortcut_cases() -> Vec<ShortcutCase> {
        let primary = primary_mods();
        let alt = mods(false, true, false, false);
        let shift_only = mods(false, false, true, false);
        let none = ModifiersState::empty();

        let mut cases = vec![
            ShortcutCase {
                row: "Enter (not a browser shortcut)",
                code: KeyCode::Enter,
                modifiers: none,
                text_focused: false,
                expected: None,
            },
            ShortcutCase {
                row: "Enter while typing",
                code: KeyCode::Enter,
                modifiers: none,
                text_focused: true,
                expected: None,
            },
            ShortcutCase {
                row: "Alt+Left back",
                code: KeyCode::ArrowLeft,
                modifiers: alt,
                text_focused: false,
                expected: Some(BrowserShortcut::Back),
            },
            ShortcutCase {
                row: "Alt+Left back in text field",
                code: KeyCode::ArrowLeft,
                modifiers: alt,
                text_focused: true,
                expected: Some(BrowserShortcut::Back),
            },
            ShortcutCase {
                row: "Alt+Right forward",
                code: KeyCode::ArrowRight,
                modifiers: alt,
                text_focused: false,
                expected: Some(BrowserShortcut::Forward),
            },
            ShortcutCase {
                row: "Alt+Right forward in text field",
                code: KeyCode::ArrowRight,
                modifiers: alt,
                text_focused: true,
                expected: Some(BrowserShortcut::Forward),
            },
            ShortcutCase {
                row: "Cmd/Ctrl+[ back",
                code: KeyCode::BracketLeft,
                modifiers: primary,
                text_focused: false,
                expected: Some(BrowserShortcut::Back),
            },
            ShortcutCase {
                row: "Cmd/Ctrl+[ back in text field",
                code: KeyCode::BracketLeft,
                modifiers: primary,
                text_focused: true,
                expected: Some(BrowserShortcut::Back),
            },
            ShortcutCase {
                row: "Cmd/Ctrl+] forward",
                code: KeyCode::BracketRight,
                modifiers: primary,
                text_focused: false,
                expected: Some(BrowserShortcut::Forward),
            },
            ShortcutCase {
                row: "Cmd/Ctrl+R reload",
                code: KeyCode::KeyR,
                modifiers: primary,
                text_focused: false,
                expected: Some(BrowserShortcut::Reload),
            },
            ShortcutCase {
                row: "Cmd/Ctrl+R reload in text field",
                code: KeyCode::KeyR,
                modifiers: primary,
                text_focused: true,
                expected: Some(BrowserShortcut::Reload),
            },
            ShortcutCase {
                row: "F5 reload",
                code: KeyCode::F5,
                modifiers: none,
                text_focused: false,
                expected: Some(BrowserShortcut::Reload),
            },
            ShortcutCase {
                row: "F5 reload in text field",
                code: KeyCode::F5,
                modifiers: none,
                text_focused: true,
                expected: Some(BrowserShortcut::Reload),
            },
            ShortcutCase {
                row: "Cmd/Ctrl+L focus address bar",
                code: KeyCode::KeyL,
                modifiers: primary,
                text_focused: false,
                expected: Some(BrowserShortcut::FocusAddressBar),
            },
            ShortcutCase {
                row: "Cmd/Ctrl+L focus address bar in text field",
                code: KeyCode::KeyL,
                modifiers: primary,
                text_focused: true,
                expected: Some(BrowserShortcut::FocusAddressBar),
            },
            ShortcutCase {
                row: "Cmd/Ctrl+W close",
                code: KeyCode::KeyW,
                modifiers: primary,
                text_focused: false,
                expected: Some(BrowserShortcut::CloseWindow),
            },
            ShortcutCase {
                row: "Cmd/Ctrl+W close in text field",
                code: KeyCode::KeyW,
                modifiers: primary,
                text_focused: true,
                expected: Some(BrowserShortcut::CloseWindow),
            },
            ShortcutCase {
                row: "Cmd/Ctrl+Q quit",
                code: KeyCode::KeyQ,
                modifiers: primary,
                text_focused: false,
                expected: Some(BrowserShortcut::Quit),
            },
            ShortcutCase {
                row: "Cmd/Ctrl+Q quit in text field",
                code: KeyCode::KeyQ,
                modifiers: primary,
                text_focused: true,
                expected: Some(BrowserShortcut::Quit),
            },
            ShortcutCase {
                row: "Cmd/Ctrl+Shift+R not intercepted",
                code: KeyCode::KeyR,
                modifiers: primary | ModifiersState::SHIFT,
                text_focused: false,
                expected: None,
            },
            ShortcutCase {
                row: "plain letter not intercepted",
                code: KeyCode::KeyA,
                modifiers: none,
                text_focused: false,
                expected: None,
            },
            ShortcutCase {
                row: "Cmd/Ctrl+C not intercepted",
                code: KeyCode::KeyC,
                modifiers: primary,
                text_focused: true,
                expected: None,
            },
            ShortcutCase {
                row: "Cmd/Ctrl+V not intercepted",
                code: KeyCode::KeyV,
                modifiers: primary,
                text_focused: true,
                expected: None,
            },
            ShortcutCase {
                row: "Cmd/Ctrl+X not intercepted",
                code: KeyCode::KeyX,
                modifiers: primary,
                text_focused: true,
                expected: None,
            },
            ShortcutCase {
                row: "Cmd/Ctrl+A not intercepted",
                code: KeyCode::KeyA,
                modifiers: primary,
                text_focused: true,
                expected: None,
            },
        ];

        let scroll_cases = [
            (
                "Space scroll down",
                KeyCode::Space,
                none,
                BrowserShortcut::Scroll(ScrollAction::Down),
            ),
            (
                "Shift+Space scroll up",
                KeyCode::Space,
                shift_only,
                BrowserShortcut::Scroll(ScrollAction::PageUp),
            ),
            (
                "PgUp",
                KeyCode::PageUp,
                none,
                BrowserShortcut::Scroll(ScrollAction::PageUp),
            ),
            (
                "PgDn",
                KeyCode::PageDown,
                none,
                BrowserShortcut::Scroll(ScrollAction::PageDown),
            ),
            (
                "Home",
                KeyCode::Home,
                none,
                BrowserShortcut::Scroll(ScrollAction::Home),
            ),
            (
                "End",
                KeyCode::End,
                none,
                BrowserShortcut::Scroll(ScrollAction::End),
            ),
            (
                "ArrowUp",
                KeyCode::ArrowUp,
                none,
                BrowserShortcut::Scroll(ScrollAction::Up),
            ),
            (
                "ArrowDown",
                KeyCode::ArrowDown,
                none,
                BrowserShortcut::Scroll(ScrollAction::Down),
            ),
        ];

        for (row, code, modifiers, action) in scroll_cases {
            cases.push(ShortcutCase {
                row,
                code,
                modifiers,
                text_focused: false,
                expected: Some(action),
            });
            cases.push(ShortcutCase {
                row: match row {
                    "Space scroll down" => "Space blocked in text field",
                    "Shift+Space scroll up" => "Shift+Space blocked in text field",
                    "PgUp" => "PgUp blocked in text field",
                    "PgDn" => "PgDn blocked in text field",
                    "Home" => "Home blocked in text field",
                    "End" => "End blocked in text field",
                    "ArrowUp" => "ArrowUp blocked in text field",
                    "ArrowDown" => "ArrowDown blocked in text field",
                    _ => "scroll key blocked in text field",
                },
                code,
                modifiers,
                text_focused: true,
                expected: None,
            });
        }

        cases
    }

    #[test]
    fn readme_shortcut_table() {
        for case in readme_shortcut_cases() {
            let got = browser_shortcut(case.code, case.modifiers, case.text_focused);
            assert_eq!(
                got, case.expected,
                "README shortcut row failed: {}",
                case.row
            );
        }
    }

    #[test]
    fn nav_from_shortcut_maps_history() {
        assert_eq!(
            nav_from_shortcut(BrowserShortcut::Back),
            Some(NavCommand::Back)
        );
        assert_eq!(
            nav_from_shortcut(BrowserShortcut::Reload),
            Some(NavCommand::Reload)
        );
        assert!(nav_from_shortcut(BrowserShortcut::FocusAddressBar).is_none());
    }

    #[cfg(target_os = "macos")]
    struct MacKeybindingCase {
        row: &'static str,
        code: KeyCode,
        modifiers: ModifiersState,
        expected: Option<&'static str>,
    }

    #[cfg(target_os = "macos")]
    fn macos_keybinding_cases() -> Vec<MacKeybindingCase> {
        let none = ModifiersState::empty();
        let shift = mods(false, false, true, false);
        let option = mods(false, true, false, false);
        let cmd = mods(false, false, false, true);
        let cmd_shift = cmd | shift;
        let option_shift = option | shift;

        vec![
            MacKeybindingCase {
                row: "plain Backspace",
                code: KeyCode::Backspace,
                modifiers: none,
                expected: Some("deleteBackward:"),
            },
            MacKeybindingCase {
                row: "plain Delete",
                code: KeyCode::Delete,
                modifiers: none,
                expected: Some("deleteForward:"),
            },
            MacKeybindingCase {
                row: "plain ArrowLeft",
                code: KeyCode::ArrowLeft,
                modifiers: none,
                expected: Some("moveLeft:"),
            },
            MacKeybindingCase {
                row: "plain ArrowRight",
                code: KeyCode::ArrowRight,
                modifiers: none,
                expected: Some("moveRight:"),
            },
            MacKeybindingCase {
                row: "plain ArrowUp",
                code: KeyCode::ArrowUp,
                modifiers: none,
                expected: Some("moveUp:"),
            },
            MacKeybindingCase {
                row: "plain ArrowDown",
                code: KeyCode::ArrowDown,
                modifiers: none,
                expected: Some("moveDown:"),
            },
            MacKeybindingCase {
                row: "plain Home",
                code: KeyCode::Home,
                modifiers: none,
                expected: Some("moveToBeginningOfLine:"),
            },
            MacKeybindingCase {
                row: "plain End",
                code: KeyCode::End,
                modifiers: none,
                expected: Some("moveToEndOfLine:"),
            },
            MacKeybindingCase {
                row: "plain Enter",
                code: KeyCode::Enter,
                modifiers: none,
                expected: Some("insertNewline:"),
            },
            MacKeybindingCase {
                row: "plain NumpadEnter",
                code: KeyCode::NumpadEnter,
                modifiers: none,
                expected: Some("insertNewline:"),
            },
            MacKeybindingCase {
                row: "Shift+ArrowLeft",
                code: KeyCode::ArrowLeft,
                modifiers: shift,
                expected: Some("moveLeftAndModifySelection:"),
            },
            MacKeybindingCase {
                row: "Shift+ArrowRight",
                code: KeyCode::ArrowRight,
                modifiers: shift,
                expected: Some("moveRightAndModifySelection:"),
            },
            MacKeybindingCase {
                row: "Shift+ArrowUp",
                code: KeyCode::ArrowUp,
                modifiers: shift,
                expected: Some("moveUpAndModifySelection:"),
            },
            MacKeybindingCase {
                row: "Shift+ArrowDown",
                code: KeyCode::ArrowDown,
                modifiers: shift,
                expected: Some("moveDownAndModifySelection:"),
            },
            MacKeybindingCase {
                row: "Option+ArrowLeft",
                code: KeyCode::ArrowLeft,
                modifiers: option,
                expected: Some("moveWordBackward:"),
            },
            MacKeybindingCase {
                row: "Option+ArrowRight",
                code: KeyCode::ArrowRight,
                modifiers: option,
                expected: Some("moveWordForward:"),
            },
            MacKeybindingCase {
                row: "Option+Shift+ArrowLeft",
                code: KeyCode::ArrowLeft,
                modifiers: option_shift,
                expected: Some("moveWordBackwardAndModifySelection:"),
            },
            MacKeybindingCase {
                row: "Option+Shift+ArrowRight",
                code: KeyCode::ArrowRight,
                modifiers: option_shift,
                expected: Some("moveWordForwardAndModifySelection:"),
            },
            MacKeybindingCase {
                row: "Option+Backspace",
                code: KeyCode::Backspace,
                modifiers: option,
                expected: Some("deleteWordBackward:"),
            },
            MacKeybindingCase {
                row: "Option+Delete",
                code: KeyCode::Delete,
                modifiers: option,
                expected: Some("deleteWordForward:"),
            },
            MacKeybindingCase {
                row: "Cmd+ArrowLeft",
                code: KeyCode::ArrowLeft,
                modifiers: cmd,
                expected: Some("moveToBeginningOfLine:"),
            },
            MacKeybindingCase {
                row: "Cmd+ArrowRight",
                code: KeyCode::ArrowRight,
                modifiers: cmd,
                expected: Some("moveToEndOfLine:"),
            },
            MacKeybindingCase {
                row: "Cmd+Shift+ArrowLeft",
                code: KeyCode::ArrowLeft,
                modifiers: cmd_shift,
                expected: Some("moveToBeginningOfLineAndModifySelection:"),
            },
            MacKeybindingCase {
                row: "Cmd+Shift+ArrowRight",
                code: KeyCode::ArrowRight,
                modifiers: cmd_shift,
                expected: Some("moveToEndOfLineAndModifySelection:"),
            },
            MacKeybindingCase {
                row: "Cmd+C passthrough",
                code: KeyCode::KeyC,
                modifiers: cmd,
                expected: None,
            },
        ]
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_standard_keybinding_table() {
        for case in macos_keybinding_cases() {
            let got = macos_standard_keybinding(case.code, case.modifiers);
            assert_eq!(
                got, case.expected,
                "macOS keybinding row failed: {}",
                case.row
            );
        }
    }

    fn winit_mods_to_blitz(modifiers: ModifiersState) -> Modifiers {
        winit_modifiers_to_kbt(modifiers)
    }

    fn blitz_key(code: KeyCode, modifiers: ModifiersState, key: Key) -> BlitzKeyEvent {
        let physical = match code {
            KeyCode::Backspace => Code::Backspace,
            KeyCode::Delete => Code::Delete,
            KeyCode::ArrowLeft => Code::ArrowLeft,
            KeyCode::ArrowRight => Code::ArrowRight,
            KeyCode::Enter => Code::Enter,
            KeyCode::KeyA => Code::KeyA,
            KeyCode::KeyB => Code::KeyB,
            KeyCode::KeyC => Code::KeyC,
            _ => Code::Unidentified,
        };
        BlitzKeyEvent {
            key,
            code: physical,
            modifiers: winit_mods_to_blitz(modifiers),
            location: keyboard_types::Location::Standard,
            is_auto_repeating: false,
            is_composing: false,
            state: KeyState::Pressed,
            text: None,
        }
    }

    fn text_value_in_doc(doc: &BaseDocument, id: &str) -> Option<String> {
        let node_id = doc.get_element_by_id(id)?;
        let input = doc.get_node(node_id)?.element_data()?.text_input_data()?;
        Some(input.editor.text().to_string())
    }

    fn test_document_config() -> DocumentConfig {
        DocumentConfig {
            viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(GlideHtmlProvider::default())),
            ..DocumentConfig::default()
        }
    }

    fn layout_doc(doc: &mut BaseDocument) {
        doc.resolve(0.0);
    }

    fn chrome_test_document() -> BaseDocument {
        let html = chrome_document("");
        let mut doc = parse::document_from_html(&html, test_document_config(), false);
        layout_doc(&mut doc);
        let url_id = doc.get_element_by_id("url").expect("url input");
        doc.set_focus_to(url_id);
        doc
    }

    fn press_editing_key(doc: &mut BaseDocument, code: KeyCode, modifiers: ModifiersState) {
        #[cfg(target_os = "macos")]
        if apply_macos_chrome_keybinding(doc, code, modifiers) {
            return;
        }
        let key = match code {
            KeyCode::Backspace => Key::Named(NamedKey::Backspace),
            KeyCode::Delete => Key::Named(NamedKey::Delete),
            KeyCode::End => Key::Named(NamedKey::End),
            KeyCode::KeyA => Key::Character("a".into()),
            KeyCode::KeyB => Key::Character("b".into()),
            KeyCode::KeyC => Key::Character("c".into()),
            _ => Key::Named(NamedKey::Unidentified),
        };
        doc.handle_ui_event(UiEvent::KeyDown(blitz_key(code, modifiers, key)));
    }

    fn press_char(doc: &mut BaseDocument, ch: char) {
        let code = match ch {
            'a' => KeyCode::KeyA,
            'b' => KeyCode::KeyB,
            'c' => KeyCode::KeyC,
            _ => panic!("unsupported test character {ch}"),
        };
        doc.handle_ui_event(UiEvent::KeyDown(blitz_key(
            code,
            ModifiersState::empty(),
            Key::Character(ch.to_string()),
        )));
    }

    #[test]
    fn headless_address_bar_typing_and_backspace() {
        let mut doc = chrome_test_document();
        press_char(&mut doc, 'a');
        press_char(&mut doc, 'b');
        press_char(&mut doc, 'c');
        press_editing_key(&mut doc, KeyCode::Backspace, ModifiersState::empty());
        assert_eq!(
            text_value_in_doc(&doc, "url").as_deref(),
            Some("ab"),
            "address bar value after typing and Backspace"
        );
    }

    #[test]
    fn headless_word_delete_in_address_bar() {
        let mut doc = chrome_test_document();
        let url_id = doc.get_element_by_id("url").expect("url");
        doc.with_text_input(url_id, |mut driver| {
            driver.editor.set_text("hello world");
            driver.refresh_layout();
        });

        #[cfg(target_os = "macos")]
        {
            press_editing_key(
                &mut doc,
                KeyCode::ArrowRight,
                mods(false, false, false, true),
            );
            press_editing_key(
                &mut doc,
                KeyCode::Backspace,
                mods(false, true, false, false),
            );
        }
        #[cfg(not(target_os = "macos"))]
        {
            press_editing_key(&mut doc, KeyCode::End, ModifiersState::empty());
            press_editing_key(
                &mut doc,
                KeyCode::Backspace,
                mods(true, false, false, false),
            );
        }
        assert_eq!(
            text_value_in_doc(&doc, "url").as_deref(),
            Some("hello "),
            "address bar value after word Backspace"
        );
    }

    const PAGE_INPUT_FIXTURE: &str = include_str!("../tests/fixtures/input.html");

    fn chrome_with_page_srcdoc() -> BaseDocument {
        let html = chrome_document("about:blank");
        let mut doc = parse::document_from_html(&html, test_document_config(), false);
        layout_doc(&mut doc);
        let page_id = doc.get_element_by_id("page").expect("page iframe");
        {
            let mut m = doc.mutate();
            m.set_attribute(page_id, qual_name!("srcdoc"), PAGE_INPUT_FIXTURE);
        }
        layout_doc(&mut doc);
        let field_id = doc
            .get_element_by_id("page")
            .and_then(|id| doc.get_node(id))
            .and_then(|n| n.subdoc())
            .and_then(|sub| sub.inner().get_element_by_id("field"))
            .expect("field input in iframe srcdoc");
        doc.set_focus_to(page_id);
        if let Some(sub) = doc.get_node_mut(page_id).and_then(|n| n.subdoc_mut()) {
            sub.inner_mut().set_focus_to(field_id);
        }
        doc
    }

    #[test]
    fn headless_iframe_input_receives_keys() {
        let mut doc = chrome_with_page_srcdoc();
        let modifiers = ModifiersState::empty();
        for ch in ['a', 'b', 'c'] {
            let code = match ch {
                'a' => KeyCode::KeyA,
                'b' => KeyCode::KeyB,
                'c' => KeyCode::KeyC,
                _ => unreachable!(),
            };
            let blitz = blitz_key(code, modifiers, Key::Character(ch.to_string()));
            assert_eq!(
                forward_page_iframe_key(&mut doc, code, modifiers, true, blitz),
                Some(true),
                "iframe should consume key for {ch}"
            );
        }

        let page_id = doc.get_element_by_id("page").expect("page");
        let value = doc
            .get_node(page_id)
            .and_then(|n| n.subdoc())
            .and_then(|sub| text_value_in_doc(&sub.inner(), "field"))
            .expect("iframe field value");
        assert_eq!(value, "abc");
    }
}
