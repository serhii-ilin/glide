//! macOS native menu bar built from [`crate::menu::menu_model`] via muda.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use muda::accelerator::{Accelerator, Code, Modifiers, CMD_OR_CTRL};
use muda::{
    AboutMetadata, CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem,
    Submenu,
};

use crate::chrome::ChromeViewState;
use crate::menu::{
    MenuCommand, MenuCommandQueue, MenuEntry, MenuPlatform, MenuSpec, Shortcut, ShortcutKey,
    menu_model,
};

pub struct NativeMenu {
    queue: MenuCommandQueue,
    menu: Menu,
    window_submenu: Rc<RefCell<Option<Submenu>>>,
    #[allow(dead_code)]
    item_refs: RefCell<HashMap<String, DynamicItem>>,
    last_specs: RefCell<Vec<MenuSpec>>,
    initialized: bool,
}

enum DynamicItem {
    Plain(MenuItem),
    Check(CheckMenuItem),
}

impl NativeMenu {
    pub fn new(queue: MenuCommandQueue) -> Self {
        Self {
            queue,
            menu: Menu::new(),
            window_submenu: Rc::new(RefCell::new(None)),
            item_refs: RefCell::new(HashMap::new()),
            last_specs: RefCell::new(Vec::new()),
            initialized: false,
        }
    }

    pub fn ensure_installed(
        &mut self,
        proxy_wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        if self.initialized {
            return Ok(());
        }
        let queue = self.queue.clone_inner();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some(cmd) = MenuCommand::from_id(event.id().0.as_str()) {
                queue.lock().expect("menu queue").push(cmd);
                proxy_wake();
            }
        }));
        let specs = menu_model(&ChromeViewState::default(), MenuPlatform::MacNative);
        self.build_from_specs(&specs)?;
        self.menu
            .init_for_nsapp();
        self.initialized = true;
        Ok(())
    }

    pub fn sync(&mut self, view: &ChromeViewState) -> Result<(), String> {
        let specs = menu_model(view, MenuPlatform::MacNative);
        if *self.last_specs.borrow() == specs {
            return Ok(());
        }
        if !self.initialized {
            return Ok(());
        }
        self.menu.remove_for_nsapp();
        self.build_from_specs(&specs)?;
        self.menu.init_for_nsapp();
        *self.last_specs.borrow_mut() = specs;
        Ok(())
    }

    fn build_from_specs(&mut self, specs: &[MenuSpec]) -> Result<(), String> {
        self.item_refs.borrow_mut().clear();
        self.menu = Menu::new();

        let glide_sub = Submenu::new("Glide", true);
        let about = PredefinedMenuItem::about(
            Some("About Glide"),
            Some(AboutMetadata {
                version: Some(env!("CARGO_PKG_VERSION").into()),
                ..AboutMetadata::default()
            }),
        );
        glide_sub.append(&about).map_err(|e| e.to_string())?;
        glide_sub
            .append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        glide_sub
            .append(&PredefinedMenuItem::services(None))
            .map_err(|e| e.to_string())?;
        glide_sub
            .append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        glide_sub
            .append(&PredefinedMenuItem::hide(Some("Hide Glide")))
            .map_err(|e| e.to_string())?;
        glide_sub
            .append(&PredefinedMenuItem::hide_others(None))
            .map_err(|e| e.to_string())?;
        glide_sub
            .append(&PredefinedMenuItem::show_all(None))
            .map_err(|e| e.to_string())?;
        glide_sub
            .append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        let quit = self.append_command_item(
            "Quit Glide",
            MenuCommand::Quit,
            Some(Shortcut {
                key: ShortcutKey::Char('q'),
                shift: false,
                alt: false,
                ctrl: false,
                cmd: true,
            }),
            true,
            false,
        )?;
        glide_sub.append(&quit).map_err(|e| e.to_string())?;
        self.menu.append(&glide_sub).map_err(|e| e.to_string())?;

        let mut window_menu: Option<Submenu> = None;
        let mut help_menu: Option<Submenu> = None;
        for spec in specs {
            if spec.id == crate::menu::MenuBarMenuId::GlideApp {
                continue;
            }
            let sub = Submenu::new(spec.title, true);
            self.fill_submenu(&sub, &spec.items)?;
            if spec.id == crate::menu::MenuBarMenuId::Window {
                sub.set_as_windows_menu_for_nsapp();
                window_menu = Some(sub.clone());
            }
            if spec.id == crate::menu::MenuBarMenuId::Help {
                sub.set_as_help_menu_for_nsapp();
                help_menu = Some(sub.clone());
            }
            self.menu.append(&sub).map_err(|e| e.to_string())?;
        }
        *self.window_submenu.borrow_mut() = window_menu;
        let _ = help_menu;
        Ok(())
    }

    fn fill_submenu(&self, sub: &Submenu, items: &[MenuEntry]) -> Result<(), String> {
        for entry in items {
            match entry {
                MenuEntry::Separator => {
                    sub.append(&PredefinedMenuItem::separator())
                        .map_err(|e| e.to_string())?;
                }
                MenuEntry::Heading(_) => {}
                MenuEntry::Item(item) => {
                    if item.command == MenuCommand::NativeOnly {
                        let native = match item.label.as_str() {
                            "Minimize" => PredefinedMenuItem::minimize(Some("Minimize")),
                            "Zoom" => PredefinedMenuItem::zoom(None),
                            "Enter Full Screen" => PredefinedMenuItem::fullscreen(None),
                            "Bring All to Front" => {
                                PredefinedMenuItem::bring_all_to_front(None)
                            }
                            _ => continue,
                        };
                        sub.append(&native).map_err(|e| e.to_string())?;
                        continue;
                    }
                    if item.checked || matches!(item.command, MenuCommand::ToggleJs) {
                        let check = self.append_check_item(
                            &item.label,
                            &item.command,
                            item.shortcut,
                            item.enabled,
                            item.checked,
                        )?;
                        sub.append(&check).map_err(|e| e.to_string())?;
                    } else {
                        let plain = self.append_command_item(
                            &item.label,
                            item.command.clone(),
                            item.shortcut,
                            item.enabled,
                            item.checked,
                        )?;
                        sub.append(&plain).map_err(|e| e.to_string())?;
                    }
                }
            }
        }
        Ok(())
    }

    fn append_command_item(
        &self,
        label: &str,
        command: MenuCommand,
        shortcut: Option<Shortcut>,
        enabled: bool,
        checked: bool,
    ) -> Result<MenuItem, String> {
        let id = command.id().ok_or_else(|| "command missing id".to_string())?;
        let item = MenuItem::with_id(
            id.clone(),
            label,
            enabled,
            shortcut.and_then(shortcut_to_accelerator),
        );
        if checked {
            // checkmark lives on a separate type; plain item ignores checked
        }
        self.item_refs
            .borrow_mut()
            .insert(id, DynamicItem::Plain(item.clone()));
        Ok(item)
    }

    fn append_check_item(
        &self,
        label: &str,
        command: &MenuCommand,
        shortcut: Option<Shortcut>,
        enabled: bool,
        checked: bool,
    ) -> Result<CheckMenuItem, String> {
        let id = command.id().ok_or_else(|| "command missing id".to_string())?;
        let item = CheckMenuItem::with_id(
            id.clone(),
            label,
            enabled,
            checked,
            shortcut.and_then(shortcut_to_accelerator),
        );
        self.item_refs
            .borrow_mut()
            .insert(id, DynamicItem::Check(item.clone()));
        Ok(item)
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
            'w' => Some(Code::KeyW),
            'x' => Some(Code::KeyX),
            'v' => Some(Code::KeyV),
            'z' => Some(Code::KeyZ),
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