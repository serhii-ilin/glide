//! Per-tab navigation state, tab selection, and the on-disk tab session format.

use std::collections::HashSet;

use keyboard_types::Modifiers;
use serde::{Deserialize, Serialize};

use crate::devtools::console::ConsoleJournal;
use crate::devtools::elements::ElementsPanelState;
use crate::devtools::network::{NetworkJournal, NetworkPanelState};
use crate::devtools::repl::ConsoleReplHistory;
use crate::find::FindSession;
use crate::history::History;
use crate::nav::NavTracker;
use crate::zoom::PageZoom;

/// Stable identity for a tab within one window.
pub type TabId = u64;

/// Everything that belongs to one browsing context.
#[derive(Debug, Clone)]
pub struct Tab {
    pub id: TabId,
    pub nav: NavTracker,
    pub title: String,
    pub loading: bool,
    pub find: FindSession,
    pub zoom: PageZoom,
    pub console: ConsoleJournal,
    pub console_repl_history: ConsoleReplHistory,
    pub network: NetworkJournal,
    pub network_panel: NetworkPanelState,
    pub elements: ElementsPanelState,
    /// Bumped on each navigation so stale console wire events are ignored.
    pub console_epoch: u64,
    pub network_epoch: u64,
    /// Bumped on navigation so Elements tree state resets for a new document.
    pub elements_epoch: u64,
    /// Last URL written into the address field, so a focused field is left alone.
    pub address_written: String,
}

impl Tab {
    pub fn new(id: TabId, url: String) -> Self {
        Self {
            id,
            nav: NavTracker::new(url.clone()),
            title: String::new(),
            loading: false,
            find: FindSession::default(),
            zoom: PageZoom::new(),
            console: ConsoleJournal::default(),
            console_repl_history: ConsoleReplHistory::default(),
            network: NetworkJournal::default(),
            network_panel: NetworkPanelState::default(),
            elements: ElementsPanelState::default(),
            console_epoch: 1,
            network_epoch: 1,
            elements_epoch: 1,
            address_written: url,
        }
    }

    pub fn url(&self) -> &str {
        let history = self.nav.history();
        &history.entries()[history.index()]
    }
}

/// How a link activation should open its target, based on modifier keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkTarget {
    CurrentTab,
    NewBackgroundTab,
    NewForegroundTab,
}

/// HTML link activation with the platform's primary modifier (Command on macOS,
/// Control elsewhere) and Shift. Modifier flags are classified here so the
/// command can be wired independently of the event source.
pub fn link_target_from_modifiers(modifiers: Modifiers) -> LinkTarget {
    let primary = modifiers.contains(Modifiers::META) || modifiers.contains(Modifiers::CONTROL);
    if !primary {
        LinkTarget::CurrentTab
    } else if modifiers.contains(Modifiers::SHIFT) {
        LinkTarget::NewForegroundTab
    } else {
        LinkTarget::NewBackgroundTab
    }
}

/// Ordered tabs for one window and the selected position.
#[derive(Debug, Clone, Default)]
pub struct TabManager {
    tabs: Vec<Tab>,
    active: Option<usize>,
    next_id: TabId,
}

impl TabManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_tabs(tabs: Vec<Tab>, active: Option<usize>) -> Self {
        let next_id = tabs.iter().map(|tab| tab.id).max().unwrap_or(0) + 1;
        let active = active.filter(|index| *index < tabs.len());
        Self {
            tabs,
            active,
            next_id,
        }
    }

    /// Add a tab at the end and make it active.
    pub fn add(&mut self, url: impl Into<String>) -> TabId {
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(Tab::new(id, url.into()));
        self.active = Some(self.tabs.len() - 1);
        id
    }

    /// Open a new tab on a blank page.
    pub fn new_tab(&mut self) -> TabId {
        self.add("about:blank")
    }

    /// Make sure the window shows at least one tab.
    ///
    /// A restored session can be empty; browsers still show a tab strip with one
    /// tab, so the window never opens with an empty tab row.
    pub fn ensure_one_tab(&mut self, url: impl Into<String>) {
        if self.tabs.is_empty() {
            self.add(url);
        }
    }

    /// Append an existing tab snapshot with a fresh identity (used by Duplicate Tab).
    pub fn insert(&mut self, mut tab: Tab) -> TabId {
        let id = self.next_id;
        self.next_id += 1;
        tab.id = id;
        tab.console = ConsoleJournal::default();
        tab.console_repl_history = ConsoleReplHistory::default();
        tab.console_epoch = 1;
        tab.elements = ElementsPanelState::default();
        tab.elements_epoch = 1;
        self.tabs.push(tab);
        self.active = Some(self.tabs.len() - 1);
        id
    }

    pub fn close(&mut self, id: TabId) -> Option<Tab> {
        let index = self.tabs.iter().position(|tab| tab.id == id)?;
        self.remove_at(index)
    }

    pub fn close_active(&mut self) -> Option<Tab> {
        let index = self.active?;
        self.remove_at(index)
    }

    fn remove_at(&mut self, index: usize) -> Option<Tab> {
        if index >= self.tabs.len() {
            return None;
        }
        let tab = self.tabs.remove(index);
        self.active = match self.active {
            Some(active) if active == index => {
                if self.tabs.is_empty() {
                    None
                } else {
                    Some(index.min(self.tabs.len() - 1))
                }
            }
            Some(active) if active > index => Some(active - 1),
            other => other,
        };
        Some(tab)
    }

    pub fn select(&mut self, id: TabId) -> bool {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return false;
        };
        self.select_index(index)
    }

    pub fn select_index(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        if self.active == Some(index) {
            return false;
        }
        self.active = Some(index);
        true
    }

    pub fn select_next(&mut self) -> bool {
        let Some(active) = self.active else {
            return false;
        };
        self.select_index((active + 1) % self.tabs.len())
    }

    pub fn select_previous(&mut self) -> bool {
        let Some(active) = self.active else {
            return false;
        };
        let previous = active.checked_sub(1).unwrap_or(self.tabs.len() - 1);
        self.select_index(previous)
    }

    pub fn active(&self) -> Option<&Tab> {
        self.active.and_then(|index| self.tabs.get(index))
    }

    pub fn active_mut(&mut self) -> Option<&mut Tab> {
        let index = self.active?;
        self.tabs.get_mut(index)
    }

    pub fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    pub fn active_id(&self) -> Option<TabId> {
        self.active().map(|tab| tab.id)
    }

    pub fn by_index(&self, index: usize) -> Option<&Tab> {
        self.tabs.get(index)
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// Restore tabs from a session, or start at the CLI URL when one is given.
    pub fn from_session(session: &Session, cli_url: Option<&str>) -> Self {
        if let Some(url) = cli_url {
            let mut tabs = Self::new();
            tabs.add(url);
            return tabs;
        }
        let tabs = session
            .tabs
            .iter()
            .map(Tab::from_session)
            .collect::<Vec<_>>();
        if tabs.is_empty() {
            return Self::new();
        }
        Self::from_tabs(tabs, Some(session.active_index))
    }
}

impl Tab {
    fn from_session(saved: &TabSession) -> Self {
        let entries = if saved.entries.is_empty() {
            vec![saved.url.clone()]
        } else {
            saved.entries.clone()
        };
        let mut tab = Self::new(0, saved.url.clone());
        tab.nav =
            NavTracker::from_history(History::from_parts(entries, saved.index), saved.url.clone());
        tab.title = saved.title.clone();
        tab.zoom.set_percent(saved.zoom_percent);
        tab.find.set_query(&saved.find_query);
        tab.find.open = saved.find_open;
        tab.address_written = saved.url.clone();
        tab
    }
}

/// Serializable state of one tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabSession {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub entries: Vec<String>,
    #[serde(default)]
    pub index: usize,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub find_query: String,
    #[serde(default)]
    pub find_open: bool,
    #[serde(default = "default_zoom_percent")]
    pub zoom_percent: i32,
}

fn default_zoom_percent() -> i32 {
    100
}

/// Version 2 session: every tab and the selected tab position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    #[serde(default = "session_version")]
    pub version: u32,
    #[serde(default)]
    pub tabs: Vec<TabSession>,
    #[serde(default)]
    pub active_index: usize,
}

fn session_version() -> u32 {
    2
}

impl Default for Session {
    fn default() -> Self {
        Self {
            version: session_version(),
            tabs: Vec::new(),
            active_index: 0,
        }
    }
}

impl Session {
    /// Snapshot tabs for disk, dropping internal URLs and failed loads.
    pub fn from_tabs(tabs: &TabManager, failed_urls: &HashSet<String>) -> Self {
        let saved: Vec<TabSession> = tabs
            .tabs()
            .iter()
            .map(|tab| TabSession::from_tab(tab, failed_urls))
            .collect();
        let active_index = tabs
            .active_index()
            .unwrap_or(0)
            .min(saved.len().saturating_sub(1));
        Self {
            version: session_version(),
            tabs: saved,
            active_index,
        }
    }

    /// Convert a version 1 `{ entries, index }` session into one tab.
    pub fn from_legacy(entries: Vec<String>, index: usize) -> Self {
        if entries.is_empty() {
            return Self::default();
        }
        let index = index.min(entries.len() - 1);
        let url = entries[index].clone();
        Self {
            version: session_version(),
            tabs: vec![TabSession {
                url,
                entries,
                index,
                title: String::new(),
                find_query: String::new(),
                find_open: false,
                zoom_percent: default_zoom_percent(),
            }],
            active_index: 0,
        }
    }

    pub fn validate(&mut self) {
        if self.tabs.is_empty() {
            self.active_index = 0;
            return;
        }
        self.active_index = self.active_index.min(self.tabs.len() - 1);
        for tab in &mut self.tabs {
            tab.index = if tab.entries.is_empty() {
                0
            } else {
                tab.index.min(tab.entries.len() - 1)
            };
            if tab.url.is_empty() {
                tab.url = tab
                    .entries
                    .get(tab.index)
                    .cloned()
                    .unwrap_or_else(|| "about:blank".to_string());
            }
        }
    }
}

impl TabSession {
    fn from_tab(tab: &Tab, failed_urls: &HashSet<String>) -> Self {
        let entries = tab.nav.history().entries();
        let index = tab.nav.history().index();
        let keep =
            |url: &String| crate::profile::should_persist_url(url) && !failed_urls.contains(url);
        let kept: Vec<String> = entries.iter().filter(|url| keep(url)).cloned().collect();
        let current = entries.get(index);
        let mapped_index = current
            .filter(|url| keep(url))
            .and_then(|url| kept.iter().position(|candidate| candidate == url))
            .unwrap_or_else(|| {
                entries
                    .iter()
                    .take(index + 1)
                    .filter(|url| keep(url))
                    .count()
                    .saturating_sub(1)
            })
            .min(kept.len().saturating_sub(1));
        Self {
            url: current
                .filter(|url| keep(url))
                .cloned()
                .unwrap_or_else(|| tab.url().to_string()),
            entries: kept,
            index: mapped_index,
            title: tab.title.clone(),
            find_query: tab.find.query().to_string(),
            find_open: tab.find.open,
            zoom_percent: tab.zoom.percent(),
        }
    }
}

#[cfg(test)]
mod tests {
    use keyboard_types::Modifiers;

    use super::*;

    fn manager(urls: &[&str]) -> TabManager {
        let mut tabs = TabManager::new();
        for url in urls {
            tabs.add(*url);
        }
        tabs
    }

    #[test]
    fn ensure_one_tab_opens_a_default_tab() {
        let mut tabs = TabManager::new();
        assert!(tabs.is_empty());
        tabs.ensure_one_tab("https://example.com/");
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs.active().map(Tab::url), Some("https://example.com/"));
        tabs.ensure_one_tab("https://example.com/");
        assert_eq!(tabs.len(), 1);
    }

    #[test]
    fn selecting_active_tab_is_a_noop() {
        let mut tabs = manager(&["https://a/"]);
        let id = tabs.active_id().expect("active");
        assert!(!tabs.select(id));
        assert_eq!(tabs.active_id(), Some(id));
    }

    #[test]
    fn next_tab_on_single_tab_is_a_noop() {
        let mut tabs = manager(&["https://only/"]);
        assert!(!tabs.select_next());
        assert!(!tabs.select_previous());
    }

    #[test]
    fn a_new_tab_becomes_active() {
        let mut tabs = TabManager::new();
        assert!(tabs.is_empty());
        assert!(tabs.active().is_none());
        let first = tabs.add("https://a/");
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs.active_id(), Some(first));
        let second = tabs.add("https://b/");
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs.active_id(), Some(second));
        assert_eq!(tabs.active().map(Tab::url), Some("https://b/"));
    }

    #[test]
    fn closing_active_tab_selects_the_right_neighbour() {
        let mut tabs = manager(&["https://a/", "https://b/", "https://c/"]);
        let b = tabs.by_index(1).map(|tab| tab.id).expect("tab B");
        assert!(tabs.select(b));
        tabs.close_active();
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs.active().map(Tab::url), Some("https://c/"));
    }

    #[test]
    fn closing_active_tab_selects_the_left_neighbour_when_last() {
        let mut tabs = manager(&["https://a/", "https://b/", "https://c/"]);
        let c = tabs.by_index(2).map(|tab| tab.id).expect("tab C");
        assert_eq!(tabs.active_id(), Some(c));
        tabs.close_active();
        assert_eq!(tabs.active().map(Tab::url), Some("https://b/"));
    }

    #[test]
    fn closing_last_tab_leaves_no_active_tab() {
        let mut tabs = manager(&["https://a/"]);
        let closed = tabs.close_active().expect("closed tab");
        assert_eq!(closed.url(), "https://a/");
        assert!(tabs.is_empty());
        assert!(tabs.active().is_none());
    }

    #[test]
    fn next_and_previous_wrap() {
        let mut tabs = manager(&["https://a/", "https://b/", "https://c/"]);
        let c = tabs.by_index(2).map(|tab| tab.id).expect("tab C");
        tabs.select(c);
        assert!(tabs.select_next());
        assert_eq!(tabs.active().map(Tab::url), Some("https://a/"));
        assert!(tabs.select_previous());
        assert_eq!(tabs.active().map(Tab::url), Some("https://c/"));
    }

    #[test]
    fn closing_a_non_active_tab_keeps_selection() {
        let mut tabs = manager(&["https://a/", "https://b/"]);
        let a = tabs.by_index(0).map(|tab| tab.id).expect("tab A");
        assert!(tabs.close(a).is_some());
        assert_eq!(tabs.active().map(Tab::url), Some("https://b/"));
    }

    #[test]
    fn duplicate_tab_resets_elements_expansion() {
        let mut tabs = manager(&["https://a/"]);
        let source = tabs.active().expect("active").id;
        {
            let tab = tabs.active_mut().expect("active");
            tab.elements.expanded.insert(99);
            tab.elements.selected = Some(42);
            tab.elements_epoch = 5;
        }
        let duped = tabs.active().expect("active").clone();
        let new_id = tabs.insert(duped);
        assert_ne!(source, new_id);
        let tab = tabs.active().expect("dup tab");
        assert!(tab.elements.expanded.is_empty());
        assert_eq!(tab.elements.selected, None);
        assert_eq!(tab.elements_epoch, 1);
    }

    #[test]
    fn per_tab_state_is_independent() {
        let mut tabs = manager(&["https://a/", "https://b/"]);
        {
            let tab = tabs.active_mut().expect("active tab");
            tab.nav
                .navigate(crate::nav::NavKind::Go, Some("https://b/2".into()));
            tab.title = "B two".into();
            tab.zoom.zoom_in();
            tab.find.open = true;
        }
        let other = tabs.by_index(0).expect("other tab");
        assert_eq!(other.url(), "https://a/");
        assert_eq!(other.zoom.percent(), 100);
        assert!(!other.find.open);
        let active = tabs.active().expect("active tab");
        assert_eq!(active.url(), "https://b/2");
        assert_eq!(active.title, "B two");
        assert_eq!(active.zoom.percent(), 110);
    }

    #[test]
    fn session_round_trip_restores_all_tabs_and_active_index() {
        let mut tabs = manager(&["https://a/", "https://b/"]);
        {
            let tab = tabs.active_mut().expect("active tab");
            tab.title = "Second".into();
            tab.zoom.set_percent(150);
            tab.find.open = true;
        }
        let session = Session::from_tabs(&tabs, &std::collections::HashSet::new());
        assert_eq!(session.version, 2);
        let restored = TabManager::from_session(&session, None);
        assert_eq!(restored.len(), 2);
        assert_eq!(restored.active_index(), Some(1));
        let tab = restored.active().expect("active tab");
        assert_eq!(tab.url(), "https://b/");
        assert_eq!(tab.title, "Second");
        assert_eq!(tab.zoom.percent(), 150);
        assert!(tab.find.open);
    }

    #[test]
    fn session_round_trip_restores_find_query() {
        let mut tabs = manager(&["https://a/"]);
        {
            let tab = tabs.active_mut().expect("active tab");
            tab.find.set_query("needle");
            tab.find.open = true;
        }
        let session = Session::from_tabs(&tabs, &std::collections::HashSet::new());
        let restored = TabManager::from_session(&session, None);
        let tab = restored.active().expect("active tab");
        assert_eq!(tab.find.query(), "needle");
        assert!(tab.find.open);
    }

    #[test]
    fn command_line_url_starts_one_tab() {
        let session = Session::from_tabs(&manager(&["https://a/"]), &Default::default());
        let tabs = TabManager::from_session(&session, Some("https://cli/"));
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs.active().map(Tab::url), Some("https://cli/"));
    }

    #[test]
    fn session_skips_internal_and_failed_entries() {
        let mut tabs = manager(&["https://a/"]);
        {
            let tab = tabs.active_mut().expect("active tab");
            tab.nav.history_mut().push("glide://bookmarks".to_string());
            tab.nav.history_mut().push("https://failed/".to_string());
        }
        let mut failed = std::collections::HashSet::new();
        failed.insert("https://failed/".to_string());
        let session = Session::from_tabs(&tabs, &failed);
        assert_eq!(session.tabs[0].entries, vec!["https://a/"]);
        assert_eq!(session.tabs[0].index, 0);
    }

    #[test]
    fn selected_tab_by_index_out_of_range_does_nothing() {
        let mut tabs = manager(&["https://a/"]);
        assert!(!tabs.select_index(5));
        assert_eq!(tabs.active().map(Tab::url), Some("https://a/"));
    }

    #[test]
    fn link_target_from_plain_click_is_current_tab() {
        assert_eq!(
            link_target_from_modifiers(Modifiers::empty()),
            LinkTarget::CurrentTab
        );
        assert_eq!(
            link_target_from_modifiers(Modifiers::SHIFT),
            LinkTarget::CurrentTab
        );
    }

    #[test]
    fn link_target_from_modified_clicks() {
        assert_eq!(
            link_target_from_modifiers(Modifiers::META),
            LinkTarget::NewBackgroundTab
        );
        assert_eq!(
            link_target_from_modifiers(Modifiers::CONTROL),
            LinkTarget::NewBackgroundTab
        );
        assert_eq!(
            link_target_from_modifiers(Modifiers::META | Modifiers::SHIFT),
            LinkTarget::NewForegroundTab
        );
        assert_eq!(
            link_target_from_modifiers(Modifiers::CONTROL | Modifiers::SHIFT),
            LinkTarget::NewForegroundTab
        );
    }
}
