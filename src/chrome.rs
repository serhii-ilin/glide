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

/// Render the chrome from the window's current view of tabs, menus and page.
/// Reload glyph. The system fonts have no circular-arrow character, so the
/// icon is inline SVG.
const RELOAD_ICON: &str = "<svg width=\"16\" height=\"16\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path fill=\"#3d3933\" d=\"M17.65 6.35A7.96 7.96 0 0 0 12 4c-4.42 0-7.99 3.58-8 8s3.58 8 8 8c3.73 0 6.84-2.55 7.73-6h-2.08A5.99 5.99 0 0 1 12 18c-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z\"/></svg>";

pub fn chrome_document_from_state(view: &ChromeViewState) -> String {
    let url = escape_attr(&view.url);
    let address_title = if view.title.trim().is_empty() {
        url.clone()
    } else {
        escape_attr(&view.title)
    };
    let js_label = if view.js_enabled { "JS on" } else { "JS off" };
    let star = if view.bookmarked {
        "&#9733;"
    } else {
        "&#9734;"
    };
    let star_title = if view.bookmarked {
        "Remove bookmark"
    } else {
        "Add bookmark"
    };
    let tab_strip = format!(
        "<div id=\"tab-strip\">\n{}</div>\n",
        tab_strip_inner_html(view)
    );
    let menu_bar = format!(
        "<nav id=\"menu-bar\">\n{}</nav>\n",
        menu_bar_inner_html(view)
    );
    let reload_icon = RELOAD_ICON;
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Glide</title>
<style>
  html {{
    --ui: 1;
  }}
  html, body {{
    margin: 0;
    height: 100%;
    background: #f4f1ea;
    color: #1c1915;
    font-family: "Segoe UI", Ubuntu, Cantarell, sans-serif;
  }}
  body {{
    display: flex;
    flex-direction: column;
  }}
  #menu-bar {{
    display: flex;
    align-items: center;
    gap: calc(1px * var(--ui));
    padding: calc(4px * var(--ui)) calc(6px * var(--ui));
    background: #f5f3ef;
    border-bottom: calc(1px * var(--ui)) solid #d9d3c8;
    flex: none;
  }}
  div.menu {{
    position: relative;
  }}
  a.menu-trigger {{
    display: block;
    padding: calc(4px * var(--ui)) calc(11px * var(--ui));
    border-radius: calc(6px * var(--ui));
    color: #23201c;
    text-decoration: none;
    font-size: calc(13px * var(--ui));
    line-height: calc(16px * var(--ui));
    letter-spacing: calc(0.01em * var(--ui));
  }}
  a.menu-trigger:hover {{
    background: #e8e4dc;
  }}
  a.menu-trigger.active {{
    background: #3478f6;
    color: #fff;
  }}
  div.menu-dropdown {{
    display: none;
    position: absolute;
    top: calc(100% + calc(3px * var(--ui)));
    left: 0;
    z-index: 10;
    min-width: calc(230px * var(--ui));
    padding: calc(6px * var(--ui)) 0;
    background: #fcfbfa;
    border: calc(1px * var(--ui)) solid #d8d2c7;
    border-radius: calc(9px * var(--ui));
    box-shadow:
      0 calc(1px * var(--ui)) calc(1px * var(--ui)) rgba(0,0,0,0.06),
      0 calc(8px * var(--ui)) calc(24px * var(--ui)) rgba(0,0,0,0.18);
  }}
  div.menu-dropdown.open {{
    display: block;
  }}
  a.menu-item, span.menu-item {{
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: calc(20px * var(--ui));
    padding: calc(5px * var(--ui)) calc(12px * var(--ui));
    color: #23201c;
    text-decoration: none;
    font-size: calc(13px * var(--ui));
    line-height: calc(18px * var(--ui));
    white-space: nowrap;
  }}
  a.menu-item:hover {{
    background: #3478f6;
    color: #fff;
  }}
  a.menu-item:hover span.menu-shortcut {{
    color: rgba(255,255,255,0.85);
  }}
  span.menu-item.disabled {{
    color: #b1ab9f;
  }}
  span.menu-item.disabled span.menu-shortcut {{
    color: #cdc7bb;
  }}
  span.menu-label {{
    flex: 1;
  }}
  span.menu-shortcut {{
    color: #8a8377;
    font-size: calc(12px * var(--ui));
  }}
  span.menu-heading {{
    display: block;
    padding: calc(8px * var(--ui)) calc(12px * var(--ui)) calc(3px * var(--ui));
    color: #9a9285;
    font-size: calc(11px * var(--ui));
    letter-spacing: calc(0.03em * var(--ui));
  }}
  span.menu-separator {{
    display: block;
    height: calc(1px * var(--ui));
    margin: calc(6px * var(--ui)) calc(8px * var(--ui));
    background: #e3ddd2;
  }}
  #tab-strip {{
    display: flex;
    align-items: flex-end;
    gap: calc(4px * var(--ui));
    padding: calc(6px * var(--ui)) calc(8px * var(--ui)) 0;
    background: #e8e4dc;
    flex: none;
  }}
  div.tab {{
    display: flex;
    align-items: center;
    gap: calc(7px * var(--ui));
    max-width: calc(230px * var(--ui));
    min-width: calc(96px * var(--ui));
    height: calc(28px * var(--ui));
    padding: 0 calc(9px * var(--ui));
    border-radius: calc(8px * var(--ui)) calc(8px * var(--ui)) 0 0;
    background: #d4cfc5;
    font-size: calc(12px * var(--ui));
    color: #4d4841;
    overflow: hidden;
    box-shadow: inset 0 calc(-1px * var(--ui)) 0 rgba(0,0,0,0.06);
  }}
  div.tab:hover {{
    background: #dcd7cd;
  }}
  div.tab.active {{
    background: #f7f5f2;
    color: #1c1915;
    margin-bottom: calc(-1px * var(--ui));
    box-shadow: inset 0 calc(1px * var(--ui)) 0 rgba(255,255,255,0.9);
  }}
  span.tab-favicon {{
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: calc(14px * var(--ui));
    height: calc(14px * var(--ui));
    border-radius: calc(4px * var(--ui));
    background: #bdb7ac;
    color: transparent;
    font-size: calc(9px * var(--ui));
  }}
  span.tab-title {{
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }}
  div.tab.loading span.tab-favicon {{
    background: #2f6fe0;
  }}
  a.tab-close {{
    color: #6b665c;
    text-decoration: none;
    font-size: calc(15px * var(--ui));
    line-height: 1;
    border-radius: calc(4px * var(--ui));
    padding: 0 calc(3px * var(--ui));
    opacity: 0;
    transition: opacity 80ms linear;
  }}
  div.tab:hover a.tab-close, div.tab.active a.tab-close {{
    opacity: 1;
  }}
  a.tab-close:hover {{
    background: #c8c3b9;
    color: #1c1915;
  }}
  div.tab.active a.tab-close:hover {{
    background: #d8d4cb;
  }}
  a#new-tab {{
    min-width: calc(26px * var(--ui));
    height: calc(26px * var(--ui));
    margin-bottom: calc(2px * var(--ui));
    border-radius: calc(7px * var(--ui));
    background: transparent;
    border-color: transparent;
    color: #5a564d;
  }}
  a#new-tab:hover {{
    background: #ded9cf;
    color: #1c1915;
  }}
  nav#toolbar {{
    display: flex;
    align-items: center;
    gap: calc(10px * var(--ui));
    padding: calc(8px * var(--ui)) calc(12px * var(--ui));
    background: #f7f5f2;
    border-bottom: calc(1px * var(--ui)) solid #e0dbd1;
    flex: none;
  }}
  .toolbar-side {{
    display: flex;
    align-items: center;
    gap: calc(2px * var(--ui));
    flex: 1 1 0;
    min-width: 0;
  }}
  #toolbar-trailing {{
    justify-content: flex-end;
  }}
  a.icon-btn {{
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: calc(30px * var(--ui));
    height: calc(30px * var(--ui));
    padding: 0 calc(6px * var(--ui));
    border: 0;
    border-radius: calc(8px * var(--ui));
    background: transparent;
    color: #3d3933;
    text-decoration: none;
    font-size: calc(19px * var(--ui));
    line-height: 1;
  }}
  a.icon-btn:hover {{
    background: rgba(0,0,0,0.07);
  }}
  a.icon-btn:active {{
    background: rgba(0,0,0,0.12);
  }}
  a.chip {{
    display: flex;
    align-items: center;
    height: calc(24px * var(--ui));
    padding: 0 calc(9px * var(--ui));
    border: calc(1px * var(--ui)) solid #d8d2c6;
    border-radius: calc(12px * var(--ui));
    background: #fffdfa;
    color: #55504a;
    text-decoration: none;
    font-size: calc(11px * var(--ui));
    letter-spacing: calc(0.02em * var(--ui));
  }}
  a.chip:hover {{
    background: #f0ece4;
  }}
  form.address {{
    position: relative;
    display: flex;
    align-items: center;
    flex: 0 1 auto;
    width: calc(560px * var(--ui));
    max-width: 55%;
    height: calc(32px * var(--ui));
    margin: 0;
    padding: 0;
    background: #fff;
    border: calc(1px * var(--ui)) solid #dcd6cb;
    border-radius: calc(10px * var(--ui));
    box-shadow: 0 calc(1px * var(--ui)) calc(2px * var(--ui)) rgba(0,0,0,0.05);
  }}
  form.address:focus-within {{
    border-color: #6f9df0;
    box-shadow: 0 0 0 calc(3px * var(--ui)) rgba(52,120,246,0.16);
  }}
  form.address input {{
    flex: 1;
    width: 100%;
    height: 100%;
    padding: 0 calc(34px * var(--ui));
    border: 0;
    border-radius: inherit;
    background: transparent;
    color: transparent;
    font-size: calc(13px * var(--ui));
    text-align: center;
  }}
  form.address input:focus {{
    outline: none;
    color: #1c1915;
    text-align: left;
  }}
  span.address-title {{
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 calc(34px * var(--ui));
    color: #35312b;
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
  form.address.focused input {{
    color: #1c1915;
    text-align: left;
  }}
  span.address-icon {{
    position: absolute;
    left: calc(12px * var(--ui));
    width: calc(7px * var(--ui));
    height: calc(7px * var(--ui));
    border-radius: 50%;
    background: #b8b2a7;
    pointer-events: none;
  }}
  a.btn {{
    min-width: calc(32px * var(--ui));
    height: calc(32px * var(--ui));
    padding: 0 calc(8px * var(--ui));
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: calc(8px * var(--ui));
    background: #fff;
    border: calc(1px * var(--ui)) solid #e4ddd0;
    color: #1c1915;
    text-decoration: none;
    font-size: calc(16px * var(--ui));
  }}
  #zoom-label {{
    min-width: calc(44px * var(--ui));
    text-align: center;
    font-size: calc(12px * var(--ui));
    color: #6b6358;
    display: none;
  }}
  #zoom-label.visible {{
    display: block;
  }}
  form {{
    flex: 1;
    display: flex;
    margin: 0;
  }}
  input {{
    width: 100%;
    height: calc(32px * var(--ui));
    border: calc(1px * var(--ui)) solid #e4ddd0;
    border-radius: 999px;
    padding: 0 calc(14px * var(--ui));
    background: #fff;
    color: #1c1915;
    font-size: calc(14px * var(--ui));
  }}
  #find-bar {{
    display: none;
    align-items: center;
    gap: calc(6px * var(--ui));
    padding: calc(6px * var(--ui)) calc(10px * var(--ui));
    background: #faf7f0;
    border-bottom: calc(1px * var(--ui)) solid #e4ddd0;
    flex: none;
  }}
  #find-bar.open {{
    display: flex;
  }}
  #find-input {{
    flex: 1;
    width: auto;
  }}
  #find-count {{
    font-size: calc(12px * var(--ui));
    color: #6b6358;
    min-width: calc(72px * var(--ui));
    text-align: center;
  }}
  iframe {{
    flex: 1;
    width: 100%;
    border: 0;
    background: #fff;
    min-height: 0;
  }}
  #loading {{
    height: calc(2px * var(--ui));
    background: #3b82f6;
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
  {tab_strip}
  <nav id="toolbar">
    <div id="toolbar-leading" class="toolbar-side">
      <a id="back" class="icon-btn" href="glide://back" title="Back" aria-label="Back">&#8592;</a>
      <a id="forward" class="icon-btn" href="glide://forward" title="Forward" aria-label="Forward">&#8594;</a>
      <a id="reload" class="icon-btn" href="glide://reload" title="Reload" aria-label="Reload">{reload_icon}</a>
    </div>
    <form id="url-form" class="address" action="https://glide.invalid/go" method="get">
      <span class="address-icon" aria-hidden="true"></span>
      <span id="url-title" class="address-title">{address_title}</span>
      <input id="url" name="q" type="text" value="{url}" spellcheck="false" autocomplete="off" aria-label="Address">
    </form>
    <div id="toolbar-trailing" class="toolbar-side">
      <a id="js-toggle" class="chip" href="glide://toggle-js" title="Toggle JavaScript">{js_label}</a>
      <a id="bookmark-star" class="icon-btn" href="glide://bookmark" title="{star_title}" aria-label="{star_title}">{star}</a>
      <span id="zoom-label"></span>
    </div>
  </nav>
  <div id="find-bar">
    <input id="find-input" type="text" spellcheck="false" autocomplete="off" placeholder="Find in page">
    <span id="find-count"></span>
    <a class="btn" href="glide://find-prev" title="Previous match">&#8593;</a>
    <a class="btn" href="glide://find-next" title="Next match">&#8595;</a>
    <a class="btn" href="glide://find-close" title="Close find">&times;</a>
  </div>
  <div id="loading"></div>
  <iframe id="page" src="{url}"></iframe>
</body>
</html>
"#
    )
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
            r#"<div id="tab-{id}" class="{classes}"><span class="tab-favicon"></span><span class="tab-title">{title}</span><a class="tab-close" href="glide://close-tab?id={id}" title="Close tab" aria-label="Close tab">&times;</a></div>
"#,
            id = tab.id,
        ));
    }
    html.push_str(
        "<a id=\"new-tab\" class=\"icon-btn\" href=\"glide://new-tab\" title=\"New tab\" aria-label=\"New tab\">+</a>\n",
    );
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
        let mut m = doc.mutate();
        if loading {
            m.set_attribute(id, qual_name!("href"), "glide://stop");
            m.set_attribute(id, qual_name!("title"), "Stop");
            let children: Vec<_> = m.child_ids(id).into_iter().collect();
            for child in children {
                m.remove_and_drop_node(child);
            }
            let text_id = m.create_text_node("■");
            m.append_children(id, &[text_id]);
        } else {
            m.set_attribute(id, qual_name!("href"), "glide://reload");
            m.set_attribute(id, qual_name!("title"), "Reload");
            let children: Vec<_> = m.child_ids(id).into_iter().collect();
            for child in children {
                m.remove_and_drop_node(child);
            }
            crate::parse::parse_inner_html_into_mutator(&mut m, id, RELOAD_ICON, false);
        }
    }
}

pub fn find_bar_open(doc: &BaseDocument) -> bool {
    doc.get_element_by_id("find-bar").is_some_and(|id| {
        doc.get_node(id)
            .and_then(|n| n.attr(local_name!("class")))
            .is_some_and(|c| c.split_whitespace().any(|part| part == "open"))
    })
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
    let star = if bookmarked { "★" } else { "☆" };
    let title = if bookmarked {
        "Remove bookmark"
    } else {
        "Add bookmark"
    };
    let mut m = doc.mutate();
    m.set_attribute(id, qual_name!("title"), title);
    let children: Vec<_> = m.child_ids(id).into_iter().collect();
    for child in children {
        m.remove_and_drop_node(child);
    }
    let text_id = m.create_text_node(star);
    m.append_children(id, &[text_id]);
}

pub fn set_ui_scale(doc: &mut BaseDocument, ui: f32) {
    let root = doc.root_element().id;
    let style = format!("--ui: {ui}");
    doc.mutate()
        .set_attribute(root, qual_name!("style"), &style);
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
        command_closes_menu, command_from_url, render_menu_bar, set_address_focused, set_ui_scale,
        should_write_committed_address, toolbar_layout_height,
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
}
