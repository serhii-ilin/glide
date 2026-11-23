//! Keyboard routing for Glide: browser shortcuts, macOS text-editing keybindings,
//! and forwarding keys into the page `<iframe>` when it has focus.
//!
//! Blitz deliberately handles `Backspace` only through Apple's standard keybinding
//! callbacks on macOS (see `blitz-dom` `TextInputData::apply_keypress_event`). Copy,
//! cut, and paste need a character key (`c` / `x` / `v`) plus the platform action
//! modifier, which macOS does not always put on the key event when Command is held.
//! Glide synthesizes those editing keys and forwards them into iframe sub-documents.

use blitz_dom::{BaseDocument, Document};
use blitz_shell::View;
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
    Stop,
    ToggleJs,
    ToggleBookmark,
    FocusAddressBar,
    CloseTab,
    NewTab,
    NextTab,
    PreviousTab,
    SelectTab(usize),
    Quit,
    Find,
    FindClose,
    FindNext,
    FindPrev,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ToggleDevTools,
    DevToolsClose,
    DevToolsConsoleSubmit,
    DevToolsConsoleHistoryPrev,
    DevToolsConsoleHistoryNext,
    DevToolsElementsUp,
    DevToolsElementsDown,
    DevToolsElementsExpand,
    DevToolsElementsCollapse,
    DevToolsInspectToggle,
    DevToolsInspectCancel,
    MenuClose,
    ShowBookmarks,
    ShowHelp,
    CloseWindow,
    DuplicateTab,
    MinimizeWindow,
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
    find_bar_open: bool,
) -> Option<BrowserShortcut> {
    browser_shortcut_on(
        cfg!(target_os = "macos"),
        code,
        modifiers,
        text_input_focused,
        find_bar_open,
    )
}

/// [`browser_shortcut`] with the platform explicit so both variants are testable everywhere.
///
/// Off macOS, Ctrl+Alt+B and Ctrl+M yield to a focused text input: Windows delivers AltGr
/// as Ctrl+Alt, and Ctrl+M is Enter in some text fields.
pub fn browser_shortcut_on(
    macos: bool,
    code: KeyCode,
    modifiers: ModifiersState,
    text_input_focused: bool,
    find_bar_open: bool,
) -> Option<BrowserShortcut> {
    let yield_to_text = !macos && text_input_focused;
    let primary = modifiers.control_key() || modifiers.meta_key();
    let alt = modifiers.alt_key();
    let shift = modifiers.shift_key();

    match code {
        KeyCode::Equal if primary => return Some(BrowserShortcut::ZoomIn),
        KeyCode::NumpadAdd if primary => return Some(BrowserShortcut::ZoomIn),
        KeyCode::Minus if primary => return Some(BrowserShortcut::ZoomOut),
        KeyCode::NumpadSubtract if primary => return Some(BrowserShortcut::ZoomOut),
        KeyCode::Digit0 if primary => return Some(BrowserShortcut::ZoomReset),
        KeyCode::KeyF if primary && !shift => return Some(BrowserShortcut::Find),
        KeyCode::KeyG if primary && find_bar_open => {
            if shift {
                return Some(BrowserShortcut::FindPrev);
            }
            return Some(BrowserShortcut::FindNext);
        }
        KeyCode::KeyL if primary && !shift => return Some(BrowserShortcut::FocusAddressBar),
        KeyCode::KeyR if primary && !shift => return Some(BrowserShortcut::Reload),
        KeyCode::KeyT if primary && !shift => return Some(BrowserShortcut::NewTab),
        KeyCode::KeyW if primary && shift => return Some(BrowserShortcut::CloseWindow),
        KeyCode::KeyW if primary && !shift => return Some(BrowserShortcut::CloseTab),
        KeyCode::Slash if primary && shift => return Some(BrowserShortcut::ShowHelp),
        KeyCode::KeyQ if primary && !shift => return Some(BrowserShortcut::Quit),
        KeyCode::Tab if primary && !shift => return Some(BrowserShortcut::NextTab),
        KeyCode::Tab if primary && shift => return Some(BrowserShortcut::PreviousTab),
        KeyCode::BracketRight if primary && shift => return Some(BrowserShortcut::NextTab),
        KeyCode::BracketLeft if primary && shift => return Some(BrowserShortcut::PreviousTab),
        KeyCode::BracketLeft if primary && !shift => return Some(BrowserShortcut::Back),
        KeyCode::BracketRight if primary && !shift => return Some(BrowserShortcut::Forward),
        KeyCode::Digit1 if primary && !shift => return Some(BrowserShortcut::SelectTab(0)),
        KeyCode::Digit2 if primary && !shift => return Some(BrowserShortcut::SelectTab(1)),
        KeyCode::Digit3 if primary && !shift => return Some(BrowserShortcut::SelectTab(2)),
        KeyCode::Digit4 if primary && !shift => return Some(BrowserShortcut::SelectTab(3)),
        KeyCode::Digit5 if primary && !shift => return Some(BrowserShortcut::SelectTab(4)),
        KeyCode::Digit6 if primary && !shift => return Some(BrowserShortcut::SelectTab(5)),
        KeyCode::Digit7 if primary && !shift => return Some(BrowserShortcut::SelectTab(6)),
        KeyCode::Digit8 if primary && !shift => return Some(BrowserShortcut::SelectTab(7)),
        KeyCode::Digit9 if primary && !shift => return Some(BrowserShortcut::SelectTab(8)),
        KeyCode::KeyJ if primary && shift => return Some(BrowserShortcut::ToggleJs),
        KeyCode::KeyC if primary && shift && !alt => {
            return Some(BrowserShortcut::DevToolsInspectToggle);
        }
        KeyCode::KeyD if primary && !shift => return Some(BrowserShortcut::ToggleBookmark),
        KeyCode::KeyB if primary && alt && !shift && !yield_to_text => {
            return Some(BrowserShortcut::ShowBookmarks);
        }
        KeyCode::KeyM if primary && !shift && !alt && !yield_to_text => {
            return Some(BrowserShortcut::MinimizeWindow);
        }
        KeyCode::Period if primary && !shift && !alt => {
            return Some(BrowserShortcut::Stop);
        }
        KeyCode::F12 if !primary && !alt && !shift => return Some(BrowserShortcut::ToggleDevTools),
        KeyCode::KeyI
            if (modifiers.meta_key() && modifiers.alt_key())
                || (modifiers.control_key() && modifiers.shift_key()) =>
        {
            return Some(BrowserShortcut::ToggleDevTools);
        }
        KeyCode::F5 if !primary && !alt => return Some(BrowserShortcut::Reload),
        KeyCode::ArrowLeft if alt && !primary => return Some(BrowserShortcut::Back),
        KeyCode::ArrowRight if alt && !primary => return Some(BrowserShortcut::Forward),
        KeyCode::Escape if !text_input_focused && !primary && !alt => {
            return Some(BrowserShortcut::Stop);
        }
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

/// Escape closes an open menu before Escape is treated as Stop.
pub fn menu_shortcut(
    code: KeyCode,
    modifiers: ModifiersState,
    text_input_focused: bool,
    menu_open: bool,
) -> Option<BrowserShortcut> {
    if !menu_open || text_input_focused {
        return None;
    }
    let primary = modifiers.control_key() || modifiers.meta_key();
    if code == KeyCode::Escape && !primary && !modifiers.alt_key() {
        return Some(BrowserShortcut::MenuClose);
    }
    None
}

#[cfg(target_os = "macos")]
pub fn macos_standard_keybinding(code: KeyCode, modifiers: ModifiersState) -> Option<&'static str> {
    if modifiers.control_key() || modifiers.meta_key() {
        match code {
            KeyCode::KeyZ if !modifiers.shift_key() => return None, // undo not implemented in Blitz
            KeyCode::KeyA if !modifiers.shift_key() && !modifiers.alt_key() => {
                return Some("selectAll:");
            }
            KeyCode::Backspace
                if modifiers.meta_key()
                    && !modifiers.control_key()
                    && !modifiers.shift_key()
                    && !modifiers.alt_key() =>
            {
                return Some("deleteToBeginningOfLine:");
            }
            KeyCode::Delete
                if modifiers.meta_key()
                    && !modifiers.control_key()
                    && !modifiers.shift_key()
                    && !modifiers.alt_key() =>
            {
                return Some("deleteToEndOfLine:");
            }
            KeyCode::KeyC | KeyCode::KeyV | KeyCode::KeyX if !modifiers.shift_key() => {
                return None; // copy, cut, and paste are sent as character keys below
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

/// `c`, `x`, or `v` when the key is the platform copy / cut / paste shortcut.
fn clipboard_character(code: KeyCode, modifiers: ModifiersState) -> Option<char> {
    if modifiers.alt_key() || modifiers.shift_key() {
        return None;
    }
    let primary = if cfg!(target_os = "macos") {
        modifiers.meta_key() ^ modifiers.control_key()
    } else {
        modifiers.control_key() && !modifiers.meta_key()
    };
    if !primary {
        return None;
    }
    match code {
        KeyCode::KeyC => Some('c'),
        KeyCode::KeyX => Some('x'),
        KeyCode::KeyV => Some('v'),
        _ => None,
    }
}

fn action_mod() -> Modifiers {
    #[cfg(target_os = "macos")]
    {
        Modifiers::META
    }
    #[cfg(not(target_os = "macos"))]
    {
        Modifiers::CONTROL
    }
}

fn clipboard_key_event(ch: char) -> BlitzKeyEvent {
    let mut modifiers = Modifiers::default();
    modifiers.insert(action_mod());
    let code = match ch {
        'c' => Code::KeyC,
        'x' => Code::KeyX,
        'v' => Code::KeyV,
        _ => Code::Unidentified,
    };
    BlitzKeyEvent {
        key: Key::Character(ch.to_string()),
        code,
        modifiers,
        location: keyboard_types::Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: KeyState::Pressed,
        text: None,
    }
}

/// Copy, cut, or paste in `doc` from the physical key, ignoring the logical key.
pub(crate) fn apply_clipboard_shortcut(
    doc: &mut BaseDocument,
    code: KeyCode,
    modifiers: ModifiersState,
) -> bool {
    let Some(ch) = clipboard_character(code, modifiers) else {
        return false;
    };
    doc.handle_ui_event(UiEvent::KeyDown(clipboard_key_event(ch)));
    true
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
    if pressed && browser_shortcut(code, modifiers, false, false).is_some() {
        return Some(false);
    }
    if pressed && clipboard_character(code, modifiers).is_some() {
        let page_id = page_iframe_id(chrome).expect("page iframe is focused");
        let Some(sub) = chrome
            .get_node_mut(page_id)
            .and_then(|node| node.subdoc_mut())
        else {
            return Some(false);
        };
        let mut inner = sub.inner_mut();
        apply_clipboard_shortcut(&mut inner, code, modifiers);
        return Some(true);
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
    {
        let mut guard = view.doc.inner_mut();
        let chrome = &mut *guard;
        let text_focused = chrome
            .get_focussed_node_id()
            .is_some_and(|id| node_is_text_input(chrome, id));
        if text_focused && apply_clipboard_shortcut(chrome, code, modifiers) {
            return true;
        }
    }
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

/// Whether Glide should consume a Stop shortcut (Esc / toolbar) instead of forwarding it.
pub fn consume_stop_shortcut(shortcut: &BrowserShortcut, page_loading: bool) -> bool {
    matches!(shortcut, BrowserShortcut::Stop) && page_loading
}

pub fn nav_from_shortcut(shortcut: BrowserShortcut) -> Option<NavCommand> {
    match shortcut {
        BrowserShortcut::Back => Some(NavCommand::Back),
        BrowserShortcut::Forward => Some(NavCommand::Forward),
        BrowserShortcut::Reload => Some(NavCommand::Reload),
        BrowserShortcut::Stop => Some(NavCommand::Stop),
        BrowserShortcut::MenuClose => Some(NavCommand::MenuClose),
        BrowserShortcut::ToggleJs => Some(NavCommand::ToggleJs),
        BrowserShortcut::ToggleBookmark => Some(NavCommand::ToggleBookmark),
        BrowserShortcut::FocusAddressBar
        | BrowserShortcut::CloseTab
        | BrowserShortcut::NewTab
        | BrowserShortcut::NextTab
        | BrowserShortcut::PreviousTab
        | BrowserShortcut::SelectTab(_)
        | BrowserShortcut::Quit
        | BrowserShortcut::Find
        | BrowserShortcut::FindClose
        | BrowserShortcut::FindNext
        | BrowserShortcut::FindPrev
        | BrowserShortcut::ZoomIn
        | BrowserShortcut::ZoomOut
        | BrowserShortcut::ZoomReset
        | BrowserShortcut::ToggleDevTools
        | BrowserShortcut::DevToolsClose
        | BrowserShortcut::DevToolsConsoleSubmit
        | BrowserShortcut::DevToolsConsoleHistoryPrev
        | BrowserShortcut::DevToolsConsoleHistoryNext
        | BrowserShortcut::DevToolsElementsUp
        | BrowserShortcut::DevToolsElementsDown
        | BrowserShortcut::DevToolsElementsExpand
        | BrowserShortcut::DevToolsElementsCollapse
        | BrowserShortcut::DevToolsInspectToggle
        | BrowserShortcut::DevToolsInspectCancel => None,
        BrowserShortcut::Scroll(_) => None,
        BrowserShortcut::MinimizeWindow
        | BrowserShortcut::ShowHelp
        | BrowserShortcut::ShowBookmarks
        | BrowserShortcut::CloseWindow
        | BrowserShortcut::DuplicateTab => None,
    }
}

/// Text-editing actions a native menu item can request.
///
/// Undo and Redo are not here: Blitz has no undo stack, so those menu items stay disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl EditAction {
    pub fn from_menu(command: &crate::menu::MenuCommand) -> Option<Self> {
        match command {
            crate::menu::MenuCommand::EditCut => Some(Self::Cut),
            crate::menu::MenuCommand::EditCopy => Some(Self::Copy),
            crate::menu::MenuCommand::EditPaste => Some(Self::Paste),
            crate::menu::MenuCommand::EditSelectAll => Some(Self::SelectAll),
            _ => None,
        }
    }

    fn key_code(self) -> KeyCode {
        match self {
            Self::Cut => KeyCode::KeyX,
            Self::Copy => KeyCode::KeyC,
            Self::Paste => KeyCode::KeyV,
            Self::SelectAll => KeyCode::KeyA,
        }
    }
}

fn primary_modifiers() -> ModifiersState {
    if cfg!(target_os = "macos") {
        ModifiersState::META
    } else {
        ModifiersState::CONTROL
    }
}

/// Run a menu edit command on whatever has focus, exactly as its key press would
/// (page iframe first, then a focused chrome text field), see [`dispatch_text_keys`].
pub fn apply_edit_action_to_doc(chrome: &mut BaseDocument, action: EditAction) -> bool {
    let code = action.key_code();
    let modifiers = primary_modifiers();
    if let Some(handled) = forward_page_iframe_key(chrome, &synthetic_key_event(code), modifiers) {
        return handled;
    }
    let text_focused = chrome
        .get_focussed_node_id()
        .is_some_and(|id| node_is_text_input(chrome, id));
    if text_focused && apply_clipboard_shortcut(chrome, code, modifiers) {
        return true;
    }
    #[cfg(target_os = "macos")]
    return apply_macos_chrome_keybinding(chrome, code, modifiers);
    #[cfg(not(target_os = "macos"))]
    return false;
}

/// [`apply_edit_action_to_doc`] on a window's chrome document.
pub fn apply_edit_action<Rend: WindowRenderer>(view: &mut View<Rend>, action: EditAction) -> bool {
    let mut guard = view.doc.inner_mut();
    apply_edit_action_to_doc(&mut guard, action)
}

/// Events that replay a menu edit chord as a real key press when nothing editable took
/// it: NSMenu consumed the key before winit saw it, so Blitz's own handling (for example
/// copying selected text in the DevTools panel) would otherwise be lost. The modifier
/// state is set for the press and restored afterwards.
pub fn edit_fallback_events(action: EditAction, previous: ModifiersState) -> Vec<WindowEvent> {
    let code = action.key_code();
    vec![
        WindowEvent::ModifiersChanged(primary_modifiers().into()),
        WindowEvent::KeyboardInput {
            device_id: None,
            event: synthetic_key_event(code),
            is_synthetic: false,
        },
        WindowEvent::ModifiersChanged(previous.into()),
    ]
}

fn synthetic_key_event(code: KeyCode) -> KeyEvent {
    let ch = match code {
        KeyCode::KeyX => "x",
        KeyCode::KeyV => "v",
        KeyCode::KeyA => "a",
        _ => "c",
    };
    KeyEvent {
        location: winit::keyboard::KeyLocation::Standard,
        logical_key: winit::keyboard::Key::Character(ch.into()),
        physical_key: PhysicalKey::Code(code),
        repeat: false,
        text: None,
        state: ElementState::Pressed,
        key_without_modifiers: winit::keyboard::Key::Unidentified(
            winit::keyboard::NativeKey::Unidentified,
        ),
        text_with_all_modifiers: None,
    }
}

pub fn try_browser_shortcut(
    view: &mut View<impl WindowRenderer>,
    event: &WindowEvent,
    menu_open: bool,
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
    let find_open = crate::chrome::find_bar_open(&chrome);
    let modifiers = view.keyboard_modifiers.state();
    if let Some(shortcut) = menu_shortcut(code, modifiers, text_focused, menu_open) {
        return Some(shortcut);
    }
    browser_shortcut(code, modifiers, text_focused, find_open)
}

/// Esc while element inspect mode is active (before find/DevTools panel handlers).
pub fn dispatch_inspect_mode_escape(
    inspect_mode: bool,
    event: &KeyEvent,
) -> Option<BrowserShortcut> {
    if !inspect_mode || !event.state.is_pressed() {
        return None;
    }
    let PhysicalKey::Code(code) = event.physical_key else {
        return None;
    };
    if matches!(code, KeyCode::Escape) {
        Some(BrowserShortcut::DevToolsInspectCancel)
    } else {
        None
    }
}

/// Find-field keys (Enter, Shift+Enter, Esc) before Blitz handles them.
/// DevTools panel keys before Blitz handles them.
pub fn dispatch_devtools_field_keys(
    chrome: &BaseDocument,
    event: &KeyEvent,
    devtools_open: bool,
) -> Option<BrowserShortcut> {
    if !devtools_open || !event.state.is_pressed() {
        return None;
    }
    if !crate::chrome::devtools_focused(chrome) {
        return None;
    }
    let PhysicalKey::Code(code) = event.physical_key else {
        return None;
    };
    match code {
        KeyCode::Escape => Some(BrowserShortcut::DevToolsClose),
        _ => None,
    }
}

pub fn dispatch_devtools_elements_keys(
    chrome: &BaseDocument,
    event: &KeyEvent,
    devtools_open: bool,
    elements_tab_active: bool,
) -> Option<BrowserShortcut> {
    if !devtools_open || !elements_tab_active || !event.state.is_pressed() {
        return None;
    }
    if !crate::chrome::devtools_elements_tree_focused(chrome) {
        return None;
    }
    let PhysicalKey::Code(code) = event.physical_key else {
        return None;
    };
    match code {
        KeyCode::ArrowUp => Some(BrowserShortcut::DevToolsElementsUp),
        KeyCode::ArrowDown => Some(BrowserShortcut::DevToolsElementsDown),
        KeyCode::ArrowRight => Some(BrowserShortcut::DevToolsElementsExpand),
        KeyCode::ArrowLeft => Some(BrowserShortcut::DevToolsElementsCollapse),
        _ => None,
    }
}

pub fn dispatch_devtools_console_keys(
    chrome: &BaseDocument,
    event: &KeyEvent,
    devtools_open: bool,
    console_tab_active: bool,
    ime_composing: bool,
) -> Option<BrowserShortcut> {
    if !devtools_open || !console_tab_active || !event.state.is_pressed() {
        return None;
    }
    if !crate::chrome::devtools_console_input_focused(chrome) {
        return None;
    }
    let PhysicalKey::Code(code) = event.physical_key else {
        return None;
    };
    match code {
        KeyCode::Enter | KeyCode::NumpadEnter if !ime_composing => {
            Some(BrowserShortcut::DevToolsConsoleSubmit)
        }
        KeyCode::ArrowUp => Some(BrowserShortcut::DevToolsConsoleHistoryPrev),
        KeyCode::ArrowDown => Some(BrowserShortcut::DevToolsConsoleHistoryNext),
        KeyCode::Escape => Some(BrowserShortcut::DevToolsClose),
        _ => None,
    }
}

pub fn dispatch_find_field_keys(
    chrome: &mut BaseDocument,
    event: &KeyEvent,
    modifiers: ModifiersState,
    find_open: bool,
) -> Option<BrowserShortcut> {
    if !find_open || !event.state.is_pressed() {
        return None;
    }
    if !crate::chrome::find_input_focused(chrome) {
        return None;
    }
    let PhysicalKey::Code(code) = event.physical_key else {
        return None;
    };
    match code {
        KeyCode::Enter | KeyCode::NumpadEnter => {
            let shift = modifiers.shift_key();
            if shift {
                Some(BrowserShortcut::FindPrev)
            } else {
                Some(BrowserShortcut::FindNext)
            }
        }
        KeyCode::Escape => Some(BrowserShortcut::FindClose),
        _ => None,
    }
}

/// Focus the chrome address field (`#url`) and select its text.
pub fn focus_address_bar<Rend: WindowRenderer>(view: &mut View<Rend>) {
    let mut doc = view.doc.inner_mut();
    focus_address_field(&mut doc);
}

/// Focus `#url` and select the whole value so the next key replaces it.
pub fn focus_address_field(doc: &mut BaseDocument) {
    if let Some(id) = doc.get_element_by_id("url") {
        doc.set_focus_to(id);
        doc.handle_ui_event(UiEvent::AppleStandardKeybinding(SmolStr::new("selectAll:")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use blitz_dom::{Document, DocumentConfig, qual_name};
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
        find_bar_open: bool,
        expected: Option<BrowserShortcut>,
    }

    fn case(
        row: impl Into<String>,
        code: KeyCode,
        modifiers: ModifiersState,
        text_focused: bool,
        find_bar_open: bool,
        expected: Option<BrowserShortcut>,
    ) -> ShortcutCase {
        ShortcutCase {
            row: row.into(),
            code,
            modifiers,
            text_focused,
            find_bar_open,
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
            case(row, code, modifiers, false, false, expected_unfocused),
            case(
                format!("{row} (text focused)"),
                code,
                modifiers,
                true,
                false,
                expected_focused,
            ),
        ]
    }

    fn zoom_cases() -> Vec<ShortcutCase> {
        let mut out = Vec::new();
        for (prim, tag) in ctrl_meta_mods() {
            let shift = prim | ModifiersState::SHIFT;
            for (row, code, modifiers, expected) in [
                (
                    format!("{tag}+= zoom in"),
                    KeyCode::Equal,
                    prim,
                    BrowserShortcut::ZoomIn,
                ),
                (
                    format!("{tag}+Shift+= zoom in (plus)"),
                    KeyCode::Equal,
                    shift,
                    BrowserShortcut::ZoomIn,
                ),
                (
                    format!("{tag}+- zoom out"),
                    KeyCode::Minus,
                    prim,
                    BrowserShortcut::ZoomOut,
                ),
                (
                    format!("{tag}+Shift+- zoom out"),
                    KeyCode::Minus,
                    shift,
                    BrowserShortcut::ZoomOut,
                ),
                (
                    format!("{tag}+0 zoom reset"),
                    KeyCode::Digit0,
                    prim,
                    BrowserShortcut::ZoomReset,
                ),
                (
                    format!("{tag}+Shift+0 zoom reset"),
                    KeyCode::Digit0,
                    shift,
                    BrowserShortcut::ZoomReset,
                ),
                (
                    format!("{tag}+NumpadAdd zoom in"),
                    KeyCode::NumpadAdd,
                    prim,
                    BrowserShortcut::ZoomIn,
                ),
                (
                    format!("{tag}+NumpadSubtract zoom out"),
                    KeyCode::NumpadSubtract,
                    prim,
                    BrowserShortcut::ZoomOut,
                ),
            ] {
                out.push(case(row, code, modifiers, false, false, Some(expected)));
            }
        }
        out
    }

    fn readme_shortcut_cases() -> Vec<ShortcutCase> {
        let alt = mods(false, true, false, false);
        let shift_only = mods(false, false, true, false);
        let none = ModifiersState::empty();
        let mut cases = Vec::new();

        for (prim, tag) in ctrl_meta_mods() {
            cases.extend(both(
                &format!("{tag}+F find"),
                KeyCode::KeyF,
                prim,
                Some(BrowserShortcut::Find),
                Some(BrowserShortcut::Find),
            ));
            cases.push(case(
                format!("{tag}+G find next (bar open)"),
                KeyCode::KeyG,
                prim,
                false,
                true,
                Some(BrowserShortcut::FindNext),
            ));
            cases.push(case(
                format!("{tag}+Shift+G find prev (bar open)"),
                KeyCode::KeyG,
                prim | ModifiersState::SHIFT,
                false,
                true,
                Some(BrowserShortcut::FindPrev),
            ));
        }
        cases.extend(zoom_cases());
        cases.extend(both(
            "F12 toggle DevTools",
            KeyCode::F12,
            none,
            Some(BrowserShortcut::ToggleDevTools),
            Some(BrowserShortcut::ToggleDevTools),
        ));
        cases.extend(both(
            "Meta+Alt+I toggle DevTools",
            KeyCode::KeyI,
            ModifiersState::META | ModifiersState::ALT,
            Some(BrowserShortcut::ToggleDevTools),
            Some(BrowserShortcut::ToggleDevTools),
        ));
        cases.extend(both(
            "Ctrl+Shift+I toggle DevTools",
            KeyCode::KeyI,
            ModifiersState::CONTROL | ModifiersState::SHIFT,
            Some(BrowserShortcut::ToggleDevTools),
            Some(BrowserShortcut::ToggleDevTools),
        ));
        cases.extend(both(
            "Ctrl+Shift+C toggle inspect mode",
            KeyCode::KeyC,
            ModifiersState::CONTROL | ModifiersState::SHIFT,
            Some(BrowserShortcut::DevToolsInspectToggle),
            Some(BrowserShortcut::DevToolsInspectToggle),
        ));
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
                &format!("{tag}+Shift+J toggle JavaScript"),
                KeyCode::KeyJ,
                prim | ModifiersState::SHIFT,
                Some(BrowserShortcut::ToggleJs),
                Some(BrowserShortcut::ToggleJs),
            ));
            cases.extend(both(
                &format!("{tag}+D toggle bookmark"),
                KeyCode::KeyD,
                prim,
                Some(BrowserShortcut::ToggleBookmark),
                Some(BrowserShortcut::ToggleBookmark),
            ));
            cases.extend(both(
                &format!("{tag}+Alt+B show bookmarks"),
                KeyCode::KeyB,
                prim | ModifiersState::ALT,
                Some(BrowserShortcut::ShowBookmarks),
                if cfg!(target_os = "macos") {
                    Some(BrowserShortcut::ShowBookmarks)
                } else {
                    None
                },
            ));
            cases.extend(both(
                &format!("{tag}+Shift+W close window"),
                KeyCode::KeyW,
                prim | ModifiersState::SHIFT,
                Some(BrowserShortcut::CloseWindow),
                Some(BrowserShortcut::CloseWindow),
            ));
            cases.extend(both(
                &format!("{tag}+M minimize window"),
                KeyCode::KeyM,
                prim,
                Some(BrowserShortcut::MinimizeWindow),
                if cfg!(target_os = "macos") {
                    Some(BrowserShortcut::MinimizeWindow)
                } else {
                    None
                },
            ));
            cases.extend(both(
                &format!("{tag}+. stop loading"),
                KeyCode::Period,
                prim,
                Some(BrowserShortcut::Stop),
                Some(BrowserShortcut::Stop),
            ));
            cases.extend(both(
                &format!("{tag}+Shift+/ keyboard shortcuts help"),
                KeyCode::Slash,
                prim | ModifiersState::SHIFT,
                Some(BrowserShortcut::ShowHelp),
                Some(BrowserShortcut::ShowHelp),
            ));
            cases.extend(both(
                "Shift+J not intercepted",
                KeyCode::KeyJ,
                shift_only,
                None,
                None,
            ));
            cases.extend(both(
                &format!("{tag}+L focus address bar"),
                KeyCode::KeyL,
                prim,
                Some(BrowserShortcut::FocusAddressBar),
                Some(BrowserShortcut::FocusAddressBar),
            ));
            cases.extend(both(
                &format!("{tag}+W close tab"),
                KeyCode::KeyW,
                prim,
                Some(BrowserShortcut::CloseTab),
                Some(BrowserShortcut::CloseTab),
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
                false,
                None,
            ));
            for (suffix, code) in [
                ("C not intercepted", KeyCode::KeyC),
                ("V not intercepted", KeyCode::KeyV),
                ("X not intercepted", KeyCode::KeyX),
                ("A not intercepted", KeyCode::KeyA),
            ] {
                cases.push(case(
                    format!("{tag}+{suffix}"),
                    code,
                    prim,
                    true,
                    false,
                    None,
                ));
            }
        }

        cases.extend(both(
            "F5 reload",
            KeyCode::F5,
            none,
            Some(BrowserShortcut::Reload),
            Some(BrowserShortcut::Reload),
        ));
        cases.extend(both(
            "Esc stop load",
            KeyCode::Escape,
            none,
            Some(BrowserShortcut::Stop),
            None,
        ));
        cases.push(case(
            "plain letter not intercepted",
            KeyCode::KeyA,
            none,
            false,
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
            cases.push(case(row, code, modifiers, false, false, Some(expected)));
            cases.push(case(
                format!("{row} (text focused)"),
                code,
                modifiers,
                true,
                false,
                None,
            ));
        }

        cases
    }

    #[test]
    fn readme_shortcut_table() {
        for case in readme_shortcut_cases() {
            let got = browser_shortcut(
                case.code,
                case.modifiers,
                case.text_focused,
                case.find_bar_open,
            );
            assert_eq!(
                got, case.expected,
                "README shortcut row failed: {}",
                case.row
            );
        }
    }

    #[test]
    fn tab_shortcut_table() {
        for (code, modifiers, expected) in [
            (
                KeyCode::KeyT,
                mods(false, false, false, true),
                BrowserShortcut::NewTab,
            ),
            (
                KeyCode::KeyW,
                mods(false, false, false, true),
                BrowserShortcut::CloseTab,
            ),
            (
                KeyCode::Tab,
                mods(false, false, false, true),
                BrowserShortcut::NextTab,
            ),
            (
                KeyCode::Tab,
                mods(false, false, true, true),
                BrowserShortcut::PreviousTab,
            ),
            (
                KeyCode::BracketRight,
                mods(false, false, true, true),
                BrowserShortcut::NextTab,
            ),
            (
                KeyCode::BracketLeft,
                mods(false, false, true, true),
                BrowserShortcut::PreviousTab,
            ),
            (
                KeyCode::Digit1,
                mods(false, false, false, true),
                BrowserShortcut::SelectTab(0),
            ),
            (
                KeyCode::Digit9,
                mods(false, false, false, true),
                BrowserShortcut::SelectTab(8),
            ),
        ] {
            assert_eq!(
                browser_shortcut(code, modifiers, false, false),
                Some(expected),
                "unexpected classification for {code:?} {modifiers:?}"
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
        assert_eq!(
            nav_from_shortcut(BrowserShortcut::Stop),
            Some(NavCommand::Stop)
        );
        assert_eq!(
            nav_from_shortcut(BrowserShortcut::ToggleJs),
            Some(NavCommand::ToggleJs)
        );
        assert_eq!(
            nav_from_shortcut(BrowserShortcut::ToggleBookmark),
            Some(NavCommand::ToggleBookmark)
        );
        assert!(nav_from_shortcut(BrowserShortcut::FocusAddressBar).is_none());
        assert_eq!(
            nav_from_shortcut(BrowserShortcut::MenuClose),
            Some(NavCommand::MenuClose)
        );
    }

    #[test]
    fn escape_closes_menu_before_stop() {
        let none = ModifiersState::empty();
        let cmd = ModifiersState::META;
        assert_eq!(
            menu_shortcut(KeyCode::Escape, none, false, true),
            Some(BrowserShortcut::MenuClose)
        );
        assert!(menu_shortcut(KeyCode::Escape, none, false, false).is_none());
        assert!(menu_shortcut(KeyCode::Escape, none, true, true).is_none());
        assert!(menu_shortcut(KeyCode::Escape, cmd, false, true).is_none());
        assert_eq!(
            browser_shortcut(KeyCode::Escape, none, false, false),
            Some(BrowserShortcut::Stop)
        );
    }

    #[test]
    fn consume_stop_only_while_page_loading() {
        assert!(consume_stop_shortcut(&BrowserShortcut::Stop, true));
        assert!(!consume_stop_shortcut(&BrowserShortcut::Stop, false));
        assert!(!consume_stop_shortcut(&BrowserShortcut::Reload, true));
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
            ("Cmd+A select all", KeyCode::KeyA, cmd, Some("selectAll:")),
            (
                "Ctrl+A select all",
                KeyCode::KeyA,
                mods(true, false, false, false),
                Some("selectAll:"),
            ),
            (
                "Cmd+Backspace delete to start",
                KeyCode::Backspace,
                cmd,
                Some("deleteToBeginningOfLine:"),
            ),
            (
                "Cmd+Delete delete to end",
                KeyCode::Delete,
                cmd,
                Some("deleteToEndOfLine:"),
            ),
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
        let html = chrome_document("", true, false);
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

    #[test]
    fn headless_address_bar_select_all_then_backspace_clears() {
        let mut doc = chrome_test_document();
        let url_id = doc.get_element_by_id("url").expect("url");
        doc.with_text_input(url_id, |mut driver| {
            driver.editor.set_text("https://example.com/path");
            driver.refresh_layout();
        });
        let primary = if cfg!(target_os = "macos") {
            mods(false, false, false, true)
        } else {
            mods(true, false, false, false)
        };
        press_editing_key(&mut doc, KeyCode::KeyA, primary);
        press_editing_key(&mut doc, KeyCode::Backspace, ModifiersState::empty());
        assert_eq!(
            text_value_in_doc(&doc, "url").as_deref(),
            Some(""),
            "select all then Backspace clears the address bar"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn headless_address_bar_cmd_backspace_clears_the_line() {
        let mut doc = chrome_test_document();
        let url_id = doc.get_element_by_id("url").expect("url");
        doc.with_text_input(url_id, |mut driver| {
            driver.editor.set_text("https://example.com/path");
            driver.refresh_layout();
        });
        press_editing_key(
            &mut doc,
            KeyCode::ArrowRight,
            mods(false, false, false, true),
        );
        press_editing_key(
            &mut doc,
            KeyCode::Backspace,
            mods(false, false, false, true),
        );
        assert_eq!(
            text_value_in_doc(&doc, "url").as_deref(),
            Some(""),
            "Cmd+Backspace removes the address bar line"
        );
    }

    fn primary_mods() -> ModifiersState {
        if cfg!(target_os = "macos") {
            mods(false, false, false, true)
        } else {
            mods(true, false, false, false)
        }
    }

    #[derive(Default)]
    struct MemoryClipboard {
        text: std::sync::Mutex<String>,
    }

    impl MemoryClipboard {
        fn text(&self) -> String {
            self.text.lock().expect("clipboard lock").clone()
        }

        fn set(&self, text: &str) {
            *self.text.lock().expect("clipboard lock") = text.to_string();
        }
    }

    impl blitz_traits::shell::ShellProvider for MemoryClipboard {
        fn set_clipboard_text(
            &self,
            text: String,
        ) -> Result<(), blitz_traits::shell::ClipboardError> {
            *self.text.lock().expect("clipboard lock") = text;
            Ok(())
        }

        fn get_clipboard_text(&self) -> Result<String, blitz_traits::shell::ClipboardError> {
            Ok(self.text.lock().expect("clipboard lock").clone())
        }
    }

    #[test]
    fn menu_edit_copy_matches_shortcut() {
        let clip = std::sync::Arc::new(MemoryClipboard::default());
        let mut doc = chrome_test_document();
        doc.set_shell_provider(clip.clone());
        let url_id = doc.get_element_by_id("url").expect("url");
        doc.with_text_input(url_id, |mut driver| {
            driver.editor.set_text("selected text");
            driver.refresh_layout();
        });
        focus_address_field(&mut doc);
        assert_eq!(
            EditAction::from_menu(&crate::menu::MenuCommand::EditCopy),
            Some(EditAction::Copy)
        );
        clip.set("");
        assert!(apply_edit_action_to_doc(&mut doc, EditAction::Copy));
        assert_eq!(clip.text(), "selected text");
        clip.set("");
        assert!(apply_clipboard_shortcut(
            &mut doc,
            KeyCode::KeyC,
            primary_mods()
        ));
        assert_eq!(clip.text(), "selected text");
    }

    #[test]
    fn menu_edit_without_editable_focus_is_not_handled_and_falls_back_to_a_key_press() {
        let mut doc = chrome_test_document();
        doc.clear_focus();
        assert!(!apply_edit_action_to_doc(&mut doc, EditAction::Copy));
        let events = edit_fallback_events(EditAction::Copy, ModifiersState::empty());
        assert_eq!(events.len(), 3);
        assert!(matches!(
            &events[0],
            WindowEvent::ModifiersChanged(m) if m.state() == primary_modifiers()
        ));
        assert!(matches!(
            &events[1],
            WindowEvent::KeyboardInput { event, .. }
                if event.physical_key == PhysicalKey::Code(KeyCode::KeyC)
                    && event.state == ElementState::Pressed
        ));
        assert!(matches!(
            &events[2],
            WindowEvent::ModifiersChanged(m) if m.state().is_empty()
        ));
    }

    #[test]
    fn ctrl_alt_b_and_ctrl_m_yield_to_text_input_off_macos() {
        let ctrl_alt = mods(true, true, false, false);
        let ctrl = mods(true, false, false, false);
        let meta_alt = mods(false, true, false, true);
        let meta = mods(false, false, false, true);
        for (macos, code, m, text, expected) in [
            (false, KeyCode::KeyB, ctrl_alt, true, None),
            (
                false,
                KeyCode::KeyB,
                ctrl_alt,
                false,
                Some(BrowserShortcut::ShowBookmarks),
            ),
            (false, KeyCode::KeyM, ctrl, true, None),
            (
                false,
                KeyCode::KeyM,
                ctrl,
                false,
                Some(BrowserShortcut::MinimizeWindow),
            ),
            (
                true,
                KeyCode::KeyB,
                meta_alt,
                true,
                Some(BrowserShortcut::ShowBookmarks),
            ),
            (
                true,
                KeyCode::KeyM,
                meta,
                true,
                Some(BrowserShortcut::MinimizeWindow),
            ),
        ] {
            assert_eq!(
                browser_shortcut_on(macos, code, m, text, false),
                expected,
                "macos={macos} {code:?} text_focused={text}"
            );
        }
    }

    #[test]
    fn menu_edit_paste_inserts_into_address_bar() {
        let clip = std::sync::Arc::new(MemoryClipboard::default());
        let mut doc = chrome_test_document();
        doc.set_shell_provider(clip.clone());
        let url_id = doc.get_element_by_id("url").expect("url");
        doc.with_text_input(url_id, |mut driver| {
            driver.editor.set_text("");
            driver.refresh_layout();
        });
        focus_address_field(&mut doc);
        clip.set("pasted");
        assert!(apply_edit_action_to_doc(&mut doc, EditAction::Paste));
        assert_eq!(text_value_in_doc(&doc, "url").as_deref(), Some("pasted"));
    }

    #[test]
    fn headless_address_bar_copy_and_paste() {
        let clip = std::sync::Arc::new(MemoryClipboard::default());
        let mut doc = chrome_test_document();
        doc.set_shell_provider(clip.clone());
        let url_id = doc.get_element_by_id("url").expect("url");
        doc.with_text_input(url_id, |mut driver| {
            driver.editor.set_text("https://example.com/path");
            driver.refresh_layout();
        });
        focus_address_field(&mut doc);
        assert!(apply_clipboard_shortcut(
            &mut doc,
            KeyCode::KeyC,
            primary_mods()
        ));
        assert_eq!(clip.text(), "https://example.com/path");
        clip.set("pasted");
        assert!(apply_clipboard_shortcut(
            &mut doc,
            KeyCode::KeyV,
            primary_mods()
        ));
        assert_eq!(text_value_in_doc(&doc, "url").as_deref(), Some("pasted"));
    }

    #[test]
    fn headless_page_input_copy_ignores_unidentified_logical_key() {
        let clip = std::sync::Arc::new(MemoryClipboard::default());
        let html = chrome_document("about:blank", true, false);
        let mut doc = parse::document_from_html(&html, test_document_config(), false);
        doc.set_shell_provider(clip.clone());
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
            let mut inner = sub.inner_mut();
            inner.set_focus_to(field_id);
            inner.with_text_input(field_id, |mut driver| {
                driver.editor.set_text("from the page");
                driver.refresh_layout();
            });
            inner.handle_ui_event(UiEvent::AppleStandardKeybinding(SmolStr::new("selectAll:")));
        }
        let event = test_key_event(
            KeyCode::KeyC,
            ElementState::Pressed,
            WinitKey::Unidentified(winit::keyboard::NativeKey::Unidentified),
        );
        assert_eq!(
            forward_page_iframe_key(&mut doc, &event, primary_mods()),
            Some(true)
        );
        assert_eq!(clip.text(), "from the page");
    }

    #[test]
    fn headless_page_selection_copy() {
        let clip = std::sync::Arc::new(MemoryClipboard::default());
        let mut doc = parse::document_from_html(
            "<!DOCTYPE html><p id=\"t\">Hello page</p>",
            test_document_config(),
            true,
        );
        doc.set_shell_provider(clip.clone());
        layout_doc(&mut doc);
        let id = doc.get_element_by_id("t").expect("paragraph");
        let selected = doc
            .get_node(id)
            .and_then(|node| node.element_data())
            .and_then(|el| el.inline_layout_data.as_ref())
            .map(|layout| layout.text.len())
            .unwrap_or(0);
        doc.set_text_selection(id, 0, id, selected);
        assert!(apply_clipboard_shortcut(
            &mut doc,
            KeyCode::KeyC,
            primary_mods()
        ));
        assert_eq!(clip.text(), "Hello page");
    }

    #[test]
    fn focus_address_field_selects_text_so_backspace_clears_it() {
        let mut doc = chrome_test_document();
        let url_id = doc.get_element_by_id("url").expect("url");
        doc.with_text_input(url_id, |mut driver| {
            driver.editor.set_text("https://example.com/path");
            driver.refresh_layout();
        });
        focus_address_field(&mut doc);
        press_editing_key(&mut doc, KeyCode::Backspace, ModifiersState::empty());
        assert_eq!(text_value_in_doc(&doc, "url").as_deref(), Some(""));
    }

    const PAGE_INPUT_FIXTURE: &str = include_str!("../tests/fixtures/input.html");

    fn chrome_with_page_srcdoc() -> BaseDocument {
        let html = chrome_document("about:blank", true, false);
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
    fn menu_edit_copy_in_page_input_matches_shortcut() {
        let clip = std::sync::Arc::new(MemoryClipboard::default());
        let html = chrome_document("about:blank", true, false);
        let mut doc = parse::document_from_html(&html, test_document_config(), false);
        doc.set_shell_provider(clip.clone());
        layout_doc(&mut doc);
        let page_id = doc.get_element_by_id("page").expect("page iframe");
        {
            let mut m = doc.mutate();
            m.set_attribute(page_id, qual_name!("srcdoc"), PAGE_INPUT_FIXTURE);
        }
        layout_doc(&mut doc);
        doc.set_focus_to(page_id);
        let field_id = doc
            .get_node(page_id)
            .and_then(|n| n.subdoc())
            .and_then(|sub| sub.inner().get_element_by_id("field"))
            .expect("field");
        if let Some(sub) = doc.get_node_mut(page_id).and_then(|n| n.subdoc_mut()) {
            let mut inner = sub.inner_mut();
            inner.set_focus_to(field_id);
            inner.with_text_input(field_id, |mut driver| {
                driver.editor.set_text("from the page");
                driver.refresh_layout();
            });
            inner.handle_ui_event(UiEvent::AppleStandardKeybinding(SmolStr::new("selectAll:")));
        }
        assert!(apply_edit_action_to_doc(&mut doc, EditAction::Copy));
        assert_eq!(clip.text(), "from the page");
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

    fn chrome_with_find_focused() -> BaseDocument {
        let html = chrome_document("about:blank", true, false);
        let mut doc = parse::document_from_html(&html, test_document_config(), false);
        layout_doc(&mut doc);
        crate::chrome::set_find_bar_open(&mut doc, true);
        crate::chrome::focus_find_input(&mut doc);
        doc
    }

    #[test]
    fn inspect_mode_escape_exits_inspect_not_devtools() {
        let html = chrome_document("about:blank", true, false);
        let mut doc = parse::document_from_html(&html, test_document_config(), false);
        layout_doc(&mut doc);
        crate::chrome::apply_devtools_state(
            &mut doc,
            &crate::devtools::DevToolsState {
                open: true,
                tab: crate::devtools::DevToolsTab::Elements,
                ..Default::default()
            },
        );
        layout_doc(&mut doc);
        crate::chrome::focus_devtools_panel(&mut doc);
        let esc = test_key_event(
            KeyCode::Escape,
            ElementState::Pressed,
            WinitKey::Named(winit::keyboard::NamedKey::Escape),
        );
        assert_eq!(
            dispatch_inspect_mode_escape(true, &esc),
            Some(BrowserShortcut::DevToolsInspectCancel)
        );
        assert_eq!(
            dispatch_devtools_field_keys(&doc, &esc, true),
            Some(BrowserShortcut::DevToolsClose)
        );
    }

    #[test]
    fn dispatch_find_field_keys_enter_and_shift_enter() {
        let mut doc = chrome_with_find_focused();
        let enter = test_key_event(
            KeyCode::Enter,
            ElementState::Pressed,
            WinitKey::Named(winit::keyboard::NamedKey::Enter),
        );
        assert_eq!(
            dispatch_find_field_keys(&mut doc, &enter, ModifiersState::empty(), true),
            Some(BrowserShortcut::FindNext)
        );
        let shift = mods(false, false, true, false);
        assert_eq!(
            dispatch_find_field_keys(&mut doc, &enter, shift, true),
            Some(BrowserShortcut::FindPrev)
        );
    }

    #[test]
    fn devtools_toggle_restores_page_input_focus() {
        let mut doc = chrome_with_page_srcdoc();
        let page_id = doc.get_element_by_id("page").expect("page");
        let field_id = doc
            .get_node(page_id)
            .and_then(|n| n.subdoc())
            .and_then(|sub| sub.inner().get_element_by_id("field"))
            .expect("field");
        doc.set_focus_to(page_id);
        if let Some(sub) = doc.get_node_mut(page_id).and_then(|n| n.subdoc_mut()) {
            sub.inner_mut().set_focus_to(field_id);
        }
        let mut session = crate::devtools::DevToolsSession::default();
        session.toggle(&mut doc);
        session.toggle(&mut doc);
        let modifiers = ModifiersState::empty();
        let event = test_key_event(
            char_code('a'),
            ElementState::Pressed,
            WinitKey::Character("a".into()),
        );
        assert_eq!(
            forward_page_iframe_key(&mut doc, &event, modifiers),
            Some(true)
        );
        let value = doc
            .get_node(page_id)
            .and_then(|n| n.subdoc())
            .and_then(|sub| text_value_in_doc(&sub.inner(), "field"))
            .expect("value");
        assert_eq!(value, "a");
    }

    #[test]
    fn dispatch_devtools_console_submit_and_history() {
        let html = chrome_document("about:blank", true, false);
        let mut doc = parse::document_from_html(&html, test_document_config(), false);
        doc.resolve(0.0);
        let state = crate::devtools::DevToolsState {
            open: true,
            tab: crate::devtools::DevToolsTab::Console,
            ..Default::default()
        };
        crate::chrome::apply_devtools_state(&mut doc, &state);
        crate::chrome::focus_devtools_console_input(&mut doc);
        let enter = test_key_event(
            KeyCode::Enter,
            ElementState::Pressed,
            winit::keyboard::Key::Named(winit::keyboard::NamedKey::Enter),
        );
        assert_eq!(
            super::dispatch_devtools_console_keys(&doc, &enter, true, true, false),
            Some(BrowserShortcut::DevToolsConsoleSubmit)
        );
        let up = test_key_event(
            KeyCode::ArrowUp,
            ElementState::Pressed,
            winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowUp),
        );
        assert_eq!(
            super::dispatch_devtools_console_keys(&doc, &up, true, true, false),
            Some(BrowserShortcut::DevToolsConsoleHistoryPrev)
        );
    }

    #[test]
    fn dispatch_devtools_elements_keys_after_row_select_focus() {
        let html = chrome_document("about:blank", true, false);
        let mut doc = parse::document_from_html(&html, test_document_config(), false);
        doc.resolve(0.0);
        let state = crate::devtools::DevToolsState {
            open: true,
            tab: crate::devtools::DevToolsTab::Elements,
            ..Default::default()
        };
        crate::chrome::apply_devtools_state(&mut doc, &state);
        let tree = doc
            .get_element_by_id("devtools-elements-tree")
            .expect("tree");
        let row_html = concat!(
            "<div class=\"devtools-elements-row\">",
            "<a class=\"twist-link\" href=\"#\"><span class=\"twist\"> </span></a>",
            "<a class=\"devtools-elements-row-link\" href=\"glide://devtools-elements-select?node=1\">",
            "<span class=\"label\">x</span></a></div>"
        );
        crate::parse::parse_inner_html_into_mutator(&mut doc.mutate(), tree, row_html, false);
        doc.resolve(0.0);
        let link = doc
            .get_node(tree)
            .and_then(|n| n.children.first().copied())
            .and_then(|row| doc.get_node(row))
            .and_then(|row| row.children.get(1).copied())
            .expect("row link");
        doc.set_focus_to(link);
        assert!(crate::chrome::devtools_elements_tree_focused(&doc));
        let down = test_key_event(
            KeyCode::ArrowDown,
            ElementState::Pressed,
            winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowDown),
        );
        assert_eq!(
            super::dispatch_devtools_elements_keys(&doc, &down, true, true),
            Some(BrowserShortcut::DevToolsElementsDown)
        );
    }

    #[test]
    fn dispatch_devtools_elements_tree_keys() {
        let html = chrome_document("about:blank", true, false);
        let mut doc = parse::document_from_html(&html, test_document_config(), false);
        doc.resolve(0.0);
        let state = crate::devtools::DevToolsState {
            open: true,
            tab: crate::devtools::DevToolsTab::Elements,
            ..Default::default()
        };
        crate::chrome::apply_devtools_state(&mut doc, &state);
        crate::chrome::focus_devtools_elements_tree(&mut doc);
        let up = test_key_event(
            KeyCode::ArrowUp,
            ElementState::Pressed,
            winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowUp),
        );
        assert_eq!(
            super::dispatch_devtools_elements_keys(&doc, &up, true, true),
            Some(BrowserShortcut::DevToolsElementsUp)
        );
        let right = test_key_event(
            KeyCode::ArrowRight,
            ElementState::Pressed,
            winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowRight),
        );
        assert_eq!(
            super::dispatch_devtools_elements_keys(&doc, &right, true, true),
            Some(BrowserShortcut::DevToolsElementsExpand)
        );
    }

    #[test]
    fn dispatch_devtools_console_ignores_enter_while_ime_composing() {
        let html = chrome_document("about:blank", true, false);
        let mut doc = parse::document_from_html(&html, test_document_config(), false);
        doc.resolve(0.0);
        let state = crate::devtools::DevToolsState {
            open: true,
            tab: crate::devtools::DevToolsTab::Console,
            ..Default::default()
        };
        crate::chrome::apply_devtools_state(&mut doc, &state);
        crate::chrome::focus_devtools_console_input(&mut doc);
        let enter = test_key_event(
            KeyCode::Enter,
            ElementState::Pressed,
            winit::keyboard::Key::Named(winit::keyboard::NamedKey::Enter),
        );
        assert_eq!(
            super::dispatch_devtools_console_keys(&doc, &enter, true, true, true),
            None
        );
    }

    #[test]
    fn dispatch_devtools_esc_closes_panel() {
        let html = chrome_document("about:blank", true, false);
        let mut doc = parse::document_from_html(&html, test_document_config(), false);
        doc.resolve(0.0);
        let state = crate::devtools::DevToolsState {
            open: true,
            ..Default::default()
        };
        crate::chrome::apply_devtools_state(&mut doc, &state);
        crate::chrome::focus_devtools_panel(&mut doc);
        let esc = test_key_event(
            KeyCode::Escape,
            ElementState::Pressed,
            winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape),
        );
        assert_eq!(
            super::dispatch_devtools_field_keys(&doc, &esc, true),
            Some(BrowserShortcut::DevToolsClose)
        );
    }

    #[test]
    fn dispatch_find_field_keys_esc_closes_find() {
        let mut doc = chrome_with_find_focused();
        let esc = test_key_event(
            KeyCode::Escape,
            ElementState::Pressed,
            WinitKey::Named(winit::keyboard::NamedKey::Escape),
        );
        assert_eq!(
            dispatch_find_field_keys(&mut doc, &esc, ModifiersState::empty(), true),
            Some(BrowserShortcut::FindClose)
        );
        assert!(
            browser_shortcut(KeyCode::Escape, ModifiersState::empty(), true, true).is_none(),
            "Esc in find field must not become Stop"
        );
    }

    #[test]
    fn dispatch_find_field_keys_ignores_closed_bar() {
        let mut doc = chrome_with_find_focused();
        let enter = test_key_event(
            KeyCode::Enter,
            ElementState::Pressed,
            WinitKey::Named(winit::keyboard::NamedKey::Enter),
        );
        assert!(
            dispatch_find_field_keys(&mut doc, &enter, ModifiersState::empty(), false).is_none()
        );
    }
}
