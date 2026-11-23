//! macOS native menu bar built from [`crate::menu::menu_model`] via muda.
//!
//! The model is the single source of truth: this module only maps it onto AppKit menu items,
//! keeps the last model it applied, and touches muda in place when the model changes.

#![cfg(target_os = "macos")]

use std::collections::HashMap;

use muda::accelerator::{Accelerator, CMD_OR_CTRL, Code, Modifiers};
use muda::{
    AboutMetadata, CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem,
    Submenu,
};

use crate::chrome::ChromeViewState;
use crate::logging;
use crate::menu::{
    MenuBarMenuId, MenuCommand, MenuCommandQueue, MenuEntry, MenuItemSpec, MenuPlatform, MenuSpec,
    NativeItem, Shortcut, ShortcutKey, menu_model,
};

/// A muda item Glide updates in place (predefined AppKit items and separators are static).
enum LiveItem {
    Plain(MenuItem),
    Check(CheckMenuItem),
}

impl LiveItem {
    fn update(&self, spec: &MenuItemSpec) {
        match self {
            Self::Plain(item) => {
                item.set_text(&spec.label);
                item.set_enabled(spec.enabled);
            }
            Self::Check(item) => {
                item.set_text(&spec.label);
                item.set_enabled(spec.enabled);
                item.set_checked(spec.checked);
            }
        }
    }
}

/// One installed top-level menu: its muda submenu, the entry shape it was built from, and the
/// updatable items aligned with that shape.
struct BuiltMenu {
    submenu: Submenu,
    shape: Vec<String>,
    items: Vec<Option<LiveItem>>,
}

struct Installed {
    /// Held so the NSApp main menu lives as long as the app.
    _menu: Menu,
    built: HashMap<MenuBarMenuId, BuiltMenu>,
    last: Vec<MenuSpec>,
}

pub struct NativeMenu {
    queue: MenuCommandQueue,
    installed: Option<Installed>,
}

impl NativeMenu {
    pub fn new(queue: MenuCommandQueue) -> Self {
        Self {
            queue,
            installed: None,
        }
    }

    /// Build and install the menu bar. Must run on the main thread, once.
    pub fn ensure_installed(
        &mut self,
        view: &ChromeViewState,
        wake: impl Fn() + Send + Sync + 'static,
    ) {
        if self.installed.is_some() {
            return;
        }
        let queue = self.queue.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some(command) = MenuCommand::from_id(event.id().0.as_str()) {
                queue.push(command);
                wake();
            }
        }));
        let specs = menu_model(view, MenuPlatform::MacNative);
        match install(&specs) {
            Ok(installed) => self.installed = Some(installed),
            Err(err) => logging::warn(
                "menu.native",
                "native menu bar unavailable",
                &[
                    ("error_kind", "menu_build".into()),
                    ("detail", err),
                    (
                        "hint",
                        "Commands remain available through keyboard shortcuts (F-049).".into(),
                    ),
                ],
            ),
        }
    }

    /// Apply a changed model to the live menus; a no-op when nothing changed.
    pub fn sync(&mut self, view: &ChromeViewState) {
        let Some(installed) = self.installed.as_mut() else {
            return;
        };
        let specs = menu_model(view, MenuPlatform::MacNative);
        if installed.last == specs {
            return;
        }
        for spec in &specs {
            let Some(built) = installed.built.get_mut(&spec.id) else {
                continue;
            };
            if let Err(err) = update_menu(built, spec) {
                logging::warn(
                    "menu.native",
                    "native menu update failed",
                    &[
                        ("error_kind", "menu_update".into()),
                        ("detail", err),
                        ("hint", "See F-049 in docs/features.md.".into()),
                    ],
                );
            }
        }
        installed.last = specs;
    }
}

fn install(specs: &[MenuSpec]) -> Result<Installed, String> {
    let menu = Menu::new();
    let mut built = HashMap::new();
    for spec in specs {
        let submenu = Submenu::new(spec.title, true);
        let mut menu_built = BuiltMenu {
            submenu: submenu.clone(),
            shape: Vec::new(),
            items: Vec::new(),
        };
        fill_menu(&mut menu_built, spec)?;
        menu.append(&submenu).map_err(|e| e.to_string())?;
        match spec.id {
            MenuBarMenuId::Window => submenu.set_as_windows_menu_for_nsapp(),
            MenuBarMenuId::Help => submenu.set_as_help_menu_for_nsapp(),
            _ => {}
        }
        built.insert(spec.id, menu_built);
    }
    menu.init_for_nsapp();
    Ok(Installed {
        _menu: menu,
        built,
        last: specs.to_vec(),
    })
}

fn entry_shape(entry: &MenuEntry) -> String {
    match entry {
        MenuEntry::Separator => "-".into(),
        MenuEntry::Heading(label) => format!("h:{label}"),
        MenuEntry::Item(item) => match &item.command {
            MenuCommand::Native(native) => format!("n:{native:?}"),
            command => command.id().unwrap_or_default(),
        },
    }
}

/// Update `built` in place, or rebuild its items when the entry shape changed (Window tab list).
fn update_menu(built: &mut BuiltMenu, spec: &MenuSpec) -> Result<(), String> {
    let shape: Vec<String> = spec.items.iter().map(entry_shape).collect();
    if shape != built.shape {
        while built.submenu.remove_at(0).is_some() {}
        return fill_menu(built, spec);
    }
    for (live, entry) in built.items.iter().zip(&spec.items) {
        if let (Some(live), MenuEntry::Item(item)) = (live, entry) {
            live.update(item);
        }
    }
    Ok(())
}

fn fill_menu(built: &mut BuiltMenu, spec: &MenuSpec) -> Result<(), String> {
    built.shape = spec.items.iter().map(entry_shape).collect();
    built.items.clear();
    for entry in &spec.items {
        let live = append_entry(&built.submenu, entry)?;
        built.items.push(live);
    }
    Ok(())
}

fn append_entry(submenu: &Submenu, entry: &MenuEntry) -> Result<Option<LiveItem>, String> {
    let append = |item: &dyn IsMenuItem| submenu.append(item).map_err(|e| e.to_string());
    match entry {
        MenuEntry::Separator => append(&PredefinedMenuItem::separator())?,
        MenuEntry::Heading(_) => {}
        MenuEntry::Item(item) => {
            if let MenuCommand::Native(native) = &item.command {
                append(&native_item(*native))?;
                return Ok(None);
            }
            let id = item
                .command
                .id()
                .ok_or_else(|| "menu command has no id".to_string())?;
            let accelerator = item.shortcut.and_then(shortcut_to_accelerator);
            if item.command.is_checkable() {
                let check = CheckMenuItem::with_id(
                    id,
                    &item.label,
                    item.enabled,
                    item.checked,
                    accelerator,
                );
                append(&check)?;
                return Ok(Some(LiveItem::Check(check)));
            }
            let plain = MenuItem::with_id(id, &item.label, item.enabled, accelerator);
            append(&plain)?;
            return Ok(Some(LiveItem::Plain(plain)));
        }
    }
    Ok(None)
}

fn native_item(native: NativeItem) -> PredefinedMenuItem {
    match native {
        NativeItem::About => PredefinedMenuItem::about(
            Some("About Glide"),
            Some(AboutMetadata {
                name: Some("Glide".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                ..AboutMetadata::default()
            }),
        ),
        NativeItem::Services => PredefinedMenuItem::services(None),
        NativeItem::Hide => PredefinedMenuItem::hide(Some("Hide Glide")),
        NativeItem::HideOthers => PredefinedMenuItem::hide_others(None),
        NativeItem::ShowAll => PredefinedMenuItem::show_all(None),
        NativeItem::Minimize => PredefinedMenuItem::minimize(None),
        NativeItem::Zoom => PredefinedMenuItem::maximize(None),
        NativeItem::FullScreen => PredefinedMenuItem::fullscreen(None),
        NativeItem::BringAllToFront => PredefinedMenuItem::bring_all_to_front(None),
    }
}

fn shortcut_to_accelerator(shortcut: Shortcut) -> Option<Accelerator> {
    let code = shortcut_to_code(shortcut.key)?;
    let mut mods = Modifiers::empty();
    if shortcut.shift {
        mods |= Modifiers::SHIFT;
    }
    if shortcut.alt {
        mods |= Modifiers::ALT;
    }
    if shortcut.cmd {
        mods |= CMD_OR_CTRL;
    }
    if shortcut.ctrl {
        mods |= Modifiers::CONTROL;
    }
    Some(Accelerator::new(mods, code))
}

fn shortcut_to_code(key: ShortcutKey) -> Option<Code> {
    match key {
        ShortcutKey::Char(c) => match c.to_ascii_lowercase() {
            'a' => Some(Code::KeyA),
            'b' => Some(Code::KeyB),
            'c' => Some(Code::KeyC),
            'd' => Some(Code::KeyD),
            'f' => Some(Code::KeyF),
            'g' => Some(Code::KeyG),
            'h' => Some(Code::KeyH),
            'i' => Some(Code::KeyI),
            'j' => Some(Code::KeyJ),
            'l' => Some(Code::KeyL),
            'm' => Some(Code::KeyM),
            'q' => Some(Code::KeyQ),
            'r' => Some(Code::KeyR),
            't' => Some(Code::KeyT),
            'v' => Some(Code::KeyV),
            'w' => Some(Code::KeyW),
            'x' => Some(Code::KeyX),
            'z' => Some(Code::KeyZ),
            '/' => Some(Code::Slash),
            _ => None,
        },
        ShortcutKey::Digit(d) => match d {
            0 => Some(Code::Digit0),
            1 => Some(Code::Digit1),
            2 => Some(Code::Digit2),
            3 => Some(Code::Digit3),
            4 => Some(Code::Digit4),
            5 => Some(Code::Digit5),
            6 => Some(Code::Digit6),
            7 => Some(Code::Digit7),
            8 => Some(Code::Digit8),
            9 => Some(Code::Digit9),
            _ => None,
        },
        ShortcutKey::F12 => Some(Code::F12),
        ShortcutKey::Escape => Some(Code::Escape),
        ShortcutKey::BracketLeft => Some(Code::BracketLeft),
        ShortcutKey::BracketRight => Some(Code::BracketRight),
        ShortcutKey::Period => Some(Code::Period),
        ShortcutKey::Plus => Some(Code::Equal),
        ShortcutKey::Minus => Some(Code::Minus),
    }
}
