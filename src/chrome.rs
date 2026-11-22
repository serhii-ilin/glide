//! The browser chrome is itself an HTML document. The page lives in an
//! `<iframe>`, so link clicks, scrolling, and subresource loads stay inside
//! Blitz. Buttons and the address form report back through [`command_from_url`].

use url::Url;

use crate::tabs::TabId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavCommand {
    Back,
    Forward,
    Reload,
    Stop,
    ToggleJs,
    ToggleBookmark,
    Find,
    FindClose,
    FindNext,
    FindPrev,
    DevToolsToggle,
    DevToolsDockBottom,
    DevToolsDockRight,
    DevToolsTab(crate::devtools::DevToolsTab),
    DevToolsConsoleClear,
    DevToolsElementsSelect(u64),
    DevToolsElementsToggle(u64),
    DevToolsInspectToggle,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    NewTab,
    CloseActiveTab,
    CloseTab(TabId),
    SelectTab(TabId),
    SelectTabIndex(usize),
    NextTab,
    PrevTab,
    DuplicateTab,
    CloseWindow,
    MenuOpen(MenuId),
    MenuClose,
    /// Raw address-bar text, not yet classified as a URL or a search.
    Go(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuId {
    File,
    Edit,
    View,
    History,
    Bookmarks,
    Tabs,
    Help,
}

impl MenuId {
    pub const ALL: [MenuId; 7] = [
        MenuId::File,
        MenuId::Edit,
        MenuId::View,
        MenuId::History,
        MenuId::Bookmarks,
        MenuId::Tabs,
        MenuId::Help,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MenuId::File => "File",
            MenuId::Edit => "Edit",
            MenuId::View => "View",
            MenuId::History => "History",
            MenuId::Bookmarks => "Bookmarks",
            MenuId::Tabs => "Tabs",
            MenuId::Help => "Help",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|menu| {
            menu.label().eq_ignore_ascii_case(name) || menu.name().eq_ignore_ascii_case(name)
        })
    }

    fn name(self) -> &'static str {
        match self {
            MenuId::File => "file",
            MenuId::Edit => "edit",
            MenuId::View => "view",
            MenuId::History => "history",
            MenuId::Bookmarks => "bookmarks",
            MenuId::Tabs => "tabs",
            MenuId::Help => "help",
        }
    }
}

/// One tab as shown in the chrome strip.
#[derive(Debug, Clone, Default)]
pub struct TabView {
    pub id: TabId,
    pub title: String,
    pub loading: bool,
}

/// Everything the chrome renderer needs to draw the window.
#[derive(Debug, Clone, Default)]
pub struct ChromeViewState {
    pub url: String,
    /// Title of the active page, shown in the address field.
    pub title: String,
    pub js_enabled: bool,
    pub bookmarked: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub page_loading: bool,
    pub tabs: Vec<TabView>,
    pub active_index: Option<usize>,
    pub open_menu: Option<MenuId>,
}

pub fn command_from_url(url: &Url) -> Option<NavCommand> {
    if url.scheme() == "glide" {
        return match url.host_str() {
            Some("back") => Some(NavCommand::Back),
            Some("forward") => Some(NavCommand::Forward),
            Some("reload") => Some(NavCommand::Reload),
            Some("stop") => Some(NavCommand::Stop),
            Some("toggle-js") => Some(NavCommand::ToggleJs),
            Some("bookmark") => Some(NavCommand::ToggleBookmark),
            Some("find") => Some(NavCommand::Find),
            Some("find-close") => Some(NavCommand::FindClose),
            Some("find-next") => Some(NavCommand::FindNext),
            Some("find-prev") => Some(NavCommand::FindPrev),
            Some("devtools") => Some(NavCommand::DevToolsToggle),
            Some("devtools-dock-bottom") => Some(NavCommand::DevToolsDockBottom),
            Some("devtools-dock-right") => Some(NavCommand::DevToolsDockRight),
            Some("devtools-tab") => query_value(url, "tab")
                .as_deref()
                .and_then(crate::devtools::DevToolsTab::parse)
                .map(NavCommand::DevToolsTab),
            Some("devtools-console-clear") => Some(NavCommand::DevToolsConsoleClear),
            Some("devtools-elements-select") => query_value(url, "node")
                .and_then(|n| n.parse().ok())
                .map(NavCommand::DevToolsElementsSelect),
            Some("devtools-elements-toggle") => query_value(url, "node")
                .and_then(|n| n.parse().ok())
                .map(NavCommand::DevToolsElementsToggle),
            Some("devtools-inspect-toggle") => Some(NavCommand::DevToolsInspectToggle),
            Some("zoom-in") => Some(NavCommand::ZoomIn),
            Some("zoom-out") => Some(NavCommand::ZoomOut),
            Some("zoom-reset") => Some(NavCommand::ZoomReset),
            Some("new-tab") => Some(NavCommand::NewTab),
            Some("close-active-tab") => Some(NavCommand::CloseActiveTab),
            Some("close-tab") => query_value(url, "id")
                .and_then(|id| id.parse().ok())
                .map(NavCommand::CloseTab),
            Some("select-tab") => query_value(url, "id")
                .and_then(|id| id.parse().ok())
                .map(NavCommand::SelectTab),
            Some("next-tab") => Some(NavCommand::NextTab),
            Some("prev-tab") => Some(NavCommand::PrevTab),
            Some("duplicate-tab") => Some(NavCommand::DuplicateTab),
            Some("close-window") => Some(NavCommand::CloseWindow),
            Some("menu-open") => query_value(url, "menu")
                .and_then(|name| MenuId::from_name(&name))
                .map(NavCommand::MenuOpen),
            Some("menu-close") => Some(NavCommand::MenuClose),
            Some("bookmarks") => Some(NavCommand::Go("glide://bookmarks".into())),
            Some("help") => Some(NavCommand::Go("glide://help".into())),
            _ => None,
        };
    }

    if url.host_str() == Some("glide.invalid") {
        let query = url
            .query_pairs()
            .find(|(key, _)| key == "q")
            .map(|(_, value)| value.into_owned())
            .unwrap_or_default();
        let query = query.trim().to_string();
        if query.is_empty() {
            return None;
        }
        return Some(NavCommand::Go(query));
    }

    Some(NavCommand::Go(url.to_string()))
}

/// Whether running a command should dismiss an open menu dropdown.
pub fn command_closes_menu(command: &NavCommand) -> bool {
    !matches!(command, NavCommand::MenuOpen(_) | NavCommand::MenuClose)
}

/// The open menu after a trigger click: the same title toggles closed,
/// another title replaces the open dropdown.
pub fn toggle_open_menu(current: Option<MenuId>, requested: MenuId) -> Option<MenuId> {
    if current == Some(requested) {
        None
    } else {
        Some(requested)
    }
}

fn query_value(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.into_owned())
}

/// Render a minimal chrome document for tests that only need the page frame.
pub fn chrome_document(start_url: &str, js_enabled: bool, bookmarked: bool) -> String {
    chrome_document_from_state(&ChromeViewState {
        url: start_url.to_string(),
        js_enabled,
        bookmarked,
        ..ChromeViewState::default()
    })
}

// Chrome icons. The system fonts have no matching line glyphs (no circular arrow, and
// arrows/stars in different weights), so every chrome icon is inline SVG in one
// SF Symbols-like line style: 24-unit grid, round caps, stroked in `currentColor` so
// the stylesheet's colour tokens apply.
const BACK_ICON: &str = "<svg class=\"icon\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M14.5 4.5 7 12l7.5 7.5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";
const FORWARD_ICON: &str = "<svg class=\"icon\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M9.5 4.5 17 12l-7.5 7.5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";
const RELOAD_ICON: &str = "<svg class=\"icon\" width=\"16\" height=\"16\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M19.5 12a7.5 7.5 0 1 1-2.2-5.3M18 3v4h-4\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";
const STOP_ICON: &str = "<svg class=\"icon\" width=\"16\" height=\"16\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M6.5 6.5l11 11M17.5 6.5l-11 11\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.2\" stroke-linecap=\"round\"/></svg>";
const STAR_ICON: &str = "<svg class=\"icon icon-star\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M12 3.6l2.6 5.3 5.8.85-4.2 4.1 1 5.8L12 16.9l-5.2 2.75 1-5.8-4.2-4.1 5.8-.85z\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.8\" stroke-linejoin=\"round\"/></svg>";
const STAR_FILLED_ICON: &str = "<svg class=\"icon icon-star-filled\" width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M12 3.6l2.6 5.3 5.8.85-4.2 4.1 1 5.8L12 16.9l-5.2 2.75 1-5.8-4.2-4.1 5.8-.85z\" fill=\"currentColor\" stroke=\"currentColor\" stroke-width=\"1.8\" stroke-linejoin=\"round\"/></svg>";
const PLUS_ICON: &str = "<svg class=\"icon\" width=\"16\" height=\"16\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M12 5v14M5 12h14\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\"/></svg>";
const CLOSE_ICON: &str = "<svg class=\"icon\" width=\"10\" height=\"10\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M5 5l14 14M19 5L5 19\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"3\" stroke-linecap=\"round\"/></svg>";
const SEARCH_ICON: &str = "<svg class=\"icon\" width=\"14\" height=\"14\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><circle cx=\"10.5\" cy=\"10.5\" r=\"6.5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.4\"/><path d=\"M15.5 15.5 20.5 20.5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.4\" stroke-linecap=\"round\"/></svg>";
const CHEVRON_UP_ICON: &str = "<svg class=\"icon\" width=\"16\" height=\"16\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M5.5 15.5 12 9l6.5 6.5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";
const CHEVRON_DOWN_ICON: &str = "<svg class=\"icon\" width=\"16\" height=\"16\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M5.5 8.5 12 15l6.5-6.5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";
const INSPECT_ICON: &str = "<svg class=\"icon\" width=\"16\" height=\"16\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M4 4h6v6H4zm10 0h6v6h-6zM4 14h6v6H4z\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\"/><path d=\"M14 14h2v2h-2zm4 0h2v2h-2zm-4 4h2v2h-2zm4 0h2v2h-2z\" fill=\"currentColor\"/></svg>";

/// Render the chrome from the window's current view of tabs, menus and page.
pub fn chrome_document_from_state(view: &ChromeViewState) -> String {
    let url = escape_attr(&view.url);
    let address_title = if view.title.trim().is_empty() {
        url.clone()
    } else {
        escape_attr(&view.title)
    };
    let js_label = if view.js_enabled { "JS on" } else { "JS off" };
    let (star, star_title) = bookmark_star(view.bookmarked);
    let (reload_href, reload_title, reload_icon) = reload_button(view.page_loading);
    let loading_class = if view.page_loading { "active" } else { "" };
    let tab_strip = format!(
        "<div id=\"tab-strip\">\n{}</div>\n",
        tab_strip_inner_html(view)
    );
    let menu_bar = format!(
        "<nav id=\"menu-bar\">\n{}</nav>\n",
        menu_bar_inner_html(view)
    );
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Glide</title>
<style>
  html {{
    --ui: 1;
    --chrome-bg: #f6f6f6;
    --tabbar-bg: #e6e6e6;
    --tab-hover: #ececec;
    --hairline: rgba(0,0,0,0.12);
    --field-fill: rgba(0,0,0,0.055);
    --btn-hover: rgba(0,0,0,0.06);
    --btn-pressed: rgba(0,0,0,0.11);
    --text: #1d1d1f;
    --text-secondary: #6e6e73;
    --text-tertiary: #a1a1a6;
    --menu-secondary: #86868b;
    --menu-disabled: #b0b0b5;
    --icon: #5f5f64;
    --accent: #007aff;
    --focus-ring: 0 0 0 calc(3.5px * var(--ui)) rgba(0,122,255,0.4);
  }}
  html, body {{
    margin: 0;
    height: 100%;
    background: var(--chrome-bg);
    color: var(--text);
    font-family: system-ui, -apple-system, "Apple Symbols", "Segoe UI", Ubuntu, Cantarell, sans-serif;
    font-size: calc(13px * var(--ui));
  }}
  body {{
    display: flex;
    flex-direction: column;
  }}
  a.icon-btn {{
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
    min-width: calc(28px * var(--ui));
    height: calc(28px * var(--ui));
    padding: 0;
    border: 0;
    border-radius: calc(6px * var(--ui));
    background: transparent;
    color: var(--icon);
    text-decoration: none;
  }}
  a.icon-btn:hover {{
    background: var(--btn-hover);
  }}
  a.icon-btn:active {{
    background: var(--btn-pressed);
  }}
  a.icon-btn svg {{
    width: calc(18px * var(--ui));
    height: calc(18px * var(--ui));
  }}
  a.icon-btn svg.icon-star-filled {{
    color: var(--accent);
  }}
  a.text-btn {{
    display: flex;
    align-items: center;
    flex: none;
    height: calc(28px * var(--ui));
    padding: 0 calc(8px * var(--ui));
    border: 0;
    border-radius: calc(6px * var(--ui));
    background: transparent;
    color: var(--text-secondary);
    text-decoration: none;
    font-size: calc(12px * var(--ui));
    font-weight: 500;
    white-space: nowrap;
  }}
  a.text-btn:hover {{
    background: var(--btn-hover);
  }}
  a.text-btn:active {{
    background: var(--btn-pressed);
  }}
  #menu-bar {{
    display: flex;
    align-items: center;
    height: calc(24px * var(--ui));
    padding: 0 calc(8px * var(--ui));
    background: var(--chrome-bg);
    flex: none;
  }}
  div.menu {{
    position: relative;
  }}
  a.menu-trigger {{
    display: block;
    height: calc(20px * var(--ui));
    padding: 0 calc(8px * var(--ui));
    border-radius: calc(5px * var(--ui));
    color: var(--text);
    text-decoration: none;
    font-size: calc(13px * var(--ui));
    line-height: calc(20px * var(--ui));
  }}
  a.menu-trigger:hover {{
    background: var(--btn-hover);
  }}
  a.menu-trigger.active {{
    background: var(--accent);
    color: #fff;
  }}
  div.menu-dropdown {{
    display: none;
    position: absolute;
    top: calc(100% + calc(2px * var(--ui)));
    left: 0;
    z-index: 10;
    min-width: calc(230px * var(--ui));
    padding: calc(5px * var(--ui));
    background: var(--chrome-bg);
    border: calc(1px * var(--ui)) solid var(--hairline);
    border-radius: calc(10px * var(--ui));
    box-shadow: 0 calc(10px * var(--ui)) calc(30px * var(--ui)) rgba(0,0,0,0.18);
  }}
  div.menu-dropdown.open {{
    display: block;
  }}
  a.menu-item, span.menu-item {{
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: calc(24px * var(--ui));
    height: calc(22px * var(--ui));
    padding: 0 calc(10px * var(--ui));
    border-radius: calc(5px * var(--ui));
    color: var(--text);
    text-decoration: none;
    font-size: calc(13px * var(--ui));
    white-space: nowrap;
  }}
  a.menu-item:hover {{
    background: var(--accent);
    color: #fff;
  }}
  a.menu-item:hover span.menu-shortcut {{
    color: rgba(255,255,255,0.85);
  }}
  span.menu-item.disabled {{
    color: var(--menu-disabled);
  }}
  span.menu-item.disabled span.menu-shortcut {{
    color: var(--menu-disabled);
  }}
  span.menu-label {{
    flex: 1;
  }}
  span.menu-shortcut {{
    color: var(--menu-secondary);
    font-size: calc(13px * var(--ui));
  }}
  span.menu-heading {{
    display: block;
    padding: calc(6px * var(--ui)) calc(10px * var(--ui)) calc(2px * var(--ui));
    color: var(--menu-secondary);
    font-size: calc(11px * var(--ui));
    font-weight: 600;
  }}
  span.menu-separator {{
    display: block;
    height: calc(1px * var(--ui));
    margin: calc(5px * var(--ui)) calc(10px * var(--ui));
    background: var(--hairline);
  }}
  nav#toolbar {{
    display: flex;
    align-items: center;
    gap: calc(10px * var(--ui));
    height: calc(44px * var(--ui));
    padding: 0 calc(10px * var(--ui));
    background: var(--chrome-bg);
    flex: none;
  }}
  .toolbar-side {{
    display: flex;
    align-items: center;
    gap: calc(4px * var(--ui));
    flex: 1 1 0;
    min-width: 0;
  }}
  #toolbar-trailing {{
    justify-content: flex-end;
  }}
  form.address {{
    position: relative;
    display: flex;
    align-items: center;
    flex: 0 1 auto;
    width: calc(600px * var(--ui));
    max-width: 55%;
    height: calc(30px * var(--ui));
    margin: 0;
    padding: 0;
    border: 0;
    border-radius: calc(8px * var(--ui));
    background: var(--field-fill);
  }}
  form.address:focus-within {{
    background: #fff;
    box-shadow: var(--focus-ring);
  }}
  form.address.focused {{
    background: #fff;
    box-shadow: var(--focus-ring);
  }}
  form.address input {{
    flex: 1 1 0;
    min-width: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    padding: 0 calc(4px * var(--ui)) 0 calc(30px * var(--ui));
    border: 0;
    border-radius: inherit;
    background: transparent;
    color: transparent;
    font-size: calc(13px * var(--ui));
    text-align: center;
    box-sizing: border-box;
  }}
  form.address input:focus {{
    outline: none;
    color: var(--text);
    text-align: left;
  }}
  form.address.focused input {{
    color: var(--text);
    text-align: left;
  }}
  span.address-title {{
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 calc(36px * var(--ui));
    color: var(--text);
    font-size: calc(13px * var(--ui));
    white-space: nowrap;
    overflow: hidden;
    pointer-events: none;
  }}
  form.address:focus-within span.address-title {{
    display: none;
  }}
  form.address.focused span.address-title {{
    display: none;
  }}
  span.search-icon {{
    position: absolute;
    color: var(--menu-secondary);
    top: 0;
    bottom: 0;
    left: calc(10px * var(--ui));
    display: flex;
    align-items: center;
    pointer-events: none;
  }}
  span.search-icon svg {{
    width: calc(14px * var(--ui));
    height: calc(14px * var(--ui));
  }}
  form.address a#reload {{
    min-width: calc(24px * var(--ui));
    height: calc(24px * var(--ui));
    margin-right: calc(3px * var(--ui));
    border-radius: calc(5px * var(--ui));
  }}
  form.address a#reload svg {{
    width: calc(15px * var(--ui));
    height: calc(15px * var(--ui));
  }}
  #zoom-label {{
    display: none;
    padding: calc(2px * var(--ui)) calc(8px * var(--ui));
    border-radius: calc(9px * var(--ui));
    background: var(--field-fill);
    color: var(--text-secondary);
    font-size: calc(11px * var(--ui));
    font-weight: 500;
  }}
  #zoom-label.visible {{
    display: block;
  }}
  #tab-strip {{
    display: flex;
    align-items: stretch;
    height: calc(28px * var(--ui));
    background: var(--tabbar-bg);
    border-top: calc(1px * var(--ui)) solid var(--hairline);
    border-bottom: calc(1px * var(--ui)) solid var(--hairline);
    flex: none;
  }}
  div.tab {{
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: calc(6px * var(--ui));
    flex: 1 1 0;
    min-width: 0;
    padding: 0 calc(28px * var(--ui));
    border-right: calc(1px * var(--ui)) solid var(--hairline);
    color: var(--text-secondary);
    font-size: calc(12px * var(--ui));
    overflow: hidden;
  }}
  div.tab:hover {{
    background: var(--tab-hover);
  }}
  div.tab.active {{
    background: var(--chrome-bg);
    color: var(--text);
  }}
  span.tab-favicon {{
    display: none;
  }}
  div.tab.loading span.tab-favicon {{
    display: block;
    flex: none;
    width: calc(6px * var(--ui));
    height: calc(6px * var(--ui));
    border-radius: 50%;
    background: var(--accent);
  }}
  span.tab-title {{
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }}
  a.tab-close {{
    position: absolute;
    color: var(--text-secondary);
    top: calc(5px * var(--ui));
    left: calc(6px * var(--ui));
    display: flex;
    align-items: center;
    justify-content: center;
    width: calc(18px * var(--ui));
    height: calc(18px * var(--ui));
    border-radius: calc(4px * var(--ui));
    text-decoration: none;
    opacity: 0;
  }}
  div.tab:hover a.tab-close, div.tab.active a.tab-close {{
    opacity: 1;
  }}
  a.tab-close:hover {{
    background: var(--btn-hover);
  }}
  a.tab-close svg {{
    width: calc(9px * var(--ui));
    height: calc(9px * var(--ui));
  }}
  a#new-tab {{
    width: calc(28px * var(--ui));
    min-width: calc(28px * var(--ui));
    height: auto;
    border-radius: 0;
  }}
  a#new-tab svg {{
    width: calc(15px * var(--ui));
    height: calc(15px * var(--ui));
  }}
  #find-bar {{
    display: none;
    align-items: center;
    justify-content: flex-end;
    gap: calc(6px * var(--ui));
    height: calc(36px * var(--ui));
    padding: 0 calc(10px * var(--ui));
    background: var(--chrome-bg);
    border-bottom: calc(1px * var(--ui)) solid var(--hairline);
    flex: none;
  }}
  #find-bar.open {{
    display: flex;
  }}
  div.find-field {{
    position: relative;
    display: flex;
    align-items: center;
    flex: 0 1 calc(420px * var(--ui));
    min-width: 0;
    height: calc(26px * var(--ui));
    border-radius: calc(7px * var(--ui));
    background: var(--field-fill);
  }}
  div.find-field:focus-within {{
    background: #fff;
    box-shadow: var(--focus-ring);
  }}
  #find-input {{
    flex: 1 1 0;
    min-width: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    padding: 0 calc(8px * var(--ui)) 0 calc(30px * var(--ui));
    border: 0;
    border-radius: inherit;
    background: transparent;
    color: var(--text);
    font-size: calc(13px * var(--ui));
    box-sizing: border-box;
  }}
  #find-input:focus {{
    outline: none;
  }}
  #find-count {{
    min-width: calc(56px * var(--ui));
    color: var(--text-secondary);
    font-size: calc(12px * var(--ui));
    text-align: center;
  }}
  #find-bar a.icon-btn svg {{
    width: calc(16px * var(--ui));
    height: calc(16px * var(--ui));
  }}
  #find-bar a.text-btn {{
    color: var(--text);
    font-size: calc(13px * var(--ui));
  }}
  #stage {{
    flex: 1;
    min-height: 0;
    min-width: 0;
    position: relative;
    display: flex;
    flex-direction: column;
  }}
  #stage.dock-right {{
    flex-direction: row;
  }}
  #devtools-split,
  #devtools {{
    display: none;
    flex: none;
  }}
  #stage.devtools-open #devtools-split {{
    display: block;
    background: #e3e3e3;
    flex: none;
  }}
  #stage.devtools-open.dock-bottom #devtools-split {{
    height: calc(6px * var(--ui));
    cursor: row-resize;
  }}
  #stage.devtools-open.dock-right #devtools-split {{
    width: calc(6px * var(--ui));
    cursor: col-resize;
  }}
  #stage.devtools-open #devtools {{
    display: flex;
    flex-direction: column;
    background: #fff;
    overflow: hidden;
    flex: none;
  }}
  #stage.devtools-open.dock-bottom #devtools {{
    border-top: calc(1px * var(--ui)) solid var(--hairline);
  }}
  #stage.devtools-open.dock-right #devtools {{
    border-left: calc(1px * var(--ui)) solid var(--hairline);
  }}
  div.devtools-tabs {{
    display: flex;
    align-items: center;
    gap: calc(2px * var(--ui));
    height: calc(28px * var(--ui));
    padding: 0 calc(6px * var(--ui));
    background: var(--chrome-bg);
    border-bottom: calc(1px * var(--ui)) solid var(--hairline);
    flex: none;
  }}
  a.devtools-tab {{
    padding: calc(3px * var(--ui)) calc(10px * var(--ui));
    border-radius: calc(5px * var(--ui));
    color: var(--text-secondary);
    text-decoration: none;
    font-size: calc(12px * var(--ui));
    font-weight: 500;
  }}
  a.devtools-tab:hover {{
    color: var(--text);
  }}
  a.devtools-tab.active {{
    background: var(--btn-hover);
    color: var(--text);
  }}
  div.devtools-pane {{
    display: none;
    flex: 1;
    min-height: 0;
    min-width: 0;
  }}
  div.devtools-pane.active {{
    display: block;
  }}
  p.devtools-placeholder {{
    margin: calc(8px * var(--ui));
    color: var(--text-secondary);
    font-size: calc(12px * var(--ui));
  }}
  div.devtools-console-toolbar {{
    display: flex;
    align-items: center;
    justify-content: flex-end;
    height: calc(24px * var(--ui));
    padding: 0 calc(4px * var(--ui));
    border-bottom: calc(1px * var(--ui)) solid var(--hairline);
    flex: none;
  }}
  a#devtools-console-clear {{
    height: calc(20px * var(--ui));
    padding: 0 calc(8px * var(--ui));
  }}
  #devtools-console-log {{
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: calc(11px * var(--ui));
    line-height: 1.4;
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0;
    box-sizing: border-box;
  }}
  #devtools-console-input {{
    flex: none;
    box-sizing: border-box;
    width: 100%;
    height: calc(26px * var(--ui));
    margin: 0;
    padding: 0 calc(8px * var(--ui));
    border: 0;
    border-top: calc(1px * var(--ui)) solid var(--hairline);
    border-radius: 0;
    background: #fff;
    color: var(--text);
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: calc(11px * var(--ui));
  }}
  #devtools-console-input:focus {{
    outline: none;
  }}
  #devtools-pane-console {{
    display: none;
    flex-direction: column;
  }}
  #devtools-pane-console.active {{
    display: flex;
  }}
  #devtools-pane-elements.active {{
    display: flex;
    flex-direction: column;
    min-height: 0;
  }}
  #devtools-elements-tree {{
    flex: 1;
    min-height: 0;
    overflow: auto;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: calc(11px * var(--ui));
    line-height: 1.35;
    padding: calc(4px * var(--ui)) 0;
  }}
  div.devtools-elements-row {{
    display: flex;
    align-items: baseline;
    padding: calc(1px * var(--ui)) calc(6px * var(--ui));
    color: var(--text);
    white-space: nowrap;
  }}
  div.devtools-elements-row:hover {{
    background: var(--field-fill);
  }}
  div.devtools-elements-row.selected {{
    background: rgba(0, 122, 255, 0.14);
  }}
  a.twist-link,
  a.devtools-elements-row-link {{
    color: inherit;
    text-decoration: none;
  }}
  a.twist-link {{
    flex: none;
    width: calc(12px * var(--ui));
  }}
  a.devtools-elements-row-link {{
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }}
  div.devtools-elements-row.more {{
    padding: calc(1px * var(--ui)) calc(6px * var(--ui));
    color: var(--text-secondary);
    font-style: italic;
  }}
  span.twist {{
    display: inline-block;
    width: calc(12px * var(--ui));
  }}
  #devtools-elements-detail {{
    flex: none;
    max-height: 28%;
    overflow: auto;
    border-top: calc(1px * var(--ui)) solid var(--hairline);
    padding: calc(6px * var(--ui));
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: calc(11px * var(--ui));
  }}
  div.devtools-elements-attr {{
    padding: calc(1px * var(--ui)) 0;
  }}
  span.attr-name {{ color: var(--accent); }}
  span.attr-value {{ color: var(--text-secondary); }}
  p.devtools-elements-detail-empty {{
    margin: 0;
    color: var(--text-secondary);
  }}
  #devtools-elements-computed {{
    flex: 1;
    min-height: 0;
    overflow: auto;
    border-top: calc(1px * var(--ui)) solid var(--hairline);
    padding: calc(6px * var(--ui));
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: calc(11px * var(--ui));
  }}
  p.devtools-computed-empty {{
    margin: 0;
    color: var(--text-secondary);
    font-style: italic;
  }}
  div.devtools-computed-row {{
    padding: calc(1px * var(--ui)) 0;
    white-space: pre-wrap;
    word-break: break-word;
  }}
  span.computed-name {{ color: var(--accent); }}
  span.computed-value {{ color: var(--text); }}
  div.devtools-box-model {{
    margin-bottom: calc(8px * var(--ui));
    font-size: calc(10px * var(--ui));
    color: var(--text-secondary);
    line-height: 1.4;
  }}
  div.bm-content {{
    margin: calc(4px * var(--ui));
    padding: calc(4px * var(--ui));
    background: var(--field-fill);
    text-align: center;
    color: var(--text);
  }}
  div.devtools-console-line {{
    padding: calc(3px * var(--ui)) calc(8px * var(--ui));
    border-bottom: calc(1px * var(--ui)) solid rgba(0,0,0,0.06);
    white-space: pre-wrap;
    word-break: break-word;
  }}
  div.devtools-console-line .console-ts,
  div.devtools-console-line .console-level,
  span.console-source {{
    display: inline-block;
  }}
  div.devtools-console-line .console-ts {{
    color: var(--text-tertiary);
    margin-right: calc(6px * var(--ui));
  }}
  div.devtools-console-line .console-level {{
    font-weight: 600;
    margin-right: calc(6px * var(--ui));
  }}
  div.devtools-console-line.level-log .console-level {{ color: #248a3d; }}
  div.devtools-console-line.level-info .console-level {{ color: #0066cc; }}
  div.devtools-console-line.level-warn .console-level {{ color: #a05a00; }}
  div.devtools-console-line.level-error .console-level {{ color: #d70015; }}
  div.devtools-console-line.level-debug .console-level {{ color: #6e6e73; }}
  span.console-source {{
    color: var(--text-secondary);
    margin-right: calc(6px * var(--ui));
  }}
  span.console-msg {{ color: var(--text); }}
  #inspect-hl {{
    position: fixed;
    display: none;
    pointer-events: none;
    box-sizing: border-box;
    border: calc(1px * var(--ui)) solid rgba(0, 122, 255, 0.85);
    background: rgba(0, 122, 255, 0.12);
    z-index: 2147483646;
  }}
  #inspect-label {{
    position: fixed;
    display: none;
    pointer-events: none;
    z-index: 2147483647;
    padding: calc(2px * var(--ui)) calc(6px * var(--ui));
    font-family: ui-monospace, monospace;
    font-size: calc(11px * var(--ui));
    color: #fff;
    background: rgba(0, 122, 255, 0.92);
    border-radius: calc(3px * var(--ui));
    white-space: nowrap;
    max-width: calc(420px * var(--ui));
    overflow: hidden;
    text-overflow: ellipsis;
  }}
  a.devtools-inspect-toggle.active {{
    background: var(--btn-pressed);
    box-shadow: inset 0 0 0 calc(1px * var(--ui)) rgba(0, 122, 255, 0.45);
  }}
  iframe#page {{
    flex: 1;
    width: 100%;
    border: 0;
    background: #fff;
    min-height: 0;
    min-width: 0;
  }}
  #loading {{
    height: calc(2px * var(--ui));
    background: var(--accent);
    flex: none;
    display: none;
  }}
  #loading.active {{
    display: block;
  }}
</style>
</head>
<body>
  {menu_bar}
  <nav id="toolbar">
    <div id="toolbar-leading" class="toolbar-side">
      <a id="back" class="icon-btn" href="glide://back" title="Back" aria-label="Back">{BACK_ICON}</a>
      <a id="forward" class="icon-btn" href="glide://forward" title="Forward" aria-label="Forward">{FORWARD_ICON}</a>
    </div>
    <form id="url-form" class="address" action="https://glide.invalid/go" method="get">
      <span class="address-icon search-icon" aria-hidden="true">{SEARCH_ICON}</span>
      <span id="url-title" class="address-title">{address_title}</span>
      <input id="url" name="q" type="text" value="{url}" spellcheck="false" autocomplete="off" aria-label="Address">
      <a id="reload" class="icon-btn" href="{reload_href}" title="{reload_title}" aria-label="Reload">{reload_icon}</a>
    </form>
    <div id="toolbar-trailing" class="toolbar-side">
      <span id="zoom-label"></span>
      <a id="js-toggle" class="text-btn" href="glide://toggle-js" title="Toggle JavaScript">{js_label}</a>
      <a id="bookmark-star" class="icon-btn" href="glide://bookmark" title="{star_title}" aria-label="{star_title}">{star}</a>
    </div>
  </nav>
  {tab_strip}
  <div id="find-bar">
    <div class="find-field">
      <span class="search-icon" aria-hidden="true">{SEARCH_ICON}</span>
      <input id="find-input" type="text" spellcheck="false" autocomplete="off" placeholder="Find in page">
    </div>
    <span id="find-count"></span>
    <a class="icon-btn" href="glide://find-prev" title="Previous match" aria-label="Previous match">{CHEVRON_UP_ICON}</a>
    <a class="icon-btn" href="glide://find-next" title="Next match" aria-label="Next match">{CHEVRON_DOWN_ICON}</a>
    <a class="text-btn" href="glide://find-close" title="Close find">Done</a>
  </div>
  <div id="loading" class="{loading_class}"></div>
  <div id="stage" class="dock-bottom">
    <iframe id="page" src="{url}"></iframe>
    <div id="devtools-split"></div>
    <aside id="devtools" tabindex="-1">
      <div class="devtools-tabs">
        <a id="devtools-inspect-toggle" class="icon-btn devtools-inspect-toggle" href="glide://devtools-inspect-toggle" title="Select an element" aria-label="Select an element">{INSPECT_ICON}</a>
        <a id="devtools-tab-console" class="devtools-tab active" href="glide://devtools-tab?tab=console">Console</a>
        <a id="devtools-tab-elements" class="devtools-tab" href="glide://devtools-tab?tab=elements">Elements</a>
        <a id="devtools-tab-network" class="devtools-tab" href="glide://devtools-tab?tab=network">Network</a>
      </div>
      <div id="devtools-pane-console" class="devtools-pane active">
        <div class="devtools-console-toolbar">
          <a id="devtools-console-clear" class="text-btn" href="glide://devtools-console-clear">Clear</a>
        </div>
        <div id="devtools-console-log"></div>
        <input id="devtools-console-input" type="text" spellcheck="false" autocomplete="off" placeholder="Evaluate JavaScript">
      </div>
      <div id="devtools-pane-elements" class="devtools-pane">
        <div id="devtools-elements-tree" tabindex="0"></div>
        <div id="devtools-elements-detail"></div>
        <div id="devtools-elements-computed"></div>
      </div>
      <div id="devtools-pane-network" class="devtools-pane"><p class="devtools-placeholder">Network log will appear here.</p></div>
    </aside>
  </div>
  <div id="inspect-hl"></div>
  <div id="inspect-label"></div>
</body>
</html>
"#
    )
}

/// Icon markup and title for the bookmark star.
fn bookmark_star(bookmarked: bool) -> (&'static str, &'static str) {
    if bookmarked {
        (STAR_FILLED_ICON, "Remove bookmark")
    } else {
        (STAR_ICON, "Add bookmark")
    }
}

/// Href, title and icon markup for the reload button, which turns into Stop while loading.
fn reload_button(loading: bool) -> (&'static str, &'static str, &'static str) {
    if loading {
        ("glide://stop", "Stop", STOP_ICON)
    } else {
        ("glide://reload", "Reload", RELOAD_ICON)
    }
}

fn tab_strip_inner_html(view: &ChromeViewState) -> String {
    let mut html = String::new();
    for (index, tab) in view.tabs.iter().enumerate() {
        let classes = if Some(index) == view.active_index {
            "tab active"
        } else {
            "tab"
        };
        let classes = if tab.loading {
            format!("{classes} loading")
        } else {
            classes.to_string()
        };
        let title = if tab.title.trim().is_empty() {
            "New tab".to_string()
        } else {
            escape_attr(&tab.title)
        };
        html.push_str(&format!(
            r#"<div id="tab-{id}" class="{classes}"><a class="tab-close" href="glide://close-tab?id={id}" title="Close tab" aria-label="Close tab">{CLOSE_ICON}</a><span class="tab-favicon"></span><span class="tab-title">{title}</span></div>
"#,
            id = tab.id,
        ));
    }
    html.push_str(&format!(
        "<a id=\"new-tab\" class=\"icon-btn\" href=\"glide://new-tab\" title=\"New tab\" aria-label=\"New tab\">{PLUS_ICON}</a>\n"
    ));
    html
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuItemKind {
    Action,
    Disabled,
    Separator,
    Heading,
}

struct MenuItem {
    label: String,
    href: String,
    shortcut: &'static str,
    kind: MenuItemKind,
}

impl MenuItem {
    fn action(label: &str, href: &str, shortcut: &'static str) -> Self {
        Self {
            label: label.to_string(),
            href: href.to_string(),
            shortcut,
            kind: MenuItemKind::Action,
        }
    }

    fn disabled(label: &str, shortcut: &'static str) -> Self {
        Self {
            label: label.to_string(),
            href: String::new(),
            shortcut,
            kind: MenuItemKind::Disabled,
        }
    }

    fn separator() -> Self {
        Self {
            label: String::new(),
            href: String::new(),
            shortcut: "",
            kind: MenuItemKind::Separator,
        }
    }

    fn heading(label: &str) -> Self {
        Self {
            label: label.to_string(),
            href: String::new(),
            shortcut: "",
            kind: MenuItemKind::Heading,
        }
    }
}

fn menu_items_html(menu: MenuId, view: &ChromeViewState) -> String {
    let mut items = Vec::new();
    let single_tab = view.tabs.len() <= 1;

    match menu {
        MenuId::File => {
            items.push(MenuItem::action("New Tab", "glide://new-tab", "⌘T"));
            items.push(MenuItem::separator());
            if single_tab {
                items.push(MenuItem::disabled("Close Tab", "⌘W"));
            } else {
                items.push(MenuItem::action(
                    "Close Tab",
                    "glide://close-active-tab",
                    "⌘W",
                ));
            }
            items.push(MenuItem::action(
                "Close Window",
                "glide://close-window",
                "⇧⌘W",
            ));
        }
        MenuId::Edit => {
            items.push(MenuItem::action("Find", "glide://find", "⌘F"));
            items.push(MenuItem::separator());
            items.push(MenuItem::heading("Editing"));
            items.push(MenuItem::disabled("Undo", "⌘Z"));
            items.push(MenuItem::disabled("Redo", "⇧⌘Z"));
            items.push(MenuItem::disabled("Cut", "⌘X"));
            items.push(MenuItem::disabled("Copy", "⌘C"));
            items.push(MenuItem::disabled("Paste", "⌘V"));
            items.push(MenuItem::disabled("Select All", "⌘A"));
        }
        MenuId::View => {
            items.push(MenuItem::action("Zoom In", "glide://zoom-in", "⌘+"));
            items.push(MenuItem::action("Zoom Out", "glide://zoom-out", "⌘−"));
            items.push(MenuItem::action("Reset Zoom", "glide://zoom-reset", "⌘0"));
            items.push(MenuItem::separator());
            items.push(MenuItem::action("Reload Page", "glide://reload", "⌘R"));
            items.push(MenuItem::separator());
            items.push(MenuItem::action(
                "Toggle Developer Tools",
                "glide://devtools",
                "F12",
            ));
            items.push(MenuItem::separator());
            let js_label = if view.js_enabled {
                "Turn JavaScript Off"
            } else {
                "Turn JavaScript On"
            };
            items.push(MenuItem::action(js_label, "glide://toggle-js", "⇧⌘J"));
        }
        MenuId::History => {
            if view.can_go_back {
                items.push(MenuItem::action("Back", "glide://back", "⌘["));
            } else {
                items.push(MenuItem::disabled("Back", "⌘["));
            }
            if view.can_go_forward {
                items.push(MenuItem::action("Forward", "glide://forward", "⌘]"));
            } else {
                items.push(MenuItem::disabled("Forward", "⌘]"));
            }
            items.push(MenuItem::separator());
            if view.page_loading {
                items.push(MenuItem::action("Stop Loading", "glide://stop", "⎋"));
            } else {
                items.push(MenuItem::action("Reload Page", "glide://reload", "⌘R"));
            }
        }
        MenuId::Bookmarks => {
            let label = if view.bookmarked {
                "Remove Bookmark"
            } else {
                "Add Bookmark"
            };
            items.push(MenuItem::action(label, "glide://bookmark", "⌘D"));
            items.push(MenuItem::separator());
            items.push(MenuItem::action(
                "Show Bookmarks",
                "glide://bookmarks",
                "⇧⌘B",
            ));
        }
        MenuId::Tabs => {
            items.push(MenuItem::action("New Tab", "glide://new-tab", "⌘T"));
            items.push(MenuItem::action(
                "Duplicate Tab",
                "glide://duplicate-tab",
                "",
            ));
            items.push(MenuItem::separator());
            if single_tab {
                items.push(MenuItem::disabled("Next Tab", "⌃⇥"));
                items.push(MenuItem::disabled("Previous Tab", "⌃⇧⇥"));
            } else {
                items.push(MenuItem::action("Next Tab", "glide://next-tab", "⌃⇥"));
                items.push(MenuItem::action("Previous Tab", "glide://prev-tab", "⌃⇧⇥"));
            }
        }
        MenuId::Help => {
            items.push(MenuItem::action(
                "Keyboard Shortcuts",
                "glide://help",
                "⇧⌘?",
            ));
            items.push(MenuItem::separator());
            items.push(MenuItem::heading("Glide"));
            items.push(MenuItem::disabled("About Glide", ""));
        }
    }

    if menu == MenuId::Tabs && view.tabs.len() > 1 {
        items.push(MenuItem::separator());
        items.push(MenuItem::heading("Open Tabs"));
        for (index, tab) in view.tabs.iter().enumerate() {
            let title = if tab.title.trim().is_empty() {
                format!("{}: New tab", index + 1)
            } else {
                format!("{}: {}", index + 1, tab.title)
            };
            let is_active = Some(index) == view.active_index;
            let href = format!("glide://select-tab?id={}", tab.id);
            if is_active {
                items.push(MenuItem::action(&title, &href, "✓"));
            } else {
                items.push(MenuItem::action(&title, &href, ""));
            }
        }
    }

    let mut html = String::new();
    for item in items {
        match item.kind {
            MenuItemKind::Action => {
                html.push_str(&format!(
                    "<a class=\"menu-item\" href=\"{href}\"><span class=\"menu-label\">{label}</span><span class=\"menu-shortcut\">{shortcut}</span></a>\n",
                    href = escape_attr(&item.href),
                    label = escape_attr(&item.label),
                    shortcut = item.shortcut,
                ));
            }
            MenuItemKind::Disabled => {
                html.push_str(&format!(
                    "<span class=\"menu-item disabled\"><span class=\"menu-label\">{label}</span><span class=\"menu-shortcut\">{shortcut}</span></span>\n",
                    label = escape_attr(&item.label),
                    shortcut = item.shortcut,
                ));
            }
            MenuItemKind::Separator => {
                html.push_str("<span class=\"menu-separator\" role=\"separator\"></span>\n");
            }
            MenuItemKind::Heading => {
                html.push_str(&format!(
                    "<span class=\"menu-heading\">{label}</span>\n",
                    label = escape_attr(&item.label),
                ));
            }
        }
    }
    html
}

fn menu_bar_inner_html(view: &ChromeViewState) -> String {
    let mut html = String::new();
    for menu in MenuId::ALL {
        let active = if view.open_menu == Some(menu) {
            " active"
        } else {
            ""
        };
        html.push_str(&format!(
            "<div class=\"menu\"><a id=\"menu-{name}\" class=\"menu-trigger{active}\" href=\"glide://menu-open?menu={name}\">{label}</a>",
            name = menu.name(),
            label = menu.label(),
        ));
        let open = if view.open_menu == Some(menu) {
            " open"
        } else {
            ""
        };
        html.push_str(&format!(
            "<div id=\"menu-{name}-dropdown\" class=\"menu-dropdown{open}\">\n{items}</div></div>\n",
            name = menu.name(),
            items = menu_items_html(menu, view),
        ));
    }
    html
}

/// Replace the tab strip inside an existing chrome document.
pub fn render_tab_strip(doc: &mut BaseDocument, view: &ChromeViewState) {
    if let Some(id) = doc.get_element_by_id("tab-strip") {
        replace_inner_html(doc, id, &tab_strip_inner_html(view));
    }
}

/// Replace the menu bar inside an existing chrome document.
pub fn render_menu_bar(doc: &mut BaseDocument, view: &ChromeViewState) {
    if let Some(id) = doc.get_element_by_id("menu-bar") {
        replace_inner_html(doc, id, &menu_bar_inner_html(view));
    }
}

fn replace_inner_html(doc: &mut BaseDocument, id: blitz_traits::node_id::NodeId, html: &str) {
    let mut m = doc.mutate();
    let children: Vec<_> = m.child_ids(id).into_iter().collect();
    for child in children {
        m.remove_and_drop_node(child);
    }
    crate::parse::parse_inner_html_into_mutator(&mut m, id, html, false);
}

use blitz_dom::{BaseDocument, local_name, qual_name};
use blitz_traits::node_id::NodeId;

/// Whether the address bar currently has keyboard focus.
pub fn address_bar_focused(doc: &BaseDocument) -> bool {
    let Some(url_id) = doc.get_element_by_id("url") else {
        return false;
    };
    doc.get_focussed_node_id() == Some(url_id)
}

/// Whether the committed URL should be written into the address field.
pub fn should_write_committed_address(focused: bool, written: &str, shown: &str) -> bool {
    !focused && written != shown
}

/// Update the committed URL when the user is not editing the address bar.
pub fn sync_committed_address(doc: &mut BaseDocument, url: &str, address_written: &mut String) {
    let focused = address_bar_focused(doc);
    if should_write_committed_address(focused, address_written, url) {
        set_address(doc, url);
        *address_written = url.to_string();
    }
}

pub fn set_address(doc: &mut BaseDocument, url: &str) {
    let Some(id) = doc.get_element_by_id("url") else {
        return;
    };
    doc.mutate().set_attribute(id, qual_name!("value"), url);
}

/// Show the page title in the address field; the URL stays in the input value.
pub fn set_address_title(doc: &mut BaseDocument, title: &str) {
    let Some(id) = doc.get_element_by_id("url-title") else {
        return;
    };
    let current = doc
        .get_node(id)
        .map(|node| node.text_content())
        .unwrap_or_default();
    if current == title {
        return;
    }
    let mut m = doc.mutate();
    let children: Vec<_> = m.child_ids(id).into_iter().collect();
    for child in children {
        m.remove_and_drop_node(child);
    }
    let text_id = m.create_text_node(title);
    m.append_children(id, &[text_id]);
}

/// Mark the address field as focused, so it shows the URL instead of the title.
pub fn set_address_focused(doc: &mut BaseDocument, focused: bool) {
    let Some(id) = doc.get_element_by_id("url-form") else {
        return;
    };
    let class = if focused {
        "address focused"
    } else {
        "address"
    };
    let current = doc
        .get_node(id)
        .and_then(|node| node.attr(local_name!("class")))
        .unwrap_or_default();
    if current == class {
        return;
    }
    doc.mutate().set_attribute(id, qual_name!("class"), class);
}

pub fn set_loading_ui(doc: &mut BaseDocument, loading: bool) {
    if let Some(id) = doc.get_element_by_id("loading") {
        let class = if loading { "active" } else { "" };
        doc.mutate().set_attribute(id, qual_name!("class"), class);
    }
    if let Some(id) = doc.get_element_by_id("reload") {
        let (href, title, icon) = reload_button(loading);
        let mut m = doc.mutate();
        m.set_attribute(id, qual_name!("href"), href);
        m.set_attribute(id, qual_name!("title"), title);
        let children: Vec<_> = m.child_ids(id).into_iter().collect();
        for child in children {
            m.remove_and_drop_node(child);
        }
        crate::parse::parse_inner_html_into_mutator(&mut m, id, icon, false);
    }
}

pub fn find_bar_open(doc: &BaseDocument) -> bool {
    doc.get_element_by_id("find-bar").is_some_and(|id| {
        doc.get_node(id)
            .and_then(|n| n.attr(local_name!("class")))
            .is_some_and(|c| c.split_whitespace().any(|part| part == "open"))
    })
}

pub fn devtools_console_input_focused(doc: &BaseDocument) -> bool {
    let Some(id) = doc.get_element_by_id("devtools-console-input") else {
        return false;
    };
    doc.get_focussed_node_id() == Some(id)
}

pub fn devtools_console_input_value(doc: &BaseDocument) -> Option<String> {
    let id = doc.get_element_by_id("devtools-console-input")?;
    let input = doc.get_node(id)?.element_data()?.text_input_data()?;
    Some(input.editor.text().to_string())
}

pub fn set_devtools_console_input_value(doc: &mut BaseDocument, value: &str) {
    let Some(id) = doc.get_element_by_id("devtools-console-input") else {
        return;
    };
    if let Some(node) = doc.get_node_mut(id)
        && let Some(input) = node
            .element_data_mut()
            .and_then(|el| el.text_input_data_mut())
    {
        input.editor.set_text(value);
    }
}

pub fn clear_devtools_console_input(doc: &mut BaseDocument) {
    set_devtools_console_input_value(doc, "");
}

pub fn focus_devtools_console_input(doc: &mut BaseDocument) {
    if let Some(id) = doc.get_element_by_id("devtools-console-input") {
        doc.set_focus_to(id);
    }
}

pub fn focus_devtools_elements_tree(doc: &mut BaseDocument) {
    if let Some(id) = doc.get_element_by_id("devtools-elements-tree") {
        doc.set_focus_to(id);
    }
}

pub fn devtools_elements_tree_focused(doc: &BaseDocument) -> bool {
    let Some(tree_id) = doc.get_element_by_id("devtools-elements-tree") else {
        return false;
    };
    let Some(mut focus) = doc.get_focussed_node_id() else {
        return false;
    };
    loop {
        if focus == tree_id {
            return true;
        }
        let Some(parent) = doc.get_node(focus).and_then(|n| n.parent) else {
            return false;
        };
        focus = parent;
    }
}

pub fn find_input_focused(doc: &BaseDocument) -> bool {
    let Some(id) = doc.get_element_by_id("find-input") else {
        return false;
    };
    doc.get_focussed_node_id() == Some(id)
}

pub fn find_input_value(doc: &BaseDocument) -> Option<String> {
    let id = doc.get_element_by_id("find-input")?;
    let input = doc.get_node(id)?.element_data()?.text_input_data()?;
    Some(input.editor.text().to_string())
}

pub fn set_find_input_value(doc: &mut BaseDocument, value: &str) {
    let Some(id) = doc.get_element_by_id("find-input") else {
        return;
    };
    doc.with_text_input(id, |mut driver| {
        driver.editor.set_text(value);
        driver.refresh_layout();
    });
}

pub fn set_find_bar_open(doc: &mut BaseDocument, open: bool) {
    let Some(id) = doc.get_element_by_id("find-bar") else {
        return;
    };
    let class = if open { "open" } else { "" };
    doc.mutate().set_attribute(id, qual_name!("class"), class);
}

pub fn focus_find_input(doc: &mut BaseDocument) {
    if let Some(id) = doc.get_element_by_id("find-input") {
        doc.set_focus_to(id);
    }
}

pub fn set_find_count(doc: &mut BaseDocument, current: usize, total: usize) {
    let Some(id) = doc.get_element_by_id("find-count") else {
        return;
    };
    let text = if total == 0 {
        String::new()
    } else {
        format!("{current} of {total}")
    };
    let mut m = doc.mutate();
    let children: Vec<_> = m.child_ids(id).into_iter().collect();
    for child in children {
        m.remove_and_drop_node(child);
    }
    let text_id = m.create_text_node(&text);
    m.append_children(id, &[text_id]);
}

pub fn set_zoom_label(doc: &mut BaseDocument, label: Option<&str>) {
    let Some(id) = doc.get_element_by_id("zoom-label") else {
        return;
    };
    let mut m = doc.mutate();
    let class = if label.is_some() { "visible" } else { "" };
    m.set_attribute(id, qual_name!("class"), class);
    let children: Vec<_> = m.child_ids(id).into_iter().collect();
    for child in children {
        m.remove_and_drop_node(child);
    }
    if let Some(label) = label {
        let text_id = m.create_text_node(label);
        m.append_children(id, &[text_id]);
    }
}

pub fn set_bookmark_star(doc: &mut BaseDocument, bookmarked: bool) {
    let Some(id) = doc.get_element_by_id("bookmark-star") else {
        return;
    };
    let (icon, title) = bookmark_star(bookmarked);
    let mut m = doc.mutate();
    m.set_attribute(id, qual_name!("title"), title);
    m.set_attribute(id, qual_name!("aria-label"), title);
    let children: Vec<_> = m.child_ids(id).into_iter().collect();
    for child in children {
        m.remove_and_drop_node(child);
    }
    crate::parse::parse_inner_html_into_mutator(&mut m, id, icon, false);
}

pub fn set_ui_scale(doc: &mut BaseDocument, ui: f32) {
    let root = doc.root_element().id;
    let style = format!("--ui: {ui}");
    doc.mutate()
        .set_attribute(root, qual_name!("style"), &style);
}

/// Chrome `--ui` scale factor from the root element style (defaults to `1.0`).
pub fn chrome_ui_scale(chrome: &BaseDocument) -> f32 {
    let root = chrome.root_element().id;
    chrome
        .get_node(root)
        .and_then(|n| n.attr(local_name!("style")))
        .and_then(|style| {
            style.split(';').find_map(|part| {
                let part = part.trim();
                part.strip_prefix("--ui:")
                    .or_else(|| part.strip_prefix("--ui :"))
                    .and_then(|v| v.trim().parse::<f32>().ok())
            })
        })
        .filter(|ui| *ui > 0.0)
        .unwrap_or(1.0)
}

use crate::devtools::{DevToolsDock, DevToolsState, DevToolsTab};

pub fn devtools_open(doc: &BaseDocument) -> bool {
    doc.get_element_by_id("stage").is_some_and(|id| {
        doc.get_node(id)
            .and_then(|n| n.attr(local_name!("class")))
            .is_some_and(|c| c.split_whitespace().any(|part| part == "devtools-open"))
    })
}

pub fn devtools_focused(doc: &BaseDocument) -> bool {
    let Some(devtools_id) = doc.get_element_by_id("devtools") else {
        return false;
    };
    let Some(focus) = doc.get_focussed_node_id() else {
        return false;
    };
    if focus == devtools_id {
        return true;
    }
    is_ancestor_of(doc, devtools_id, focus)
}

fn is_ancestor_of(doc: &BaseDocument, ancestor: NodeId, node: NodeId) -> bool {
    let mut current = node;
    while let Some(parent) = doc.get_node(current).and_then(|n| n.parent) {
        if parent == ancestor {
            return true;
        }
        current = parent;
    }
    false
}

fn class_contains_token(class: &str, token: &str) -> bool {
    class.split_whitespace().any(|part| part == token)
}

fn node_keeps_menu_open(doc: &BaseDocument, node: NodeId) -> bool {
    let mut current = node;
    loop {
        if doc
            .get_node(current)
            .and_then(|n| n.attr(local_name!("class")))
            .is_some_and(|class| {
                class_contains_token(class, "menu-dropdown")
                    || class_contains_token(class, "menu-trigger")
            })
        {
            return true;
        }
        let Some(parent) = doc.get_node(current).and_then(|n| n.parent) else {
            return false;
        };
        current = parent;
    }
}

/// Whether a pointer at these chrome coordinates is on the open dropdown or a
/// menu-bar trigger, where a press must leave the menu open.
pub fn pointer_keeps_menu_open(doc: &mut BaseDocument, x: f32, y: f32) -> bool {
    hit_test_chrome(doc, x, y).is_some_and(|node| node_keeps_menu_open(doc, node))
}

pub fn stage_layout_axis_css_px(doc: &BaseDocument, dock: DevToolsDock) -> Option<f32> {
    let id = doc.get_element_by_id("stage")?;
    let layout = doc.get_node(id)?.final_layout();
    let axis = match dock {
        DevToolsDock::Bottom => layout.size.height,
        DevToolsDock::Right => layout.size.width,
    };
    if axis <= 0.0 {
        return None;
    }
    Some(axis)
}

/// Logical pixels for clamping: CSS layout size times page zoom (matches `calc(Npx * var(--ui))`).
pub fn stage_axis_budget_logical_px(doc: &BaseDocument, dock: DevToolsDock) -> Option<i32> {
    let css = stage_layout_axis_css_px(doc, dock)?;
    let zoom = doc.viewport().zoom();
    let logical = (css * zoom).round() as i32;
    Some((logical - crate::devtools::SPLITTER_PX).max(0))
}

pub fn apply_devtools_state(doc: &mut BaseDocument, state: &DevToolsState) {
    let Some(stage_id) = doc.get_element_by_id("stage") else {
        return;
    };
    let devtools_id = doc.get_element_by_id("devtools");
    let tab_ids: Vec<(NodeId, NodeId, bool)> = [
        DevToolsTab::Console,
        DevToolsTab::Elements,
        DevToolsTab::Network,
    ]
    .into_iter()
    .filter_map(|tab| {
        let tab_id = doc.get_element_by_id(&format!("devtools-tab-{}", tab.name()))?;
        let pane_id = doc.get_element_by_id(&format!("devtools-pane-{}", tab.name()))?;
        Some((tab_id, pane_id, state.tab == tab))
    })
    .collect();
    let dock_class = match state.dock {
        DevToolsDock::Bottom => "dock-bottom",
        DevToolsDock::Right => "dock-right",
    };
    let stage_class = if state.open {
        format!("devtools-open {dock_class}")
    } else {
        dock_class.to_string()
    };
    let axis_style = if state.open {
        match state.dock {
            DevToolsDock::Bottom => format!("height: calc({}px * var(--ui))", state.size_px),
            DevToolsDock::Right => format!("width: calc({}px * var(--ui))", state.size_px),
        }
    } else {
        String::new()
    };
    let mut m = doc.mutate();
    m.set_attribute(stage_id, qual_name!("class"), &stage_class);
    if let Some(id) = devtools_id {
        m.set_attribute(id, qual_name!("style"), &axis_style);
    }
    for (tab_el, pane_el, active) in tab_ids {
        let tab_class = if active {
            "devtools-tab active"
        } else {
            "devtools-tab"
        };
        let pane_class = if active {
            "devtools-pane active"
        } else {
            "devtools-pane"
        };
        m.set_attribute(tab_el, qual_name!("class"), tab_class);
        m.set_attribute(pane_el, qual_name!("class"), pane_class);
    }
}

pub fn node_is_devtools_split(doc: &BaseDocument, node_id: NodeId) -> bool {
    doc.get_element_by_id("devtools-split")
        .is_some_and(|split_id| node_id == split_id || is_ancestor_of(doc, split_id, node_id))
}

pub fn hit_test_chrome(doc: &mut BaseDocument, x: f32, y: f32) -> Option<NodeId> {
    let pruned = crate::js::prune_stale_paint_children(doc);
    if pruned > 0 {
        doc.resolve(0.0);
    }
    doc.hit(x, y).map(|hit| hit.node_id)
}

pub fn focus_devtools_panel(doc: &mut BaseDocument) {
    if let Some(id) = doc.get_element_by_id("devtools") {
        doc.set_focus_to(id);
    }
}

pub fn set_inspect_toggle_active(doc: &mut BaseDocument, active: bool) {
    let Some(id) = doc.get_element_by_id("devtools-inspect-toggle") else {
        return;
    };
    let class = if active {
        "icon-btn devtools-inspect-toggle active"
    } else {
        "icon-btn devtools-inspect-toggle"
    };
    doc.mutate().set_attribute(id, qual_name!("class"), class);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DevToolsFocusRestore {
    pub chrome_focus: Option<NodeId>,
    pub page_inner_focus: Option<NodeId>,
}

pub fn capture_devtools_focus_restore(doc: &BaseDocument) -> DevToolsFocusRestore {
    let chrome_focus = doc.get_focussed_node_id();
    let page_inner_focus = (|| {
        let page_id = doc.get_element_by_id("page")?;
        if doc.get_focussed_node_id() != Some(page_id) {
            return None;
        }
        doc.get_node(page_id)?
            .subdoc()?
            .inner()
            .get_focussed_node_id()
    })();
    DevToolsFocusRestore {
        chrome_focus,
        page_inner_focus,
    }
}

pub fn restore_devtools_focus(doc: &mut BaseDocument, saved: DevToolsFocusRestore) {
    let devtools_id = doc.get_element_by_id("devtools");
    if let Some(inner_id) = saved.page_inner_focus {
        let Some(page_id) = doc.get_element_by_id("page") else {
            return;
        };
        if doc.get_node(inner_id).is_some() {
            doc.set_focus_to(page_id);
            if let Some(sub) = doc.get_node_mut(page_id).and_then(|n| n.subdoc_mut()) {
                sub.inner_mut().set_focus_to(inner_id);
            }
            return;
        }
    }
    if let Some(id) = saved.chrome_focus
        && doc.get_node(id).is_some()
        && devtools_id != Some(id)
        && !devtools_id.is_some_and(|devtools| is_ancestor_of(doc, devtools, id))
    {
        doc.set_focus_to(id);
        return;
    }
    if let Some(page_id) = doc.get_element_by_id("page") {
        doc.set_focus_to(page_id);
    }
}

pub fn node_layout_size(doc: &BaseDocument, element_id: &str) -> Option<(f32, f32)> {
    let id = doc.get_element_by_id(element_id)?;
    let layout = doc.get_node(id)?.final_layout();
    Some((layout.size.width, layout.size.height))
}

pub fn toolbar_layout_height(doc: &BaseDocument) -> Option<f32> {
    doc.get_element_by_id("toolbar")
        .and_then(|id| doc.get_node(id))
        .map(|n| n.final_layout().size.height)
}

fn escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        ChromeViewState, MenuId, NavCommand, TabView, chrome_document_from_state,
        command_closes_menu, command_from_url, pointer_keeps_menu_open, render_menu_bar,
        set_address_focused, set_bookmark_star, set_loading_ui, set_ui_scale,
        should_write_committed_address, toggle_open_menu, toolbar_layout_height,
    };
    use blitz_dom::{BaseDocument, DocumentConfig, local_name};
    use blitz_traits::node_id::NodeId;
    use blitz_traits::shell::{ColorScheme, Viewport};
    use url::Url;

    use crate::chrome::chrome_document;
    use crate::html::GlideHtmlProvider;
    use crate::parse;

    #[test]
    fn classifies_chrome_actions() {
        assert_eq!(
            command_from_url(&Url::parse("glide://back").unwrap()),
            Some(NavCommand::Back)
        );
        assert_eq!(
            command_from_url(&Url::parse("glide://forward").unwrap()),
            Some(NavCommand::Forward)
        );
        assert_eq!(
            command_from_url(&Url::parse("glide://reload").unwrap()),
            Some(NavCommand::Reload)
        );
        assert_eq!(
            command_from_url(&Url::parse("glide://stop").unwrap()),
            Some(NavCommand::Stop)
        );
        assert_eq!(
            command_from_url(&Url::parse("glide://toggle-js").unwrap()),
            Some(NavCommand::ToggleJs)
        );
        assert_eq!(
            command_from_url(&Url::parse("glide://bookmark").unwrap()),
            Some(NavCommand::ToggleBookmark)
        );
    }

    #[test]
    fn tab_strip_renders_tabs_and_controls() {
        let view = ChromeViewState {
            url: "https://a/".into(),
            tabs: vec![TabView {
                id: 4,
                title: "Example".into(),
                ..TabView::default()
            }],
            active_index: Some(0),
            ..ChromeViewState::default()
        };
        let html = chrome_document_from_state(&view);
        assert!(html.contains("id=\"tab-strip\""));
        assert!(html.contains("Example"));
        assert!(html.contains("id=\"new-tab\""));
        assert!(html.contains("glide://close-tab?id=4"));
    }

    #[test]
    fn active_tab_is_marked_in_tab_strip() {
        let view = ChromeViewState {
            tabs: vec![
                TabView {
                    id: 1,
                    title: "A".into(),
                    ..TabView::default()
                },
                TabView {
                    id: 2,
                    title: "B".into(),
                    ..TabView::default()
                },
            ],
            active_index: Some(1),
            ..ChromeViewState::default()
        };
        let html = chrome_document_from_state(&view);
        assert!(html.contains("id=\"tab-1\" class=\"tab\""));
        assert!(html.contains("id=\"tab-2\" class=\"tab active\""));
    }

    #[test]
    fn tab_close_buttons_carry_tab_ids() {
        let view = ChromeViewState {
            tabs: vec![
                TabView {
                    id: 7,
                    ..TabView::default()
                },
                TabView {
                    id: 9,
                    ..TabView::default()
                },
            ],
            ..ChromeViewState::default()
        };
        let html = chrome_document_from_state(&view);
        assert!(html.contains("glide://close-tab?id=7"));
        assert!(html.contains("glide://close-tab?id=9"));
    }

    #[test]
    fn menu_bar_renders_all_menus() {
        let html = chrome_document_from_state(&ChromeViewState::default());
        for menu in MenuId::ALL {
            assert!(html.contains(&format!("id=\"menu-{}\"", menu.name())));
        }
    }

    #[test]
    fn menu_open_command_exposes_items() {
        let url = Url::parse("glide://menu-open?menu=file").unwrap();
        assert_eq!(
            command_from_url(&url),
            Some(NavCommand::MenuOpen(MenuId::File))
        );
        let view = ChromeViewState {
            open_menu: Some(MenuId::File),
            ..ChromeViewState::default()
        };
        let html = chrome_document_from_state(&view);
        assert!(html.contains("class=\"menu-dropdown open\""));
        assert!(html.contains("Close Window"));
    }

    #[test]
    fn history_disables_back_and_forward_when_unavailable() {
        let html = chrome_document_from_state(&ChromeViewState::default());
        assert!(
            html.contains(
                "<span class=\"menu-item disabled\"><span class=\"menu-label\">Back</span>"
            )
        );
        assert!(html.contains(
            "<span class=\"menu-item disabled\"><span class=\"menu-label\">Forward</span>"
        ));
        assert!(!html.contains("<a class=\"menu-item\" href=\"glide://back\""));
        assert!(!html.contains("<a class=\"menu-item\" href=\"glide://forward\""));
    }

    #[test]
    fn history_enables_back_and_forward_when_available() {
        let view = ChromeViewState {
            can_go_back: true,
            can_go_forward: true,
            ..ChromeViewState::default()
        };
        let html = chrome_document_from_state(&view);
        assert!(html.contains("href=\"glide://back\""));
        assert!(html.contains("href=\"glide://forward\""));
    }

    #[test]
    fn close_tab_is_disabled_with_a_single_tab() {
        let view = ChromeViewState {
            tabs: vec![TabView {
                id: 1,
                title: "Only".into(),
                ..TabView::default()
            }],
            active_index: Some(0),
            ..ChromeViewState::default()
        };
        let html = chrome_document_from_state(&view);
        assert!(html.contains(
            "<span class=\"menu-item disabled\"><span class=\"menu-label\">Close Tab</span>"
        ));
        assert!(!html.contains("href=\"glide://close-active-tab\""));
    }

    #[test]
    fn tab_cycling_is_disabled_with_a_single_tab() {
        let view = ChromeViewState {
            tabs: vec![TabView {
                id: 1,
                ..TabView::default()
            }],
            ..ChromeViewState::default()
        };
        let html = chrome_document_from_state(&view);
        assert!(html.contains(
            "<span class=\"menu-item disabled\"><span class=\"menu-label\">Next Tab</span>"
        ));
        assert!(html.contains(
            "<span class=\"menu-item disabled\"><span class=\"menu-label\">Previous Tab</span>"
        ));
    }

    #[test]
    fn toolbar_has_navigation_address_and_actions() {
        let view = ChromeViewState {
            url: "https://example.test/".into(),
            title: "Example Domain".into(),
            ..ChromeViewState::default()
        };
        let html = chrome_document_from_state(&view);
        assert!(html.contains("id=\"toolbar-leading\""));
        assert!(html.contains("id=\"toolbar-trailing\""));
        assert!(html.contains("id=\"url-form\""));
        assert!(html.contains("class=\"address\""));
    }

    #[test]
    fn address_field_shows_the_page_title() {
        let view = ChromeViewState {
            url: "https://example.test/".into(),
            title: "Example Domain".into(),
            ..ChromeViewState::default()
        };
        let html = chrome_document_from_state(&view);
        assert!(html.contains("id=\"url-title\""));
        assert!(html.contains("Example Domain"));
        assert!(html.contains("value=\"https://example.test/\""));
    }

    #[test]
    fn toolbar_uses_borderless_icon_buttons() {
        let html = chrome_document_from_state(&ChromeViewState::default());
        assert!(html.contains("class=\"icon-btn\""));
        let start = html.find(".icon-btn {").expect("icon button style");
        let block = &html[start..start + 400];
        assert!(block.contains("border: 0;"), "{block}");
        assert!(!block.contains("1px solid"), "{block}");
    }

    #[test]
    fn address_field_marks_the_focus_state() {
        let mut doc = parse_chrome(&chrome_document_from_state(&ChromeViewState::default()));
        set_address_focused(&mut doc, true);
        let id = doc.get_element_by_id("url-form").expect("address form");
        assert!(class_of(&doc, id).contains("focused"));
        set_address_focused(&mut doc, false);
        assert!(!class_of(&doc, id).contains("focused"));
    }

    #[test]
    fn menu_action_closes_dropdown() {
        let action = command_from_url(&Url::parse("glide://back").unwrap()).expect("menu action");
        assert!(command_closes_menu(&action));
        assert!(!command_closes_menu(&NavCommand::MenuOpen(MenuId::File)));
        assert!(!command_closes_menu(&NavCommand::MenuClose));
    }

    #[test]
    fn toggle_open_menu_switches_or_closes() {
        assert_eq!(toggle_open_menu(Some(MenuId::File), MenuId::File), None);
        assert_eq!(
            toggle_open_menu(Some(MenuId::File), MenuId::History),
            Some(MenuId::History)
        );
        assert_eq!(toggle_open_menu(None, MenuId::Tabs), Some(MenuId::Tabs));
    }

    fn center_of(doc: &BaseDocument, element_id: &str) -> (f32, f32) {
        let id = doc.get_element_by_id(element_id).expect(element_id);
        let node = doc.get_node(id).expect("node");
        let pos = node.absolute_position(0.0, 0.0);
        let size = node.final_layout().size;
        (pos.x + size.width / 2.0, pos.y + size.height / 2.0)
    }

    fn open_file_menu_doc() -> BaseDocument {
        let view = ChromeViewState {
            open_menu: Some(MenuId::File),
            ..ChromeViewState::default()
        };
        let mut doc = parse_chrome(&chrome_document_from_state(&view));
        doc.resolve(0.0);
        doc
    }

    #[test]
    fn pointer_on_dropdown_or_trigger_keeps_menu_open() {
        let mut doc = open_file_menu_doc();
        let (tx, ty) = center_of(&doc, "menu-file");
        let (dx, dy) = center_of(&doc, "menu-file-dropdown");
        assert!(
            pointer_keeps_menu_open(&mut doc, tx, ty),
            "menu title trigger"
        );
        assert!(pointer_keeps_menu_open(&mut doc, dx, dy), "open dropdown");
    }

    #[test]
    fn pointer_outside_dropdown_dismisses_menu() {
        let mut doc = open_file_menu_doc();
        let (px, py) = center_of(&doc, "page");
        assert!(
            !pointer_keeps_menu_open(&mut doc, px, py),
            "page click should dismiss"
        );
        let mut doc = open_file_menu_doc();
        let (ux, uy) = center_of(&doc, "url-form");
        assert!(
            !pointer_keeps_menu_open(&mut doc, ux, uy),
            "address field should dismiss"
        );
    }

    #[test]
    fn menu_close_hides_dropdown() {
        let view = ChromeViewState {
            open_menu: Some(MenuId::File),
            ..ChromeViewState::default()
        };
        let mut doc = parse_chrome(&chrome_document_from_state(&view));
        let id = doc
            .get_element_by_id("menu-file-dropdown")
            .expect("dropdown");
        assert!(class_of(&doc, id).contains("open"));

        render_menu_bar(
            &mut doc,
            &ChromeViewState {
                open_menu: None,
                ..view
            },
        );
        let id = doc
            .get_element_by_id("menu-file-dropdown")
            .expect("dropdown");
        assert!(!class_of(&doc, id).contains("open"));
    }

    #[test]
    fn classifies_tab_commands() {
        let parse = |url| command_from_url(&Url::parse(url).unwrap());
        assert_eq!(parse("glide://new-tab"), Some(NavCommand::NewTab));
        assert_eq!(
            parse("glide://close-tab?id=3"),
            Some(NavCommand::CloseTab(3))
        );
        assert_eq!(
            parse("glide://select-tab?id=3"),
            Some(NavCommand::SelectTab(3))
        );
        assert_eq!(parse("glide://next-tab"), Some(NavCommand::NextTab));
        assert_eq!(parse("glide://prev-tab"), Some(NavCommand::PrevTab));
        assert_eq!(parse("glide://close-window"), Some(NavCommand::CloseWindow));
    }

    #[test]
    fn address_sync_skips_when_focused_writes_when_stale() {
        assert!(!should_write_committed_address(
            true,
            "https://a/",
            "https://b/"
        ));
        assert!(should_write_committed_address(
            false,
            "https://a/",
            "https://b/"
        ));
        assert!(!should_write_committed_address(
            false,
            "https://b/",
            "https://b/"
        ));
    }

    #[test]
    fn reads_the_address_form() {
        let url = Url::parse("https://glide.invalid/go?q=example.com").unwrap();
        assert_eq!(
            command_from_url(&url),
            Some(NavCommand::Go("example.com".into()))
        );
    }

    #[test]
    fn menu_rebuild_keeps_hit_testing_safe() {
        let open = ChromeViewState {
            url: "about:blank".into(),
            open_menu: Some(MenuId::File),
            ..ChromeViewState::default()
        };
        let mut doc = parse_chrome(&chrome_document_from_state(&open));
        doc.resolve(0.0);
        assert!(doc.hit(10.0, 10.0).is_some(), "initial chrome hit test");

        let closed = ChromeViewState {
            open_menu: None,
            ..open.clone()
        };
        render_menu_bar(&mut doc, &closed);
        // The shell hit-tests the next hovered window before any resolve. Without the
        // prune that `sync` runs after chrome mutations, this hit panics on the stale
        // hoisted dropdown entry ("invalid SlotMap key").
        let pruned = crate::js::prune_stale_paint_children(&mut doc);
        assert!(
            pruned > 0,
            "replacing an open menu bar should leave stale hoisted entries"
        );
        assert!(
            doc.hit(10.0, 10.0).is_some(),
            "hit test after a menu bar rebuild"
        );
        doc.resolve(0.0);
        assert!(doc.hit(10.0, 10.0).is_some(), "hit test after resolving");
    }

    #[test]
    fn toolbar_height_stable_across_page_zoom() {
        let mut doc = parse_chrome(&chrome_document("about:blank", true, false));
        doc.resolve(0.0);
        let zoom_at_100 = doc.viewport().zoom();
        let visual_h100 =
            toolbar_layout_height(&doc).expect("toolbar height at 100%") * zoom_at_100;
        {
            let mut vp = doc.viewport_mut();
            vp.set_zoom(2.0);
        }
        set_ui_scale(&mut doc, 0.5);
        doc.resolve(0.0);
        let zoom_at_200 = doc.viewport().zoom();
        let visual_h200 =
            toolbar_layout_height(&doc).expect("toolbar height at 200% zoom") * zoom_at_200;
        assert!(
            (visual_h100 - visual_h200).abs() < 2.0,
            "toolbar visual height should stay stable: {visual_h100} vs {visual_h200}"
        );
    }

    // Apple HIG: SF is the macOS system font; `system-ui` resolves to it in Blitz and
    // "Apple Symbols" covers ⇥, which SF lacks.
    #[test]
    fn chrome_uses_the_system_font_stack() {
        let html = chrome_document_from_state(&ChromeViewState::default());
        assert!(html.contains(
            "font-family: system-ui, -apple-system, \"Apple Symbols\", \"Segoe UI\", Ubuntu, Cantarell, sans-serif;"
        ));
    }

    // Apple HIG toolbars: one consistent family of line icons (SF Symbols style).
    #[test]
    fn navigation_buttons_use_svg_icons() {
        let doc = parse_chrome(&chrome_document_from_state(&ChromeViewState::default()));
        for id in ["back", "forward", "reload", "bookmark-star", "new-tab"] {
            assert_svg_icon_only(&doc, id);
        }
    }

    // Safari for macOS keeps reload at the trailing edge of the address field.
    #[test]
    fn reload_sits_inside_the_address_field() {
        let doc = parse_chrome(&chrome_document_from_state(&ChromeViewState::default()));
        let form = doc.get_element_by_id("url-form").expect("address form");
        let reload = doc.get_element_by_id("reload").expect("reload button");
        assert_eq!(doc.get_node(reload).and_then(|n| n.parent), Some(form));
        assert_eq!(element_children(&doc, form).last(), Some(&reload));
        let leading = doc.get_element_by_id("toolbar-leading").expect("leading");
        assert!(!element_children(&doc, leading).contains(&reload));
    }

    // Safari shows a thin ✕ line icon in place of reload while a page loads.
    #[test]
    fn stop_state_uses_an_svg_icon() {
        let mut doc = parse_chrome(&chrome_document_from_state(&ChromeViewState::default()));
        let reload = doc.get_element_by_id("reload").expect("reload button");
        set_loading_ui(&mut doc, true);
        assert_eq!(attr_of(&doc, reload, "href"), "glide://stop");
        assert_eq!(attr_of(&doc, reload, "title"), "Stop");
        assert_svg_icon_only(&doc, "reload");
        set_loading_ui(&mut doc, false);
        assert_eq!(attr_of(&doc, reload, "href"), "glide://reload");
        assert_eq!(attr_of(&doc, reload, "title"), "Reload");
        assert_svg_icon_only(&doc, "reload");
    }

    #[test]
    fn bookmark_star_uses_svg_icons() {
        let view = ChromeViewState {
            bookmarked: true,
            ..ChromeViewState::default()
        };
        let mut doc = parse_chrome(&chrome_document_from_state(&view));
        assert_svg_icon_only(&doc, "bookmark-star");
        assert!(star_icon_class(&doc).contains("icon-star-filled"));
        set_bookmark_star(&mut doc, false);
        assert_svg_icon_only(&doc, "bookmark-star");
        assert!(!star_icon_class(&doc).contains("icon-star-filled"));
        let star = doc.get_element_by_id("bookmark-star").expect("star");
        assert_eq!(attr_of(&doc, star, "title"), "Add bookmark");
        set_bookmark_star(&mut doc, true);
        assert!(star_icon_class(&doc).contains("icon-star-filled"));
        assert_eq!(attr_of(&doc, star, "title"), "Remove bookmark");
    }

    // Safari "separate" tab bar: equal-width flat segments, close at the leading edge,
    // a fixed-width new-tab button at the end.
    #[test]
    fn tabs_are_equal_width_segments() {
        let view = ChromeViewState {
            tabs: vec![
                TabView {
                    id: 1,
                    title: "A".into(),
                    ..TabView::default()
                },
                TabView {
                    id: 2,
                    title: "A much longer tab title that has to be truncated".into(),
                    loading: true,
                },
                TabView {
                    id: 3,
                    title: "Example Domain".into(),
                    ..TabView::default()
                },
            ],
            active_index: Some(0),
            ..ChromeViewState::default()
        };
        let html = chrome_document_from_state(&view);
        let rule = css_rule(&html, "div.tab");
        assert!(rule.contains("flex: 1 1 0;"), "{rule}");
        assert!(rule.contains("min-width: 0;"), "{rule}");
        assert!(!rule.contains("border-radius"), "{rule}");
        let strip = css_rule(&html, "#tab-strip");
        assert!(!strip.contains("gap"), "{strip}");

        let mut doc = parse_chrome(&html);
        doc.resolve(0.0);
        let widths: Vec<f32> = ["tab-1", "tab-2", "tab-3"]
            .iter()
            .map(|id| super::node_layout_size(&doc, id).expect("tab box").0)
            .collect();
        assert!(widths[0] > 100.0, "{widths:?}");
        // Layout snaps boxes to whole pixels, so equal shares may differ by one pixel.
        for width in &widths {
            assert!((width - widths[0]).abs() <= 1.0, "{widths:?}");
        }
        let (new_tab_w, _) = super::node_layout_size(&doc, "new-tab").expect("new tab box");
        assert!((new_tab_w - 28.0).abs() < 1.0, "new-tab width {new_tab_w}");

        let tab = doc.get_element_by_id("tab-2").expect("tab");
        let children = element_children(&doc, tab);
        let close = children
            .iter()
            .position(|id| class_of(&doc, *id) == "tab-close")
            .expect("close control");
        let title = children
            .iter()
            .position(|id| class_of(&doc, *id) == "tab-title")
            .expect("title");
        assert!(close < title, "close control leads the title");
    }

    // Safari find banner: chevron buttons and a Done button instead of boxed glyphs.
    #[test]
    fn find_bar_has_chevrons_and_done() {
        let html = chrome_document_from_state(&ChromeViewState::default());
        assert!(!html.contains("class=\"btn\""));
        let doc = parse_chrome(&html);
        let bar = doc.get_element_by_id("find-bar").expect("find bar");
        let links: Vec<NodeId> = descendants(&doc, bar)
            .into_iter()
            .filter(|id| tag_of(&doc, *id) == "a")
            .collect();
        let by_href = |href: &str| {
            *links
                .iter()
                .find(|id| attr_of(&doc, **id, "href") == href)
                .unwrap_or_else(|| panic!("{href} link"))
        };
        for href in ["glide://find-prev", "glide://find-next"] {
            let id = by_href(href);
            assert_eq!(class_of(&doc, id), "icon-btn");
            assert_eq!(
                element_children(&doc, id)
                    .first()
                    .map(|child| tag_of(&doc, *child)),
                Some("svg".to_string())
            );
        }
        let close = by_href("glide://find-close");
        assert_eq!(text_of(&doc, close).trim(), "Done");
        assert_eq!(attr_of(&doc, close, "title"), "Close find");
        assert!(descendants(&doc, bar).iter().any(|id| {
            class_of(&doc, *id)
                .split_whitespace()
                .any(|c| c == "search-icon")
        }));
    }

    // Safari light appearance: neutral greys and white from one set of tokens.
    #[test]
    fn chrome_palette_is_neutral() {
        let view = ChromeViewState {
            open_menu: Some(MenuId::View),
            ..ChromeViewState::default()
        };
        let html = chrome_document_from_state(&view).to_ascii_lowercase();
        for warm in [
            "#f4f1ea", "#e8e4dc", "#d4cfc5", "#f0ece4", "#f7f5f2", "#f5f3ef", "#d9d3c8", "#faf7f0",
            "#e4ddd0", "#fcfbfa", "#dcd6cb", "#1c1915", "#3d3933", "#d8d2c7", "#b1ab9f", "#8a8377",
        ] {
            assert!(!html.contains(warm), "warm colour {warm} remains");
        }
        for token in [
            "--chrome-bg: #f6f6f6;",
            "--tabbar-bg: #e6e6e6;",
            "--hairline: rgba(0,0,0,0.12);",
            "--field-fill: rgba(0,0,0,0.055);",
            "--text: #1d1d1f;",
            "--accent: #007aff;",
        ] {
            assert!(html.contains(token), "missing token {token}");
        }
    }

    // Apple HIG search fields: filled and borderless, with a focus ring while editing.
    #[test]
    fn address_field_is_borderless_with_a_focus_ring() {
        let html = chrome_document_from_state(&ChromeViewState::default());
        let rule = css_rule(&html, "form.address");
        assert!(rule.contains("border: 0;"), "{rule}");
        assert!(rule.contains("background: var(--field-fill);"), "{rule}");
        for selector in ["form.address:focus-within", "form.address.focused"] {
            let rule = css_rule(&html, selector);
            assert!(rule.contains("box-shadow: var(--focus-ring);"), "{rule}");
            assert!(rule.contains("background: #fff;"), "{rule}");
        }
        let doc = parse_chrome(&html);
        let form = doc.get_element_by_id("url-form").expect("address form");
        assert!(descendants(&doc, form).iter().any(|id| {
            class_of(&doc, *id)
                .split_whitespace()
                .any(|c| c == "search-icon")
        }));
    }

    #[test]
    fn devtools_console_input_fits_the_panel() {
        let html = chrome_document("about:blank", true, false);
        let rule = css_rule(&html, "#devtools-console-input");
        assert!(rule.contains("box-sizing: border-box;"), "{rule}");
        let mut doc = parse_chrome(&html);
        let state = crate::devtools::DevToolsState {
            open: true,
            size_px: 240,
            ..Default::default()
        };
        crate::chrome::apply_devtools_state(&mut doc, &state);
        doc.resolve(0.0);
        let pane = doc
            .get_element_by_id("devtools-pane-console")
            .expect("console pane");
        let input = doc
            .get_element_by_id("devtools-console-input")
            .expect("console input");
        let pane_w = doc.get_node(pane).expect("pane").final_layout().size.width;
        let layout = doc.get_node(input).expect("input").final_layout();
        assert!(layout.size.width > 100.0, "{layout:?}");
        assert!(
            layout.location.x + layout.size.width <= pane_w + 0.5,
            "input right edge {} overflows pane width {pane_w}",
            layout.location.x + layout.size.width
        );
    }

    fn assert_svg_icon_only(doc: &BaseDocument, id: &str) {
        let node = doc.get_element_by_id(id).unwrap_or_else(|| panic!("#{id}"));
        let children = element_children(doc, node);
        assert_eq!(children.len(), 1, "#{id} has one icon child");
        assert_eq!(tag_of(doc, children[0]), "svg", "#{id} icon is SVG");
        assert!(
            text_of(doc, node).trim().is_empty(),
            "#{id} has no text glyph: {:?}",
            text_of(doc, node)
        );
    }

    fn star_icon_class(doc: &BaseDocument) -> String {
        let star = doc.get_element_by_id("bookmark-star").expect("star");
        let icon = *element_children(doc, star).first().expect("star icon");
        class_of(doc, icon)
    }

    fn css_rule<'a>(html: &'a str, selector: &str) -> &'a str {
        let start = html
            .find(&format!("\n  {selector} {{"))
            .unwrap_or_else(|| panic!("rule {selector}"));
        let len = html[start..].find("\n  }").expect("rule end");
        &html[start..start + len]
    }

    fn element_children(doc: &BaseDocument, id: NodeId) -> Vec<NodeId> {
        doc.get_node(id)
            .map(|node| {
                node.children
                    .iter()
                    .copied()
                    .filter(|child| doc.get_node(*child).is_some_and(|n| n.is_element()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn descendants(doc: &BaseDocument, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = element_children(doc, id);
        while let Some(next) = stack.pop() {
            out.push(next);
            stack.extend(element_children(doc, next));
        }
        out
    }

    fn tag_of(doc: &BaseDocument, id: NodeId) -> String {
        doc.get_node(id)
            .and_then(|node| node.element_data())
            .map(|el| el.name.local.to_string())
            .unwrap_or_default()
    }

    fn text_of(doc: &BaseDocument, id: NodeId) -> String {
        doc.get_node(id)
            .map(|node| node.text_content())
            .unwrap_or_default()
    }

    fn attr_of(doc: &BaseDocument, id: NodeId, name: &str) -> String {
        doc.get_node(id)
            .and_then(|node| node.element_data())
            .and_then(|el| {
                el.attrs()
                    .iter()
                    .find(|attr| &*attr.name.local == name)
                    .map(|attr| attr.value.to_string())
            })
            .unwrap_or_default()
    }

    fn parse_chrome(html: &str) -> BaseDocument {
        let config = DocumentConfig {
            viewport: Some(Viewport::new(1100, 760, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(GlideHtmlProvider::default())),
            ..DocumentConfig::default()
        };
        parse::document_from_html(html, config, false)
    }

    fn class_of(doc: &BaseDocument, id: NodeId) -> String {
        doc.get_node(id)
            .and_then(|node| node.attr(local_name!("class")))
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn devtools_markup_wraps_page_iframe() {
        let html = chrome_document("about:blank", true, false);
        assert!(html.contains("id=\"stage\""));
        assert!(html.contains("id=\"devtools-split\""));
        assert!(html.contains("id=\"devtools\""));
        assert!(html.contains("id=\"inspect-hl\""));
        assert!(html.contains("id=\"page\""));
    }

    #[test]
    fn devtools_console_input_exists_when_console_tab_active() {
        let html = chrome_document("about:blank", true, false);
        let mut doc = parse::document_from_html(&html, DocumentConfig::default(), false);
        doc.resolve(0.0);
        assert!(doc.get_element_by_id("devtools-console-input").is_some());
    }

    #[test]
    fn classifies_devtools_commands() {
        use crate::devtools::DevToolsTab;

        let parse = |url| command_from_url(&Url::parse(url).unwrap());
        assert_eq!(parse("glide://devtools"), Some(NavCommand::DevToolsToggle));
        assert_eq!(
            parse("glide://devtools-dock-bottom"),
            Some(NavCommand::DevToolsDockBottom)
        );
        assert_eq!(
            parse("glide://devtools-dock-right"),
            Some(NavCommand::DevToolsDockRight)
        );
        assert_eq!(
            parse("glide://devtools-tab?tab=network"),
            Some(NavCommand::DevToolsTab(DevToolsTab::Network))
        );
        assert_eq!(
            parse("glide://devtools-console-clear"),
            Some(NavCommand::DevToolsConsoleClear)
        );
        assert_eq!(
            parse("glide://devtools-elements-select?node=42"),
            Some(NavCommand::DevToolsElementsSelect(42))
        );
        assert_eq!(
            parse("glide://devtools-elements-toggle?node=7"),
            Some(NavCommand::DevToolsElementsToggle(7))
        );
        assert_eq!(parse("glide://devtools-elements-toggle?node=bad"), None);
        assert_eq!(
            parse("glide://devtools-inspect-toggle"),
            Some(NavCommand::DevToolsInspectToggle)
        );
    }

    #[test]
    fn closed_devtools_leaves_stage_without_open_class() {
        let mut doc = parse_chrome(&chrome_document("about:blank", true, false));
        let open = crate::devtools::DevToolsState {
            open: true,
            size_px: 240,
            ..Default::default()
        };
        crate::chrome::apply_devtools_state(&mut doc, &open);
        doc.resolve(0.0);
        let closed = crate::devtools::DevToolsState::default();
        crate::chrome::apply_devtools_state(&mut doc, &closed);
        doc.resolve(0.0);
        let stage = doc.get_element_by_id("stage").expect("stage");
        let class = class_of(&doc, stage);
        assert!(!class.contains("devtools-open"));
        let (stage_w, stage_h) = crate::chrome::node_layout_size(&doc, "stage").expect("stage box");
        let (iframe_w, iframe_h) =
            crate::chrome::node_layout_size(&doc, "page").expect("iframe box");
        assert!((stage_w - iframe_w).abs() < 2.0);
        assert!((stage_h - iframe_h).abs() < 2.0);
        let split = doc.get_element_by_id("devtools-split").expect("split");
        assert!(
            doc.get_node(split)
                .is_some_and(|n| n.final_layout().size.height < 1.0),
            "splitter is hidden when closed"
        );
    }

    #[test]
    fn open_bottom_dock_sets_height_with_ui_var() {
        let mut doc = parse_chrome(&chrome_document("about:blank", true, false));
        doc.resolve(0.0);
        let state = crate::devtools::DevToolsState {
            open: true,
            dock: crate::devtools::DevToolsDock::Bottom,
            size_px: 240,
            ..Default::default()
        };
        crate::chrome::apply_devtools_state(&mut doc, &state);
        let devtools = doc.get_element_by_id("devtools").expect("devtools");
        let style = doc
            .get_node(devtools)
            .and_then(|n| n.attr(local_name!("style")))
            .unwrap_or_default();
        assert!(style.contains("height: calc(240px * var(--ui))"));
    }

    #[test]
    fn open_right_dock_sets_width_with_ui_var() {
        let mut doc = parse_chrome(&chrome_document("about:blank", true, false));
        doc.resolve(0.0);
        let state = crate::devtools::DevToolsState {
            open: true,
            dock: crate::devtools::DevToolsDock::Right,
            size_px: 200,
            ..Default::default()
        };
        crate::chrome::apply_devtools_state(&mut doc, &state);
        let stage = doc.get_element_by_id("stage").expect("stage");
        assert!(class_of(&doc, stage).contains("dock-right"));
        let devtools = doc.get_element_by_id("devtools").expect("devtools");
        let style = doc
            .get_node(devtools)
            .and_then(|n| n.attr(local_name!("style")))
            .unwrap_or_default();
        assert!(style.contains("width: calc(200px * var(--ui))"));
    }

    #[test]
    fn devtools_elements_tree_focused_includes_descendant() {
        let html = chrome_document("about:blank", true, false);
        let mut doc = parse_chrome(&html);
        doc.resolve(0.0);
        let state = crate::devtools::DevToolsState {
            open: true,
            tab: crate::devtools::DevToolsTab::Elements,
            ..Default::default()
        };
        super::apply_devtools_state(&mut doc, &state);
        let tree = doc
            .get_element_by_id("devtools-elements-tree")
            .expect("tree");
        crate::parse::parse_inner_html_into_mutator(
            &mut doc.mutate(),
            tree,
            "<a class=\"devtools-elements-row-link\" href=\"#\"><span class=\"label\">x</span></a>",
            false,
        );
        doc.resolve(0.0);
        let link = doc
            .get_node(tree)
            .and_then(|n| n.children.first().copied())
            .expect("link");
        doc.set_focus_to(link);
        assert!(super::devtools_elements_tree_focused(&doc));
    }

    #[test]
    fn devtools_elements_computed_pane_markup() {
        let html = chrome_document("about:blank", true, false);
        assert!(html.contains("id=\"devtools-elements-computed\""));
    }

    #[test]
    fn devtools_elements_pane_markup() {
        let html = chrome_document("about:blank", true, false);
        assert!(html.contains("id=\"devtools-elements-tree\""));
        assert!(html.contains("id=\"devtools-elements-detail\""));
        assert!(!html.contains("DOM tree will appear here"));
    }

    #[test]
    fn devtools_tab_switch_updates_active_pane() {
        let mut doc = parse_chrome(&chrome_document("about:blank", true, false));
        doc.resolve(0.0);
        let state = crate::devtools::DevToolsState {
            open: true,
            tab: crate::devtools::DevToolsTab::Elements,
            ..Default::default()
        };
        crate::chrome::apply_devtools_state(&mut doc, &state);
        let tab = doc.get_element_by_id("devtools-tab-elements").expect("tab");
        assert!(class_of(&doc, tab).contains("active"));
        let pane = doc
            .get_element_by_id("devtools-pane-elements")
            .expect("pane");
        assert!(class_of(&doc, pane).contains("active"));
    }

    #[test]
    fn view_menu_lists_toggle_developer_tools() {
        let html = chrome_document_from_state(&ChromeViewState::default());
        assert!(html.contains("Toggle Developer Tools"));
        assert!(html.contains("glide://devtools"));
    }
}
