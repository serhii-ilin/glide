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
    event: &KeyEvent,
    modifiers: ModifiersState,
) -> Option<bool> {
    if !page_iframe_accepts_keys(chrome) {
        return None;
    }
    let PhysicalKey::Code(code) = event.physical_key else {
        return Some(false);
    };
    let pressed = event.state.is_pressed();
    if pressed && browser_shortcut(code, modifiers, false).is_some() {
        return Some(false);
    }
    let page_id = page_iframe_id(chrome).expect("page iframe is focused");
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
    let blitz_key = winit_key_to_blitz(event, modifiers);
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
        forward_page_iframe_key(chrome, event, modifiers)
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

    use blitz_dom::{DocumentConfig, qual_name};
    use blitz_traits::shell::{ColorScheme, Viewport};
    use keyboard_types::{Code, Key, NamedKey};
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

    fn ctrl_meta_mods() -> [(ModifiersState, &'static str); 2] {
        [
            (mods(true, false, false, false), "Ctrl"),
            (mods(false, false, false, true), "Meta"),
        ]
    }

    struct ShortcutCase {
        row: String,
        code: KeyCode,
        modifiers: ModifiersState,
        text_focused: bool,
        expected: Option<BrowserShortcut>,
    }

    fn case(
        row: impl Into<String>,
        code: KeyCode,
        modifiers: ModifiersState,
        text_focused: bool,
        expected: Option<BrowserShortcut>,
    ) -> ShortcutCase {
        ShortcutCase {
            row: row.into(),
            code,
            modifiers,
            text_focused,
            expected,
        }
    }

    fn both(
        row: &str,
        code: KeyCode,
        modifiers: ModifiersState,
        expected_unfocused: Option<BrowserShortcut>,
        expected_focused: Option<BrowserShortcut>,
    ) -> [ShortcutCase; 2] {
        [
            case(row, code, modifiers, false, expected_unfocused),
            case(
                format!("{row} (text focused)"),
                code,
                modifiers,
                true,
                expected_focused,
            ),
        ]
    }

    fn readme_shortcut_cases() -> Vec<ShortcutCase> {
        let alt = mods(false, true, false, false);
        let shift_only = mods(false, false, true, false);
        let none = ModifiersState::empty();
        let mut cases = Vec::new();

        cases.extend(both(
            "Enter (not a browser shortcut)",
            KeyCode::Enter,
            none,
            None,
            None,
        ));
        cases.extend(both(
            "Alt+Left back",
            KeyCode::ArrowLeft,
            alt,
            Some(BrowserShortcut::Back),
            Some(BrowserShortcut::Back),
        ));
        cases.extend(both(
            "Alt+Right forward",
            KeyCode::ArrowRight,
            alt,
            Some(BrowserShortcut::Forward),
            Some(BrowserShortcut::Forward),
        ));

        for (prim, tag) in ctrl_meta_mods() {
            cases.extend(both(
                &format!("{tag}+[ back"),
                KeyCode::BracketLeft,
                prim,
                Some(BrowserShortcut::Back),
                Some(BrowserShortcut::Back),
            ));
            cases.extend(both(
                &format!("{tag}+] forward"),
                KeyCode::BracketRight,
                prim,
                Some(BrowserShortcut::Forward),
                Some(BrowserShortcut::Forward),
            ));
            cases.extend(both(
                &format!("{tag}+R reload"),
                KeyCode::KeyR,
                prim,
                Some(BrowserShortcut::Reload),
                Some(BrowserShortcut::Reload),
            ));
            cases.extend(both(
                &format!("{tag}+L focus address bar"),
                KeyCode::KeyL,
                prim,
                Some(BrowserShortcut::FocusAddressBar),
                Some(BrowserShortcut::FocusAddressBar),
            ));
            cases.extend(both(
                &format!("{tag}+W close"),
                KeyCode::KeyW,
                prim,
                Some(BrowserShortcut::CloseWindow),
                Some(BrowserShortcut::CloseWindow),
            ));
            cases.extend(both(
                &format!("{tag}+Q quit"),
                KeyCode::KeyQ,
                prim,
                Some(BrowserShortcut::Quit),
                Some(BrowserShortcut::Quit),
            ));
            cases.push(case(
                format!("{tag}+Shift+R not intercepted"),
                KeyCode::KeyR,
                prim | ModifiersState::SHIFT,
                false,
                None,
            ));
            for (suffix, code) in [
                ("C not intercepted", KeyCode::KeyC),
                ("V not intercepted", KeyCode::KeyV),
                ("X not intercepted", KeyCode::KeyX),
                ("A not intercepted", KeyCode::KeyA),
            ] {
                cases.push(case(format!("{tag}+{suffix}"), code, prim, true, None));
            }
        }

        cases.extend(both(
            "F5 reload",
            KeyCode::F5,
            none,
            Some(BrowserShortcut::Reload),
            Some(BrowserShortcut::Reload),
        ));
        cases.push(case(
            "plain letter not intercepted",
            KeyCode::KeyA,
            none,
            false,
            None,
        ));

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
        for (row, code, modifiers, expected) in scroll_cases {
            cases.push(case(row, code, modifiers, false, Some(expected)));
            cases.push(case(
                format!("{row} (text focused)"),
                code,
                modifiers,
                true,
                None,
            ));
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

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_standard_keybinding_table() {
        let none = ModifiersState::empty();
        let shift = mods(false, false, true, false);
        let option = mods(false, true, false, false);
        let cmd = mods(false, false, false, true);
        let cmd_shift = cmd | shift;
        let option_shift = option | shift;

        let rows: &[(&str, KeyCode, ModifiersState, Option<&str>)] = &[
            (
                "plain Backspace",
                KeyCode::Backspace,
                none,
                Some("deleteBackward:"),
            ),
            (
                "plain Delete",
                KeyCode::Delete,
                none,
                Some("deleteForward:"),
            ),
            (
                "plain ArrowLeft",
                KeyCode::ArrowLeft,
                none,
                Some("moveLeft:"),
            ),
            (
                "plain ArrowRight",
                KeyCode::ArrowRight,
                none,
                Some("moveRight:"),
            ),
            ("plain ArrowUp", KeyCode::ArrowUp, none, Some("moveUp:")),
            (
                "plain ArrowDown",
                KeyCode::ArrowDown,
                none,
                Some("moveDown:"),
            ),
            (
                "plain Home",
                KeyCode::Home,
                none,
                Some("moveToBeginningOfLine:"),
            ),
            ("plain End", KeyCode::End, none, Some("moveToEndOfLine:")),
            ("plain Enter", KeyCode::Enter, none, Some("insertNewline:")),
            (
                "plain NumpadEnter",
                KeyCode::NumpadEnter,
                none,
                Some("insertNewline:"),
            ),
            (
                "Shift+ArrowLeft",
                KeyCode::ArrowLeft,
                shift,
                Some("moveLeftAndModifySelection:"),
            ),
            (
                "Shift+ArrowRight",
                KeyCode::ArrowRight,
                shift,
                Some("moveRightAndModifySelection:"),
            ),
            (
                "Shift+ArrowUp",
                KeyCode::ArrowUp,
                shift,
                Some("moveUpAndModifySelection:"),
            ),
            (
                "Shift+ArrowDown",
                KeyCode::ArrowDown,
                shift,
                Some("moveDownAndModifySelection:"),
            ),
            (
                "Option+ArrowLeft",
                KeyCode::ArrowLeft,
                option,
                Some("moveWordBackward:"),
            ),
            (
                "Option+ArrowRight",
                KeyCode::ArrowRight,
                option,
                Some("moveWordForward:"),
            ),
            (
                "Option+Shift+ArrowLeft",
                KeyCode::ArrowLeft,
                option_shift,
                Some("moveWordBackwardAndModifySelection:"),
            ),
            (
                "Option+Shift+ArrowRight",
                KeyCode::ArrowRight,
                option_shift,
                Some("moveWordForwardAndModifySelection:"),
            ),
            (
                "Option+Backspace",
                KeyCode::Backspace,
                option,
                Some("deleteWordBackward:"),
            ),
            (
                "Option+Delete",
                KeyCode::Delete,
                option,
                Some("deleteWordForward:"),
            ),
            (
                "Cmd+ArrowLeft",
                KeyCode::ArrowLeft,
                cmd,
                Some("moveToBeginningOfLine:"),
            ),
            (
                "Cmd+ArrowRight",
                KeyCode::ArrowRight,
                cmd,
                Some("moveToEndOfLine:"),
            ),
            (
                "Cmd+Shift+ArrowLeft",
                KeyCode::ArrowLeft,
                cmd_shift,
                Some("moveToBeginningOfLineAndModifySelection:"),
            ),
            (
                "Cmd+Shift+ArrowRight",
                KeyCode::ArrowRight,
                cmd_shift,
                Some("moveToEndOfLineAndModifySelection:"),
            ),
            ("Cmd+C passthrough", KeyCode::KeyC, cmd, None),
        ];

        for &(row, code, modifiers, expected) in rows {
            let got = macos_standard_keybinding(code, modifiers);
            assert_eq!(got, expected, "macOS keybinding row failed: {row}");
        }
    }

    fn char_code(ch: char) -> KeyCode {
        match ch {
            'a' => KeyCode::KeyA,
            'b' => KeyCode::KeyB,
            'c' => KeyCode::KeyC,
            _ => panic!("unsupported test character {ch}"),
        }
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
            modifiers: winit_modifiers_to_kbt(modifiers),
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
        let code = char_code(ch);
        doc.handle_ui_event(UiEvent::KeyDown(blitz_key(
            code,
            ModifiersState::empty(),
            Key::Character(ch.to_string()),
        )));
    }

    use winit::event::ElementState;
    use winit::keyboard::Key as WinitKey;

    fn test_key_event(code: KeyCode, state: ElementState, logical: WinitKey) -> KeyEvent {
        let key_without_modifiers = logical.clone();
        KeyEvent {
            physical_key: PhysicalKey::Code(code),
            logical_key: logical,
            text: None,
            text_with_all_modifiers: None,
            key_without_modifiers,
            location: winit::keyboard::KeyLocation::Standard,
            state,
            repeat: false,
        }
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
            let code = char_code(ch);
            let event = test_key_event(
                code,
                ElementState::Pressed,
                WinitKey::Character(ch.to_string().into()),
            );
            assert_eq!(
                forward_page_iframe_key(&mut doc, &event, modifiers),
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

    #[test]
    fn forward_page_iframe_key_forwards_key_up() {
        let mut doc = chrome_with_page_srcdoc();
        let modifiers = ModifiersState::empty();
        let event = test_key_event(
            KeyCode::KeyA,
            ElementState::Released,
            WinitKey::Character("a".into()),
        );
        assert_eq!(
            forward_page_iframe_key(&mut doc, &event, modifiers),
            Some(true)
        );
    }

    #[test]
    fn forward_page_iframe_key_leaves_browser_shortcut() {
        let mut doc = chrome_with_page_srcdoc();
        let event = test_key_event(
            KeyCode::KeyL,
            ElementState::Pressed,
            WinitKey::Character("l".into()),
        );
        for prim in [
            mods(true, false, false, false),
            mods(false, false, false, true),
        ] {
            assert_eq!(forward_page_iframe_key(&mut doc, &event, prim), Some(false));
        }
    }

    #[test]
    fn forward_page_iframe_key_ignores_unfocused_iframe() {
        let mut doc = chrome_with_page_srcdoc();
        let url_id = doc.get_element_by_id("url").expect("url");
        doc.set_focus_to(url_id);
        let event = test_key_event(
            KeyCode::KeyA,
            ElementState::Pressed,
            WinitKey::Character("a".into()),
        );
        assert_eq!(
            forward_page_iframe_key(&mut doc, &event, ModifiersState::empty()),
            None
        );
    }
}
