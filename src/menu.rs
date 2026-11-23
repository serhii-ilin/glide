//! Platform-neutral menu model shared by the native macOS menu bar and the in-window menu bar.

use std::sync::{Arc, Mutex};

use crate::chrome::{ChromeViewState, MenuId, NavCommand};
use crate::keyboard::BrowserShortcut;
use crate::tabs::TabId;
use winit::keyboard::KeyCode;
use winit::keyboard::ModifiersState;

/// Queue from the macOS menu event handler to the UI thread.
#[derive(Clone, Default)]
pub struct MenuCommandQueue {
    inner: Arc<Mutex<Vec<MenuCommand>>>,
}

impl MenuCommandQueue {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<MenuCommand>> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn push(&self, command: MenuCommand) {
        self.lock().push(command);
    }

    pub fn take(&self) -> Vec<MenuCommand> {
        std::mem::take(&mut *self.lock())
    }
}

/// Fields of a `menu.native` warning, in the shared diagnostics schema.
pub fn native_menu_warning_fields(error_kind: &str, detail: String) -> Vec<(&'static str, String)> {
    vec![
        ("feature", "F-049".into()),
        ("error_kind", error_kind.into()),
        ("detail", detail),
        (
            "hint",
            "Commands remain available through keyboard shortcuts; see F-049 in docs/features.md."
                .into(),
        ),
    ]
}

/// Where the menu model is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuPlatform {
    MacNative,
    InWindow,
}

/// Top-level menu in the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuBarMenuId {
    GlideApp,
    File,
    Edit,
    View,
    History,
    Bookmarks,
    Develop,
    Window,
    Tabs,
    Help,
}

impl MenuBarMenuId {
    pub fn title(self) -> &'static str {
        match self {
            Self::GlideApp => "Glide",
            Self::File => "File",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::History => "History",
            Self::Bookmarks => "Bookmarks",
            Self::Develop => "Develop",
            Self::Window => "Window",
            Self::Tabs => "Tabs",
            Self::Help => "Help",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::GlideApp => "glide",
            Self::File => "file",
            Self::Edit => "edit",
            Self::View => "view",
            Self::History => "history",
            Self::Bookmarks => "bookmarks",
            Self::Develop => "develop",
            Self::Window => "window",
            Self::Tabs => "tabs",
            Self::Help => "help",
        }
    }

    pub fn chrome_open_menu(self) -> Option<MenuId> {
        match self {
            Self::File => Some(MenuId::File),
            Self::Edit => Some(MenuId::Edit),
            Self::View => Some(MenuId::View),
            Self::History => Some(MenuId::History),
            Self::Bookmarks => Some(MenuId::Bookmarks),
            Self::Tabs => Some(MenuId::Tabs),
            Self::Help => Some(MenuId::Help),
            Self::GlideApp | Self::Develop | Self::Window => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuSpec {
    pub id: MenuBarMenuId,
    pub title: &'static str,
    pub items: Vec<MenuEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    Item(MenuItemSpec),
    Separator,
    Heading(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItemSpec {
    pub command: MenuCommand,
    pub label: String,
    pub shortcut: Option<Shortcut>,
    pub enabled: bool,
    pub checked: bool,
}

/// Every action a menu row can trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuCommand {
    Back,
    Forward,
    Reload,
    Stop,
    NewTab,
    CloseActiveTab,
    CloseWindow,
    DuplicateTab,
    FocusAddressBar,
    Find,
    FindNext,
    FindPrev,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ToggleJs,
    ToggleBookmark,
    ShowBookmarks,
    ToggleDevTools,
    DevToolsInspectToggle,
    NextTab,
    PrevTab,
    SelectTab(TabId),
    Quit,
    ShowHelp,
    EditUndo,
    EditRedo,
    EditCut,
    EditCopy,
    EditPaste,
    EditSelectAll,
    /// Handled by AppKit (hide, minimize, about, …); not routed through Glide.
    Native(NativeItem),
}

/// AppKit-provided menu items that muda exposes as predefined items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeItem {
    About,
    Services,
    Hide,
    HideOthers,
    ShowAll,
    Minimize,
    Zoom,
    FullScreen,
    BringAllToFront,
}

impl MenuCommand {
    /// Whether the native item carries a checkmark that can change (JS toggle, active tab).
    pub fn is_checkable(&self) -> bool {
        matches!(self, Self::ToggleJs | Self::SelectTab(_))
    }

    pub fn id(&self) -> Option<String> {
        match self {
            Self::Back => Some("glide://back".into()),
            Self::Forward => Some("glide://forward".into()),
            Self::Reload => Some("glide://reload".into()),
            Self::Stop => Some("glide://stop".into()),
            Self::NewTab => Some("glide://new-tab".into()),
            Self::CloseActiveTab => Some("glide://close-active-tab".into()),
            Self::CloseWindow => Some("glide://close-window".into()),
            Self::DuplicateTab => Some("glide://duplicate-tab".into()),
            Self::FocusAddressBar => Some("glide://focus-address".into()),
            Self::Find => Some("glide://find".into()),
            Self::FindNext => Some("glide://find-next".into()),
            Self::FindPrev => Some("glide://find-prev".into()),
            Self::ZoomIn => Some("glide://zoom-in".into()),
            Self::ZoomOut => Some("glide://zoom-out".into()),
            Self::ZoomReset => Some("glide://zoom-reset".into()),
            Self::ToggleJs => Some("glide://toggle-js".into()),
            Self::ToggleBookmark => Some("glide://bookmark".into()),
            Self::ShowBookmarks => Some("glide://bookmarks".into()),
            Self::ToggleDevTools => Some("glide://devtools".into()),
            Self::DevToolsInspectToggle => Some("glide://devtools-inspect-toggle".into()),
            Self::NextTab => Some("glide://next-tab".into()),
            Self::PrevTab => Some("glide://prev-tab".into()),
            Self::SelectTab(id) => Some(format!("glide://select-tab?id={id}")),
            Self::Quit => Some("glide://quit".into()),
            Self::ShowHelp => Some("glide://help".into()),
            Self::EditUndo => Some("glide://edit-undo".into()),
            Self::EditRedo => Some("glide://edit-redo".into()),
            Self::EditCut => Some("glide://edit-cut".into()),
            Self::EditCopy => Some("glide://edit-copy".into()),
            Self::EditPaste => Some("glide://edit-paste".into()),
            Self::EditSelectAll => Some("glide://edit-select-all".into()),
            Self::Native(_) => None,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::from_glide_host(id.strip_prefix("glide://")?)
    }

    fn from_glide_host(rest: &str) -> Option<Self> {
        let (host, query) = rest.split_once('?').map_or((rest, ""), |(h, q)| (h, q));
        match host {
            "back" => Some(Self::Back),
            "forward" => Some(Self::Forward),
            "reload" => Some(Self::Reload),
            "stop" => Some(Self::Stop),
            "new-tab" => Some(Self::NewTab),
            "close-active-tab" => Some(Self::CloseActiveTab),
            "close-window" => Some(Self::CloseWindow),
            "duplicate-tab" => Some(Self::DuplicateTab),
            "focus-address" => Some(Self::FocusAddressBar),
            "find" => Some(Self::Find),
            "find-next" => Some(Self::FindNext),
            "find-prev" => Some(Self::FindPrev),
            "zoom-in" => Some(Self::ZoomIn),
            "zoom-out" => Some(Self::ZoomOut),
            "zoom-reset" => Some(Self::ZoomReset),
            "toggle-js" => Some(Self::ToggleJs),
            "bookmark" => Some(Self::ToggleBookmark),
            "bookmarks" => Some(Self::ShowBookmarks),
            "devtools" => Some(Self::ToggleDevTools),
            "devtools-inspect-toggle" => Some(Self::DevToolsInspectToggle),
            "next-tab" => Some(Self::NextTab),
            "prev-tab" => Some(Self::PrevTab),
            "select-tab" => query
                .strip_prefix("id=")
                .and_then(|id| id.parse::<TabId>().ok())
                .map(Self::SelectTab),
            "quit" => Some(Self::Quit),
            "help" => Some(Self::ShowHelp),
            "edit-undo" => Some(Self::EditUndo),
            "edit-redo" => Some(Self::EditRedo),
            "edit-cut" => Some(Self::EditCut),
            "edit-copy" => Some(Self::EditCopy),
            "edit-paste" => Some(Self::EditPaste),
            "edit-select-all" => Some(Self::EditSelectAll),
            _ => None,
        }
    }

    pub fn from_nav_command(command: NavCommand) -> Option<Self> {
        match command {
            NavCommand::Back => Some(Self::Back),
            NavCommand::Forward => Some(Self::Forward),
            NavCommand::Reload => Some(Self::Reload),
            NavCommand::Stop => Some(Self::Stop),
            NavCommand::NewTab => Some(Self::NewTab),
            NavCommand::CloseActiveTab => Some(Self::CloseActiveTab),
            NavCommand::CloseWindow => Some(Self::CloseWindow),
            NavCommand::DuplicateTab => Some(Self::DuplicateTab),
            NavCommand::Find => Some(Self::Find),
            NavCommand::FindNext => Some(Self::FindNext),
            NavCommand::FindPrev => Some(Self::FindPrev),
            NavCommand::ZoomIn => Some(Self::ZoomIn),
            NavCommand::ZoomOut => Some(Self::ZoomOut),
            NavCommand::ZoomReset => Some(Self::ZoomReset),
            NavCommand::ToggleJs => Some(Self::ToggleJs),
            NavCommand::ToggleBookmark => Some(Self::ToggleBookmark),
            NavCommand::DevToolsToggle => Some(Self::ToggleDevTools),
            NavCommand::DevToolsInspectToggle => Some(Self::DevToolsInspectToggle),
            NavCommand::NextTab => Some(Self::NextTab),
            NavCommand::PrevTab => Some(Self::PrevTab),
            NavCommand::SelectTab(id) => Some(Self::SelectTab(id)),
            NavCommand::Go(url) if url == "glide://bookmarks" => Some(Self::ShowBookmarks),
            NavCommand::Go(url) if url == "glide://help" => Some(Self::ShowHelp),
            _ => None,
        }
    }

    pub fn to_browser_shortcut(&self) -> Option<BrowserShortcut> {
        match self {
            Self::Back => Some(BrowserShortcut::Back),
            Self::Forward => Some(BrowserShortcut::Forward),
            Self::Reload => Some(BrowserShortcut::Reload),
            Self::Stop => Some(BrowserShortcut::Stop),
            Self::NewTab => Some(BrowserShortcut::NewTab),
            Self::CloseActiveTab => Some(BrowserShortcut::CloseTab),
            Self::FocusAddressBar => Some(BrowserShortcut::FocusAddressBar),
            Self::Find => Some(BrowserShortcut::Find),
            Self::FindNext => Some(BrowserShortcut::FindNext),
            Self::FindPrev => Some(BrowserShortcut::FindPrev),
            Self::ZoomIn => Some(BrowserShortcut::ZoomIn),
            Self::ZoomOut => Some(BrowserShortcut::ZoomOut),
            Self::ZoomReset => Some(BrowserShortcut::ZoomReset),
            Self::ToggleJs => Some(BrowserShortcut::ToggleJs),
            Self::ToggleBookmark => Some(BrowserShortcut::ToggleBookmark),
            Self::ShowBookmarks => Some(BrowserShortcut::ShowBookmarks),
            Self::ToggleDevTools => Some(BrowserShortcut::ToggleDevTools),
            Self::DevToolsInspectToggle => Some(BrowserShortcut::DevToolsInspectToggle),
            Self::NextTab => Some(BrowserShortcut::NextTab),
            Self::PrevTab => Some(BrowserShortcut::PreviousTab),
            Self::Quit => Some(BrowserShortcut::Quit),
            Self::ShowHelp => Some(BrowserShortcut::ShowHelp),
            Self::CloseWindow => Some(BrowserShortcut::CloseWindow),
            Self::DuplicateTab => Some(BrowserShortcut::DuplicateTab),
            Self::SelectTab(_)
            | Self::EditUndo
            | Self::EditRedo
            | Self::EditCut
            | Self::EditCopy
            | Self::EditPaste
            | Self::EditSelectAll
            | Self::Native(_) => None,
        }
    }

    pub fn to_nav_command(&self) -> Option<NavCommand> {
        match self {
            Self::Back => Some(NavCommand::Back),
            Self::Forward => Some(NavCommand::Forward),
            Self::Reload => Some(NavCommand::Reload),
            Self::Stop => Some(NavCommand::Stop),
            Self::NewTab => Some(NavCommand::NewTab),
            Self::CloseActiveTab => Some(NavCommand::CloseActiveTab),
            Self::CloseWindow => Some(NavCommand::CloseWindow),
            Self::DuplicateTab => Some(NavCommand::DuplicateTab),
            Self::Find => Some(NavCommand::Find),
            Self::FindNext => Some(NavCommand::FindNext),
            Self::FindPrev => Some(NavCommand::FindPrev),
            Self::ZoomIn => Some(NavCommand::ZoomIn),
            Self::ZoomOut => Some(NavCommand::ZoomOut),
            Self::ZoomReset => Some(NavCommand::ZoomReset),
            Self::ToggleJs => Some(NavCommand::ToggleJs),
            Self::ToggleBookmark => Some(NavCommand::ToggleBookmark),
            Self::ToggleDevTools => Some(NavCommand::DevToolsToggle),
            Self::DevToolsInspectToggle => Some(NavCommand::DevToolsInspectToggle),
            Self::NextTab => Some(NavCommand::NextTab),
            Self::PrevTab => Some(NavCommand::PrevTab),
            Self::SelectTab(id) => Some(NavCommand::SelectTab(*id)),
            Self::ShowBookmarks => Some(NavCommand::Go("glide://bookmarks".into())),
            Self::ShowHelp => Some(NavCommand::Go("glide://help".into())),
            _ => None,
        }
    }
}

/// Physical key plus modifier flags for menu display and accelerator agreement tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub key: ShortcutKey,
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub cmd: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutKey {
    Char(char),
    Digit(u8),
    F12,
    Escape,
    BracketLeft,
    BracketRight,
    Period,
    Plus,
    Minus,
}

impl Shortcut {
    pub fn display_macos(&self) -> String {
        let mut out = String::new();
        if self.ctrl {
            out.push('⌃');
        }
        if self.alt {
            out.push('⌥');
        }
        if self.shift {
            out.push('⇧');
        }
        if self.cmd {
            out.push('⌘');
        }
        out.push_str(&self.key.display_macos());
        out
    }

    pub fn to_key_code(&self) -> Option<KeyCode> {
        self.key.to_key_code()
    }

    pub fn to_modifiers_state(&self) -> ModifiersState {
        let mut m = ModifiersState::empty();
        if self.ctrl {
            m |= ModifiersState::CONTROL;
        }
        if self.alt {
            m |= ModifiersState::ALT;
        }
        if self.shift {
            m |= ModifiersState::SHIFT;
        }
        if self.cmd {
            m |= ModifiersState::META;
        }
        m
    }
}

impl ShortcutKey {
    fn display_macos(self) -> String {
        match self {
            Self::Char(c) => c.to_uppercase().collect(),
            Self::Digit(d) => d.to_string(),
            Self::F12 => "F12".into(),
            Self::Escape => "⎋".into(),
            Self::BracketLeft => "[".into(),
            Self::BracketRight => "]".into(),
            Self::Period => ".".into(),
            Self::Plus => "+".into(),
            Self::Minus => "−".into(),
        }
    }

    fn to_key_code(self) -> Option<KeyCode> {
        match self {
            Self::Char(c) => char_to_key_code(c),
            Self::Digit(d) => match d {
                0 => Some(KeyCode::Digit0),
                1 => Some(KeyCode::Digit1),
                2 => Some(KeyCode::Digit2),
                3 => Some(KeyCode::Digit3),
                4 => Some(KeyCode::Digit4),
                5 => Some(KeyCode::Digit5),
                6 => Some(KeyCode::Digit6),
                7 => Some(KeyCode::Digit7),
                8 => Some(KeyCode::Digit8),
                9 => Some(KeyCode::Digit9),
                _ => None,
            },
            Self::F12 => Some(KeyCode::F12),
            Self::Escape => Some(KeyCode::Escape),
            Self::BracketLeft => Some(KeyCode::BracketLeft),
            Self::BracketRight => Some(KeyCode::BracketRight),
            Self::Period => Some(KeyCode::Period),
            Self::Plus => Some(KeyCode::Equal),
            Self::Minus => Some(KeyCode::Minus),
        }
    }
}

fn char_to_key_code(ch: char) -> Option<KeyCode> {
    let lower = ch.to_ascii_lowercase();
    match lower {
        'a' => Some(KeyCode::KeyA),
        'b' => Some(KeyCode::KeyB),
        'c' => Some(KeyCode::KeyC),
        'd' => Some(KeyCode::KeyD),
        'f' => Some(KeyCode::KeyF),
        'g' => Some(KeyCode::KeyG),
        'h' => Some(KeyCode::KeyH),
        'i' => Some(KeyCode::KeyI),
        'j' => Some(KeyCode::KeyJ),
        'l' => Some(KeyCode::KeyL),
        'm' => Some(KeyCode::KeyM),
        'q' => Some(KeyCode::KeyQ),
        'r' => Some(KeyCode::KeyR),
        't' => Some(KeyCode::KeyT),
        'w' => Some(KeyCode::KeyW),
        'x' => Some(KeyCode::KeyX),
        'v' => Some(KeyCode::KeyV),
        'z' => Some(KeyCode::KeyZ),
        '/' => Some(KeyCode::Slash),
        _ => None,
    }
}

fn cmd(char_key: char) -> Shortcut {
    Shortcut {
        key: ShortcutKey::Char(char_key),
        shift: false,
        alt: false,
        ctrl: false,
        cmd: true,
    }
}

fn cmd_shift(char_key: char) -> Shortcut {
    Shortcut {
        key: ShortcutKey::Char(char_key),
        shift: true,
        alt: false,
        ctrl: false,
        cmd: true,
    }
}

fn cmd_alt(char_key: char) -> Shortcut {
    Shortcut {
        key: ShortcutKey::Char(char_key),
        shift: false,
        alt: true,
        ctrl: false,
        cmd: true,
    }
}

fn item(command: MenuCommand, label: &str, shortcut: Option<Shortcut>, enabled: bool) -> MenuEntry {
    MenuEntry::Item(MenuItemSpec {
        command,
        label: label.to_string(),
        shortcut,
        enabled,
        checked: false,
    })
}

fn check_item(
    command: MenuCommand,
    label: &str,
    shortcut: Option<Shortcut>,
    enabled: bool,
    checked: bool,
) -> MenuEntry {
    MenuEntry::Item(MenuItemSpec {
        command,
        label: label.to_string(),
        shortcut,
        enabled,
        checked,
    })
}

fn sep() -> MenuEntry {
    MenuEntry::Separator
}

fn heading(label: &str) -> MenuEntry {
    MenuEntry::Heading(label.to_string())
}

/// Build the full menu tree for the given platform and view state.
pub fn menu_model(view: &ChromeViewState, platform: MenuPlatform) -> Vec<MenuSpec> {
    match platform {
        MenuPlatform::MacNative => mac_native_model(view),
        MenuPlatform::InWindow => in_window_model(view),
    }
}

fn single_tab(view: &ChromeViewState) -> bool {
    view.tabs.len() <= 1
}

fn mac_native_model(view: &ChromeViewState) -> Vec<MenuSpec> {
    let st = single_tab(view);
    let bookmark_label = if view.bookmarked {
        "Remove Bookmark"
    } else {
        "Add Bookmark"
    };
    let devtools_label = if view.devtools_open {
        "Hide Web Inspector"
    } else {
        "Show Web Inspector"
    };
    let mut window_items = vec![
        item(
            MenuCommand::Native(NativeItem::Minimize),
            "Minimize",
            Some(cmd('m')),
            true,
        ),
        item(MenuCommand::Native(NativeItem::Zoom), "Zoom", None, true),
        sep(),
        item(
            MenuCommand::PrevTab,
            "Show Previous Tab",
            Some(Shortcut {
                key: ShortcutKey::BracketLeft,
                shift: true,
                alt: false,
                ctrl: false,
                cmd: true,
            }),
            !st,
        ),
        item(
            MenuCommand::NextTab,
            "Show Next Tab",
            Some(Shortcut {
                key: ShortcutKey::BracketRight,
                shift: true,
                alt: false,
                ctrl: false,
                cmd: true,
            }),
            !st,
        ),
        sep(),
    ];
    for (index, tab) in view.tabs.iter().enumerate() {
        let title = if tab.title.trim().is_empty() {
            format!("{}: New tab", index + 1)
        } else {
            format!("{}: {}", index + 1, tab.title)
        };
        let active = Some(index) == view.active_index;
        window_items.push(check_item(
            MenuCommand::SelectTab(tab.id),
            &title,
            None,
            true,
            active,
        ));
    }
    window_items.push(sep());
    window_items.push(item(
        MenuCommand::Native(NativeItem::BringAllToFront),
        "Bring All to Front",
        None,
        true,
    ));

    vec![
        MenuSpec {
            id: MenuBarMenuId::GlideApp,
            title: "Glide",
            items: vec![
                item(
                    MenuCommand::Native(NativeItem::About),
                    "About Glide",
                    None,
                    true,
                ),
                sep(),
                item(
                    MenuCommand::Native(NativeItem::Services),
                    "Services",
                    None,
                    true,
                ),
                sep(),
                item(
                    MenuCommand::Native(NativeItem::Hide),
                    "Hide Glide",
                    Some(cmd('h')),
                    true,
                ),
                item(
                    MenuCommand::Native(NativeItem::HideOthers),
                    "Hide Others",
                    Some(cmd_alt('h')),
                    true,
                ),
                item(
                    MenuCommand::Native(NativeItem::ShowAll),
                    "Show All",
                    None,
                    true,
                ),
                sep(),
                item(MenuCommand::Quit, "Quit Glide", Some(cmd('q')), true),
            ],
        },
        MenuSpec {
            id: MenuBarMenuId::File,
            title: "File",
            items: vec![
                item(MenuCommand::NewTab, "New Tab", Some(cmd('t')), true),
                item(
                    MenuCommand::FocusAddressBar,
                    "Open Location…",
                    Some(cmd('l')),
                    true,
                ),
                sep(),
                item(
                    MenuCommand::CloseWindow,
                    "Close Window",
                    Some(cmd_shift('w')),
                    true,
                ),
                item(
                    MenuCommand::CloseActiveTab,
                    "Close Tab",
                    Some(cmd('w')),
                    // AppKit may swallow the chord of a disabled item, so the last tab
                    // stays enabled; the shortcut path closes the window then.
                    true,
                ),
                sep(),
                item(MenuCommand::DuplicateTab, "Duplicate Tab", None, true),
            ],
        },
        MenuSpec {
            id: MenuBarMenuId::Edit,
            title: "Edit",
            items: vec![
                item(MenuCommand::EditUndo, "Undo", Some(cmd('z')), false),
                item(MenuCommand::EditRedo, "Redo", Some(cmd_shift('z')), false),
                sep(),
                item(MenuCommand::EditCut, "Cut", Some(cmd('x')), true),
                item(MenuCommand::EditCopy, "Copy", Some(cmd('c')), true),
                item(MenuCommand::EditPaste, "Paste", Some(cmd('v')), true),
                item(
                    MenuCommand::EditSelectAll,
                    "Select All",
                    Some(cmd('a')),
                    true,
                ),
                sep(),
                item(MenuCommand::Find, "Find…", Some(cmd('f')), true),
                item(
                    MenuCommand::FindNext,
                    "Find Next",
                    Some(cmd('g')),
                    view.find_bar_open,
                ),
                item(
                    MenuCommand::FindPrev,
                    "Find Previous",
                    Some(cmd_shift('g')),
                    view.find_bar_open,
                ),
            ],
        },
        MenuSpec {
            id: MenuBarMenuId::View,
            title: "View",
            items: vec![
                item(
                    MenuCommand::Stop,
                    "Stop",
                    Some(Shortcut {
                        key: ShortcutKey::Period,
                        shift: false,
                        alt: false,
                        ctrl: false,
                        cmd: true,
                    }),
                    view.page_loading,
                ),
                item(MenuCommand::Reload, "Reload Page", Some(cmd('r')), true),
                sep(),
                item(
                    MenuCommand::ZoomReset,
                    "Actual Size",
                    Some(Shortcut {
                        key: ShortcutKey::Digit(0),
                        shift: false,
                        alt: false,
                        ctrl: false,
                        cmd: true,
                    }),
                    true,
                ),
                item(
                    MenuCommand::ZoomIn,
                    "Zoom In",
                    Some(Shortcut {
                        key: ShortcutKey::Plus,
                        shift: false,
                        alt: false,
                        ctrl: false,
                        cmd: true,
                    }),
                    true,
                ),
                item(
                    MenuCommand::ZoomOut,
                    "Zoom Out",
                    Some(Shortcut {
                        key: ShortcutKey::Minus,
                        shift: false,
                        alt: false,
                        ctrl: false,
                        cmd: true,
                    }),
                    true,
                ),
                sep(),
                item(
                    MenuCommand::Native(NativeItem::FullScreen),
                    "Enter Full Screen",
                    Some(Shortcut {
                        key: ShortcutKey::Char('f'),
                        shift: false,
                        alt: false,
                        ctrl: true,
                        cmd: true,
                    }),
                    true,
                ),
            ],
        },
        MenuSpec {
            id: MenuBarMenuId::History,
            title: "History",
            items: vec![
                item(
                    MenuCommand::Back,
                    "Back",
                    Some(Shortcut {
                        key: ShortcutKey::BracketLeft,
                        shift: false,
                        alt: false,
                        ctrl: false,
                        cmd: true,
                    }),
                    view.can_go_back,
                ),
                item(
                    MenuCommand::Forward,
                    "Forward",
                    Some(Shortcut {
                        key: ShortcutKey::BracketRight,
                        shift: false,
                        alt: false,
                        ctrl: false,
                        cmd: true,
                    }),
                    view.can_go_forward,
                ),
            ],
        },
        MenuSpec {
            id: MenuBarMenuId::Bookmarks,
            title: "Bookmarks",
            items: vec![
                item(
                    MenuCommand::ShowBookmarks,
                    "Show Bookmarks",
                    Some(cmd_alt('b')),
                    true,
                ),
                sep(),
                item(
                    MenuCommand::ToggleBookmark,
                    bookmark_label,
                    Some(cmd('d')),
                    true,
                ),
            ],
        },
        MenuSpec {
            id: MenuBarMenuId::Develop,
            title: "Develop",
            items: vec![
                item(
                    MenuCommand::ToggleDevTools,
                    devtools_label,
                    Some(cmd_alt('i')),
                    true,
                ),
                item(
                    MenuCommand::DevToolsInspectToggle,
                    "Start Element Selection",
                    Some(cmd_shift('c')),
                    true,
                ),
                sep(),
                check_item(
                    MenuCommand::ToggleJs,
                    "Disable JavaScript",
                    Some(cmd_shift('j')),
                    true,
                    !view.js_enabled,
                ),
            ],
        },
        MenuSpec {
            id: MenuBarMenuId::Window,
            title: "Window",
            items: window_items,
        },
        MenuSpec {
            id: MenuBarMenuId::Help,
            title: "Help",
            items: vec![item(MenuCommand::ShowHelp, "Glide Help", None, true)],
        },
    ]
}

fn in_window_model(view: &ChromeViewState) -> Vec<MenuSpec> {
    let st = single_tab(view);
    let js_label = if view.js_enabled {
        "Turn JavaScript Off"
    } else {
        "Turn JavaScript On"
    };
    let bookmark_label = if view.bookmarked {
        "Remove Bookmark"
    } else {
        "Add Bookmark"
    };

    let mut tabs_items_fixed = vec![
        item(MenuCommand::NewTab, "New Tab", Some(cmd('t')), true),
        item(MenuCommand::DuplicateTab, "Duplicate Tab", None, true),
        sep(),
        item(MenuCommand::NextTab, "Next Tab", None, !st),
        item(MenuCommand::PrevTab, "Previous Tab", None, !st),
    ];
    if view.tabs.len() > 1 {
        tabs_items_fixed.push(sep());
        tabs_items_fixed.push(heading("Open Tabs"));
        for (index, tab) in view.tabs.iter().enumerate() {
            let title = if tab.title.trim().is_empty() {
                format!("{}: New tab", index + 1)
            } else {
                format!("{}: {}", index + 1, tab.title)
            };
            let active = Some(index) == view.active_index;
            tabs_items_fixed.push(MenuEntry::Item(MenuItemSpec {
                command: MenuCommand::SelectTab(tab.id),
                label: title,
                shortcut: None,
                enabled: true,
                checked: active,
            }));
        }
    }
    vec![
        MenuSpec {
            id: MenuBarMenuId::File,
            title: "File",
            items: vec![
                item(MenuCommand::NewTab, "New Tab", Some(cmd('t')), true),
                sep(),
                item(
                    MenuCommand::CloseActiveTab,
                    "Close Tab",
                    Some(cmd('w')),
                    !st,
                ),
                item(
                    MenuCommand::CloseWindow,
                    "Close Window",
                    Some(cmd_shift('w')),
                    true,
                ),
            ],
        },
        MenuSpec {
            id: MenuBarMenuId::Edit,
            title: "Edit",
            items: vec![
                item(MenuCommand::Find, "Find", Some(cmd('f')), true),
                sep(),
                heading("Editing"),
                item(MenuCommand::EditUndo, "Undo", Some(cmd('z')), false),
                item(MenuCommand::EditRedo, "Redo", Some(cmd_shift('z')), false),
                item(MenuCommand::EditCut, "Cut", Some(cmd('x')), false),
                item(MenuCommand::EditCopy, "Copy", Some(cmd('c')), false),
                item(MenuCommand::EditPaste, "Paste", Some(cmd('v')), false),
                item(
                    MenuCommand::EditSelectAll,
                    "Select All",
                    Some(cmd('a')),
                    false,
                ),
            ],
        },
        MenuSpec {
            id: MenuBarMenuId::View,
            title: "View",
            items: vec![
                item(
                    MenuCommand::ZoomIn,
                    "Zoom In",
                    Some(Shortcut {
                        key: ShortcutKey::Plus,
                        shift: false,
                        alt: false,
                        ctrl: false,
                        cmd: true,
                    }),
                    true,
                ),
                item(
                    MenuCommand::ZoomOut,
                    "Zoom Out",
                    Some(Shortcut {
                        key: ShortcutKey::Minus,
                        shift: false,
                        alt: false,
                        ctrl: false,
                        cmd: true,
                    }),
                    true,
                ),
                item(
                    MenuCommand::ZoomReset,
                    "Reset Zoom",
                    Some(Shortcut {
                        key: ShortcutKey::Digit(0),
                        shift: false,
                        alt: false,
                        ctrl: false,
                        cmd: true,
                    }),
                    true,
                ),
                sep(),
                item(MenuCommand::Reload, "Reload Page", Some(cmd('r')), true),
                sep(),
                item(
                    MenuCommand::ToggleDevTools,
                    "Toggle Developer Tools",
                    Some(Shortcut {
                        key: ShortcutKey::F12,
                        shift: false,
                        alt: false,
                        ctrl: false,
                        cmd: false,
                    }),
                    true,
                ),
                sep(),
                item(MenuCommand::ToggleJs, js_label, Some(cmd_shift('j')), true),
            ],
        },
        MenuSpec {
            id: MenuBarMenuId::History,
            title: "History",
            items: {
                let mut items = vec![
                    item(
                        MenuCommand::Back,
                        "Back",
                        Some(Shortcut {
                            key: ShortcutKey::BracketLeft,
                            shift: false,
                            alt: false,
                            ctrl: false,
                            cmd: true,
                        }),
                        view.can_go_back,
                    ),
                    item(
                        MenuCommand::Forward,
                        "Forward",
                        Some(Shortcut {
                            key: ShortcutKey::BracketRight,
                            shift: false,
                            alt: false,
                            ctrl: false,
                            cmd: true,
                        }),
                        view.can_go_forward,
                    ),
                    sep(),
                ];
                if view.page_loading {
                    items.push(item(
                        MenuCommand::Stop,
                        "Stop Loading",
                        Some(Shortcut {
                            key: ShortcutKey::Escape,
                            shift: false,
                            alt: false,
                            ctrl: false,
                            cmd: false,
                        }),
                        true,
                    ));
                } else {
                    items.push(item(
                        MenuCommand::Reload,
                        "Reload Page",
                        Some(cmd('r')),
                        true,
                    ));
                }
                items
            },
        },
        MenuSpec {
            id: MenuBarMenuId::Bookmarks,
            title: "Bookmarks",
            items: vec![
                item(
                    MenuCommand::ToggleBookmark,
                    bookmark_label,
                    Some(cmd('d')),
                    true,
                ),
                sep(),
                item(
                    MenuCommand::ShowBookmarks,
                    "Show Bookmarks",
                    Some(cmd_alt('b')),
                    true,
                ),
            ],
        },
        MenuSpec {
            id: MenuBarMenuId::Tabs,
            title: "Tabs",
            items: tabs_items_fixed,
        },
        MenuSpec {
            id: MenuBarMenuId::Help,
            title: "Help",
            items: vec![
                item(
                    MenuCommand::ShowHelp,
                    "Keyboard Shortcuts",
                    Some(Shortcut {
                        key: ShortcutKey::Char('/'),
                        shift: true,
                        alt: false,
                        ctrl: false,
                        cmd: true,
                    }),
                    true,
                ),
                sep(),
                heading("Glide"),
                item(
                    MenuCommand::Native(NativeItem::About),
                    "About Glide",
                    None,
                    false,
                ),
            ],
        },
    ]
}

/// Items with shortcuts that should match [`keyboard::browser_shortcut`].
pub fn actionable_menu_items(platform: MenuPlatform) -> Vec<(String, MenuCommand, Shortcut)> {
    let view = ChromeViewState {
        can_go_back: true,
        can_go_forward: true,
        page_loading: true,
        js_enabled: true,
        bookmarked: false,
        devtools_open: true,
        tabs: vec![
            crate::chrome::TabView {
                id: 1,
                title: "A".into(),
                ..Default::default()
            },
            crate::chrome::TabView {
                id: 2,
                title: "B".into(),
                ..Default::default()
            },
        ],
        active_index: Some(0),
        ..ChromeViewState::default()
    };
    let mut out = Vec::new();
    for spec in menu_model(&view, platform) {
        for entry in &spec.items {
            if let MenuEntry::Item(item) = entry
                && let Some(shortcut) = item.shortcut
                && item.enabled
                && !matches!(item.command, MenuCommand::Native(_))
            {
                out.push((item.label.clone(), item.command.clone(), shortcut));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_display_macos_glyph_order() {
        let s = Shortcut {
            key: ShortcutKey::Char('w'),
            shift: true,
            alt: false,
            ctrl: false,
            cmd: true,
        };
        assert_eq!(s.display_macos(), "⇧⌘W");
        let s = Shortcut {
            key: ShortcutKey::Char('i'),
            shift: false,
            alt: true,
            ctrl: false,
            cmd: true,
        };
        assert_eq!(s.display_macos(), "⌥⌘I");
    }

    #[test]
    fn menu_command_id_round_trip() {
        for cmd in [
            MenuCommand::Back,
            MenuCommand::Quit,
            MenuCommand::EditCopy,
            MenuCommand::SelectTab(42),
        ] {
            let id = cmd.id().expect("id");
            let parsed = MenuCommand::from_id(&id).expect("round trip");
            assert_eq!(parsed, cmd);
        }
    }

    #[test]
    fn mac_native_menu_titles_and_order() {
        let specs = menu_model(&ChromeViewState::default(), MenuPlatform::MacNative);
        let titles: Vec<_> = specs.iter().map(|s| s.title).collect();
        assert_eq!(
            titles,
            [
                "Glide",
                "File",
                "Edit",
                "View",
                "History",
                "Bookmarks",
                "Develop",
                "Window",
                "Help"
            ]
        );
    }

    #[test]
    fn in_window_menu_titles_match_f045() {
        let specs = menu_model(&ChromeViewState::default(), MenuPlatform::InWindow);
        let titles: Vec<_> = specs.iter().map(|s| s.title).collect();
        assert_eq!(
            titles,
            [
                "File",
                "Edit",
                "View",
                "History",
                "Bookmarks",
                "Tabs",
                "Help"
            ]
        );
    }

    #[test]
    fn history_items_enabled_state() {
        let back_off = menu_model(&ChromeViewState::default(), MenuPlatform::MacNative);
        let hist = back_off
            .iter()
            .find(|s| s.id == MenuBarMenuId::History)
            .unwrap();
        let back = hist.items.iter().find_map(|e| match e {
            MenuEntry::Item(i) if i.command == MenuCommand::Back => Some(i.enabled),
            _ => None,
        });
        assert_eq!(back, Some(false));
        let on = menu_model(
            &ChromeViewState {
                can_go_back: true,
                can_go_forward: true,
                ..Default::default()
            },
            MenuPlatform::MacNative,
        );
        let hist = on.iter().find(|s| s.id == MenuBarMenuId::History).unwrap();
        let back = hist.items.iter().find_map(|e| match e {
            MenuEntry::Item(i) if i.command == MenuCommand::Back => Some(i.enabled),
            _ => None,
        });
        assert_eq!(back, Some(true));
    }

    #[test]
    fn close_tab_stays_enabled_with_one_tab_on_mac_native() {
        let specs = menu_model(&ChromeViewState::default(), MenuPlatform::MacNative);
        let file = specs.iter().find(|s| s.id == MenuBarMenuId::File).unwrap();
        let close = file.items.iter().find_map(|e| match e {
            MenuEntry::Item(i) if i.label == "Close Tab" => Some(i.enabled),
            _ => None,
        });
        assert_eq!(close, Some(true));
    }

    #[test]
    fn native_menu_warning_uses_schema_fields() {
        let fields = native_menu_warning_fields("menu_build", "boom".into());
        let line = crate::logging::format_record(
            crate::logging::Level::Warn,
            "menu.native",
            "native menu bar unavailable",
            &fields,
            crate::logging::Level::Warn,
        )
        .expect("warn is enabled");
        assert!(line.contains("target=\"menu.native\""), "{line}");
        for key in [
            "feature=\"F-049\"",
            "error_kind=\"menu_build\"",
            "detail=\"boom\"",
            "hint=",
        ] {
            assert!(line.contains(key), "{key} in {line}");
        }
    }

    #[test]
    fn find_next_prev_enabled_only_with_find_bar_open() {
        let enabled = |view: &ChromeViewState| -> Vec<bool> {
            let specs = menu_model(view, MenuPlatform::MacNative);
            let edit = specs.iter().find(|s| s.id == MenuBarMenuId::Edit).unwrap();
            edit.items
                .iter()
                .filter_map(|e| match e {
                    MenuEntry::Item(i)
                        if matches!(i.command, MenuCommand::FindNext | MenuCommand::FindPrev) =>
                    {
                        Some(i.enabled)
                    }
                    _ => None,
                })
                .collect()
        };
        assert_eq!(enabled(&ChromeViewState::default()), vec![false, false]);
        let open = ChromeViewState {
            find_bar_open: true,
            ..ChromeViewState::default()
        };
        assert_eq!(enabled(&open), vec![true, true]);
    }

    #[test]
    fn select_tab_command_routes_to_tab_selection() {
        let command = MenuCommand::SelectTab(7);
        assert_eq!(
            MenuCommand::from_id(&command.id().unwrap()),
            Some(command.clone())
        );
        assert_eq!(command.to_nav_command(), Some(NavCommand::SelectTab(7)));
        assert_eq!(command.to_browser_shortcut(), None);
    }

    #[test]
    fn stop_enabled_while_loading() {
        let idle = menu_model(&ChromeViewState::default(), MenuPlatform::MacNative);
        let view = idle.iter().find(|s| s.id == MenuBarMenuId::View).unwrap();
        let stop_idle = view.items.iter().find_map(|e| match e {
            MenuEntry::Item(i) if i.command == MenuCommand::Stop => Some(i.enabled),
            _ => None,
        });
        assert_eq!(stop_idle, Some(false));
        let loading = menu_model(
            &ChromeViewState {
                page_loading: true,
                ..Default::default()
            },
            MenuPlatform::MacNative,
        );
        let view = loading
            .iter()
            .find(|s| s.id == MenuBarMenuId::View)
            .unwrap();
        let stop = view.items.iter().find_map(|e| match e {
            MenuEntry::Item(i) if i.command == MenuCommand::Stop => Some(i.enabled),
            _ => None,
        });
        assert_eq!(stop, Some(true));
    }

    #[test]
    fn bookmark_label_toggles() {
        let specs = menu_model(
            &ChromeViewState {
                bookmarked: true,
                ..Default::default()
            },
            MenuPlatform::MacNative,
        );
        let bm = specs
            .iter()
            .find(|s| s.id == MenuBarMenuId::Bookmarks)
            .unwrap();
        let label = bm.items.iter().find_map(|e| match e {
            MenuEntry::Item(i) if i.command == MenuCommand::ToggleBookmark => Some(i.label.clone()),
            _ => None,
        });
        assert_eq!(label.as_deref(), Some("Remove Bookmark"));
    }

    #[test]
    fn disable_js_checkmark() {
        let specs = menu_model(
            &ChromeViewState {
                js_enabled: false,
                ..Default::default()
            },
            MenuPlatform::MacNative,
        );
        let dev = specs
            .iter()
            .find(|s| s.id == MenuBarMenuId::Develop)
            .unwrap();
        let checked = dev.items.iter().find_map(|e| match e {
            MenuEntry::Item(i) if i.command == MenuCommand::ToggleJs => Some(i.checked),
            _ => None,
        });
        assert_eq!(checked, Some(true));
    }

    #[test]
    fn window_menu_tab_list() {
        let specs = menu_model(
            &ChromeViewState {
                tabs: vec![
                    crate::chrome::TabView {
                        id: 1,
                        title: "One".into(),
                        ..Default::default()
                    },
                    crate::chrome::TabView {
                        id: 2,
                        title: "Two".into(),
                        ..Default::default()
                    },
                ],
                active_index: Some(1),
                ..Default::default()
            },
            MenuPlatform::MacNative,
        );
        let window = specs
            .iter()
            .find(|s| s.id == MenuBarMenuId::Window)
            .unwrap();
        let tabs: Vec<_> = window
            .items
            .iter()
            .filter_map(|e| match e {
                MenuEntry::Item(i) if matches!(i.command, MenuCommand::SelectTab(_)) => {
                    Some((i.label.clone(), i.checked))
                }
                _ => None,
            })
            .collect();
        assert_eq!(tabs.len(), 2);
        assert!(tabs[1].1);
    }

    #[test]
    fn menu_shortcuts_match_keyboard() {
        for platform in [MenuPlatform::MacNative, MenuPlatform::InWindow] {
            for (label, command, shortcut) in actionable_menu_items(platform) {
                let Some(code) = shortcut.to_key_code() else {
                    continue;
                };
                let mods = shortcut.to_modifiers_state();
                let find_open = matches!(command, MenuCommand::FindNext | MenuCommand::FindPrev);
                let got = crate::keyboard::browser_shortcut(code, mods, false, find_open);
                let expected = command.to_browser_shortcut();
                assert_eq!(
                    got, expected,
                    "{label}: shortcut {:?} on {platform:?}",
                    shortcut
                );
            }
        }
    }

    fn two_tab_view() -> ChromeViewState {
        ChromeViewState {
            tabs: vec![
                crate::chrome::TabView {
                    id: 1,
                    title: "One".into(),
                    ..Default::default()
                },
                crate::chrome::TabView {
                    id: 2,
                    title: "Two".into(),
                    ..Default::default()
                },
            ],
            active_index: Some(0),
            ..Default::default()
        }
    }

    fn all_commands(platform: MenuPlatform) -> Vec<MenuCommand> {
        menu_model(&two_tab_view(), platform)
            .into_iter()
            .flat_map(|spec| spec.items)
            .filter_map(|entry| match entry {
                MenuEntry::Item(item) => Some(item.command),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_model_command_id_round_trips() {
        for platform in [MenuPlatform::MacNative, MenuPlatform::InWindow] {
            for command in all_commands(platform) {
                if matches!(command, MenuCommand::Native(_)) {
                    assert_eq!(command.id(), None);
                    continue;
                }
                let id = command.id().expect("non-native command has an id");
                assert_eq!(MenuCommand::from_id(&id), Some(command), "{id}");
            }
        }
    }

    #[test]
    fn native_menu_command_ids_are_unique() {
        let ids: Vec<String> = all_commands(MenuPlatform::MacNative)
            .iter()
            .filter_map(MenuCommand::id)
            .collect();
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "duplicate ids: {ids:?}");
    }

    #[test]
    fn unknown_menu_id_is_rejected() {
        assert_eq!(MenuCommand::from_id("glide://nope"), None);
        assert_eq!(MenuCommand::from_id("not a url"), None);
    }

    #[test]
    fn app_menu_quit_uses_the_quit_shortcut_path() {
        let specs = menu_model(&ChromeViewState::default(), MenuPlatform::MacNative);
        let app = &specs[0];
        assert_eq!(app.id, MenuBarMenuId::GlideApp);
        let quit = app
            .items
            .iter()
            .find_map(|e| match e {
                MenuEntry::Item(i) if i.command == MenuCommand::Quit => Some(i),
                _ => None,
            })
            .expect("Quit Glide item");
        assert_eq!(
            quit.shortcut.map(|s| s.display_macos()).as_deref(),
            Some("⌘Q")
        );
        assert_eq!(
            quit.command.to_browser_shortcut(),
            Some(BrowserShortcut::Quit)
        );
    }

    #[test]
    fn in_window_model_has_no_app_or_window_menu() {
        let specs = menu_model(&ChromeViewState::default(), MenuPlatform::InWindow);
        assert!(
            specs
                .iter()
                .all(|s| !matches!(s.id, MenuBarMenuId::GlideApp | MenuBarMenuId::Window))
        );
    }

    #[test]
    fn develop_menu_label_follows_devtools_state() {
        let label = |open: bool| {
            let specs = menu_model(
                &ChromeViewState {
                    devtools_open: open,
                    ..Default::default()
                },
                MenuPlatform::MacNative,
            );
            specs
                .iter()
                .find(|s| s.id == MenuBarMenuId::Develop)
                .and_then(|s| {
                    s.items.iter().find_map(|e| match e {
                        MenuEntry::Item(i) if i.command == MenuCommand::ToggleDevTools => {
                            Some(i.label.clone())
                        }
                        _ => None,
                    })
                })
        };
        assert_eq!(label(false).as_deref(), Some("Show Web Inspector"));
        assert_eq!(label(true).as_deref(), Some("Hide Web Inspector"));
    }

    #[test]
    fn window_menu_next_prev_tab_need_two_tabs() {
        let enabled = |view: &ChromeViewState| {
            menu_model(view, MenuPlatform::MacNative)
                .into_iter()
                .find(|s| s.id == MenuBarMenuId::Window)
                .expect("window menu")
                .items
                .into_iter()
                .filter_map(|e| match e {
                    MenuEntry::Item(i)
                        if matches!(i.command, MenuCommand::NextTab | MenuCommand::PrevTab) =>
                    {
                        Some(i.enabled)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(enabled(&ChromeViewState::default()), [false, false]);
        assert_eq!(enabled(&two_tab_view()), [true, true]);
    }

    #[test]
    fn undo_and_redo_stay_disabled() {
        let specs = menu_model(&ChromeViewState::default(), MenuPlatform::MacNative);
        let edit = specs
            .iter()
            .find(|s| s.id == MenuBarMenuId::Edit)
            .expect("edit");
        for entry in &edit.items {
            if let MenuEntry::Item(i) = entry
                && matches!(i.command, MenuCommand::EditUndo | MenuCommand::EditRedo)
            {
                assert!(!i.enabled, "{} must stay disabled", i.label);
            }
        }
    }

    #[test]
    fn shortcut_display_covers_brackets_and_control() {
        let back = cmd_key(ShortcutKey::BracketLeft);
        assert_eq!(back.display_macos(), "⌘[");
        let full = Shortcut {
            ctrl: true,
            ..cmd('f')
        };
        assert_eq!(full.display_macos(), "⌃⌘F");
    }

    fn cmd_key(key: ShortcutKey) -> Shortcut {
        Shortcut {
            key,
            shift: false,
            alt: false,
            ctrl: false,
            cmd: true,
        }
    }
}
