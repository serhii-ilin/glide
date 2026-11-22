//! Windowed browser: chrome shell, history, keyboard shortcuts, and JS pump.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use blitz_dom::{BaseDocument, Document, DocumentConfig, qual_name};
use blitz_net::Provider;
use blitz_shell::{
    BlitzApplication, BlitzShellEvent, BlitzShellProxy, WindowConfig, create_default_event_loop,
};
use blitz_traits::navigation::{NavigationOptions, NavigationProvider};
use winit::application::ApplicationHandler;
use winit::event::{ButtonSource, ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{WindowAttributes, WindowId};

use crate::devtools::console::apply_console_wire;
use crate::devtools::{DevToolsDock, DevToolsSession, DevToolsTab};
use crate::js::console_feed;

use std::sync::atomic::Ordering;

use crate::bookmarks::BookmarksState;
use crate::chrome::{
    ChromeViewState, MenuId, NavCommand, TabView, chrome_document_from_state, command_closes_menu,
    command_from_url, render_menu_bar, render_tab_strip, set_address, set_loading_ui,
    sync_committed_address,
};
use crate::html::{GlideHtmlProvider, HtmlPageDocument, set_js_runtime_mode};
use crate::js::{JsHost, JsTap};
use crate::keyboard;
use crate::location::resolve_location;
use crate::nav::{NavKind, ObserveEffect};
use crate::net::{GlideNet, LoadState};
use crate::parse;
use crate::profile::{
    HistoryCommitKind, HistoryEntry, HistoryLogState, PendingHistoryCommand, ProfileStore, Session,
    Settings, effective_js_enabled, history_log_state_for_launch, settings_from_saved,
    should_append_history_log, should_persist_url,
};
use crate::tabs::{Tab, TabManager};
use crate::zoom::PageZoom;

const CHROME_BASE: &str = "https://glide.invalid/chrome";

type Renderer = anyrender_vello_cpu::VelloCpuWindowRenderer;

/// Startup state for the windowed browser.
pub struct WindowLaunch {
    pub tabs: TabManager,
    pub defer_session_save: bool,
    pub js_enabled: bool,
    pub persist_js_setting: bool,
    pub default_zoom: PageZoom,
    pub profile: ProfileStore,
    pub saved_settings: Settings,
    pub history_log: HistoryLogState,
}

pub fn prepare_window_launch(
    cli_url: Option<&str>,
    cli_no_js: bool,
    profile: ProfileStore,
) -> WindowLaunch {
    let saved = profile.load_settings();
    let (saved_settings, default_zoom) = settings_from_saved(&saved);
    let js_enabled = effective_js_enabled(cli_no_js, &saved_settings);
    let session = profile.load_session();
    let defer_session_save = cli_url.is_some();
    let mut tabs = TabManager::from_session(&session, cli_url);
    tabs.ensure_one_tab(crate::profile::DEFAULT_START_URL);
    let start_url = tabs
        .active()
        .map(Tab::url)
        .unwrap_or(crate::profile::DEFAULT_START_URL)
        .to_string();
    let history_log = history_log_state_for_launch(cli_url, &start_url, &session);
    WindowLaunch {
        tabs,
        defer_session_save,
        js_enabled,
        persist_js_setting: !cli_no_js,
        default_zoom,
        profile,
        saved_settings,
        history_log,
    }
}

pub(crate) fn chrome_accepts_navigation(source_document: usize, chrome_doc_id: usize) -> bool {
    chrome_doc_id != 0 && source_document == chrome_doc_id
}

pub fn run_window(launch: WindowLaunch) -> Result<(), String> {
    let WindowLaunch {
        mut tabs,
        defer_session_save,
        js_enabled,
        persist_js_setting,
        default_zoom,
        profile,
        saved_settings,
        history_log,
    } = launch;
    let profile = Arc::new(profile);
    let event_loop = create_default_event_loop();
    let (proxy, receiver) = BlitzShellProxy::new(event_loop.create_proxy());
    let load_state = Arc::new(LoadState::default());
    let net_impl = Arc::new(Provider::new(Some(Arc::new(proxy.clone()))));
    let waker: Arc<dyn blitz_traits::net::NetWaker> = Arc::new(proxy.clone());
    let bookmarks_state = BookmarksState::load(profile.clone());
    let bookmark_list = Arc::new(std::sync::RwLock::new(bookmarks_state.entries().to_vec()));
    let bookmarks = BookmarksState::shared(bookmark_list.clone());
    // Shared by the network layer (noscript meta refresh) and the HTML provider.
    let scripting_enabled = Arc::new(AtomicBool::new(js_enabled));
    let net = GlideNet::with_scripting(
        net_impl.clone(),
        load_state.clone(),
        waker,
        bookmarks,
        scripting_enabled.clone(),
    );
    let bus = NavBus::default();

    let js_tap = if js_enabled {
        JsTap::new_enabled()
    } else {
        JsTap::new_disabled()
    };
    let html_provider = GlideHtmlProvider {
        scripting_enabled: Arc::clone(&scripting_enabled),
        js_tap: js_tap.clone(),
    };
    let js_host = JsHost::new(js_enabled, js_tap, Some(net_impl));

    let chrome_config = DocumentConfig {
        base_url: Some(CHROME_BASE.to_string()),
        net_provider: Some(net.clone()),
        navigation_provider: Some(Arc::new(ChromeNav {
            bus: bus.clone(),
            proxy: proxy.clone(),
            load_state: load_state.clone(),
        })),
        html_parser_provider: Some(Arc::new(html_provider.clone())),
        ..Default::default()
    };

    let view = chrome_view_state(&tabs, js_enabled, &bookmarks_state, None);
    let devtools = DevToolsSession::from_settings(&saved_settings);
    let mut doc = build_chrome_page_document(&view, js_enabled, &load_state, chrome_config);
    {
        let mut chrome = doc.0.inner_mut();
        if let Some(tab) = tabs.active_mut() {
            let find_open = tab.find.open;
            let find_query = tab.find.query().to_string();
            tab.find.restore(&mut chrome, find_open, &find_query);
            tab.zoom.apply_to_chrome(&mut chrome);
        }
        devtools.sync_chrome(&mut chrome);
    }

    let attrs = WindowAttributes::default()
        .with_title("Glide")
        .with_surface_size(winit::dpi::LogicalSize::new(1100.0, 760.0));
    let window = WindowConfig::with_attributes(Box::new(doc), Renderer::new(), attrs);

    let mut app = Glide::new(GlideBootstrap {
        proxy,
        receiver,
        bus,
        tabs,
        js: js_host,
        html_provider,
        load_state,
        session: GlideSessionState {
            profile,
            saved_settings,
            persist_js_setting,
            defer_session_save,
            default_zoom,
        },
        bookmarks: bookmarks_state,
        bookmark_list: bookmark_list.clone(),
        history_log,
        devtools,
    });
    app.inner.add_window(window);
    event_loop
        .run_app(app)
        .map_err(|err| format!("glide exited: {err}"))
}

#[derive(Clone, Default)]
struct NavBus {
    inner: Arc<Mutex<Vec<NavCommand>>>,
}

impl NavBus {
    fn push(&self, command: NavCommand) {
        self.inner.lock().expect("nav bus").push(command);
    }

    fn take(&self) -> Vec<NavCommand> {
        std::mem::take(&mut *self.inner.lock().expect("nav bus"))
    }
}

struct ChromeNav {
    bus: NavBus,
    proxy: BlitzShellProxy,
    load_state: Arc<LoadState>,
}

impl NavigationProvider for ChromeNav {
    fn navigate_to(&self, options: NavigationOptions) {
        let chrome_id = self.load_state.chrome_doc_id();
        if !chrome_accepts_navigation(options.source_document, chrome_id) {
            return;
        }
        if let Some(command) = command_from_url(&options.url) {
            self.bus.push(command);
            self.proxy.wake_up();
        }
    }
}

struct GlideSessionState {
    profile: Arc<ProfileStore>,
    saved_settings: Settings,
    persist_js_setting: bool,
    defer_session_save: bool,
    default_zoom: PageZoom,
}

struct GlideBootstrap {
    proxy: BlitzShellProxy,
    receiver: std::sync::mpsc::Receiver<BlitzShellEvent>,
    bus: NavBus,
    tabs: TabManager,
    js: JsHost,
    html_provider: GlideHtmlProvider,
    load_state: Arc<LoadState>,
    session: GlideSessionState,
    bookmarks: BookmarksState,
    bookmark_list: Arc<std::sync::RwLock<Vec<crate::profile::Bookmark>>>,
    history_log: HistoryLogState,
    devtools: DevToolsSession,
}

struct DevToolsSplitDrag {
    dock: DevToolsDock,
    start_axis: f32,
    start_size: i32,
}

enum DevToolsPointerOutcome {
    NotConsumed,
    Consumed,
    ConsumedSaveSettings,
}

fn handle_devtools_pointer(
    devtools: &mut DevToolsSession,
    drag: &mut Option<DevToolsSplitDrag>,
    view: &mut blitz_shell::View<Renderer>,
    event: &WindowEvent,
) -> DevToolsPointerOutcome {
    match event {
        WindowEvent::PointerLeft { .. } if drag.is_some() => {
            *drag = None;
            DevToolsPointerOutcome::ConsumedSaveSettings
        }
        WindowEvent::PointerButton {
            button,
            state,
            position,
            ..
        } => {
            if *state == ElementState::Released {
                if drag.is_some() {
                    *drag = None;
                    return DevToolsPointerOutcome::ConsumedSaveSettings;
                }
                return DevToolsPointerOutcome::NotConsumed;
            }
            if !matches!(button, ButtonSource::Mouse(MouseButton::Left)) {
                return DevToolsPointerOutcome::NotConsumed;
            }
            if !devtools.state.open {
                return DevToolsPointerOutcome::NotConsumed;
            }
            let coords = view.pointer_coords(*position);
            let mut chrome = view.doc.inner_mut();
            let Some(node_id) =
                crate::chrome::hit_test_chrome(&mut chrome, coords.client_x, coords.client_y)
            else {
                return DevToolsPointerOutcome::NotConsumed;
            };
            if !crate::chrome::node_is_devtools_split(&chrome, node_id) {
                return DevToolsPointerOutcome::NotConsumed;
            }
            drop(chrome);
            *drag = Some(DevToolsSplitDrag {
                dock: devtools.state.dock,
                start_axis: match devtools.state.dock {
                    DevToolsDock::Bottom => coords.client_y,
                    DevToolsDock::Right => coords.client_x,
                },
                start_size: devtools.state.size_px,
            });
            DevToolsPointerOutcome::Consumed
        }
        WindowEvent::PointerMoved { position, .. } if drag.is_some() => {
            let coords = view.pointer_coords(*position);
            let dock = drag.as_ref().map(|d| d.dock).unwrap_or(devtools.state.dock);
            let start_axis = drag.as_ref().map(|d| d.start_axis).unwrap_or(0.0);
            let start_size = drag.as_ref().map(|d| d.start_size).unwrap_or(0);
            let axis = match dock {
                DevToolsDock::Bottom => coords.client_y,
                DevToolsDock::Right => coords.client_x,
            };
            let zoom = {
                let chrome = view.doc.inner();
                chrome.viewport().zoom()
            };
            let size_px = crate::devtools::panel_size_from_drag(start_size, start_axis, axis, zoom);
            let resized = {
                let mut chrome = view.doc.inner_mut();
                devtools.resize_from_drag(&mut chrome, size_px).is_some()
            };
            if resized {
                view.request_redraw();
            }
            DevToolsPointerOutcome::Consumed
        }
        _ => DevToolsPointerOutcome::NotConsumed,
    }
}

struct Glide {
    inner: BlitzApplication<Renderer>,
    bus: NavBus,
    js: JsHost,
    html_provider: GlideHtmlProvider,
    tabs: TabManager,
    devtools: DevToolsSession,
    devtools_drag: Option<DevToolsSplitDrag>,
    last_title: String,
    load_state: Arc<LoadState>,
    chrome_loading: bool,
    open_menu: Option<MenuId>,
    session: GlideSessionState,
    bookmarks: BookmarksState,
    bookmark_list: Arc<std::sync::RwLock<Vec<crate::profile::Bookmark>>>,
    history_log: HistoryLogState,
}

fn chrome_view_state(
    tabs: &TabManager,
    js_enabled: bool,
    bookmarks: &BookmarksState,
    open_menu: Option<MenuId>,
) -> ChromeViewState {
    let active = tabs.active();
    let url = active.map(Tab::url).unwrap_or("about:blank").to_string();
    let (can_go_back, can_go_forward, page_loading) = active
        .map(|tab| {
            let history = tab.nav.history();
            (
                history.index() > 0,
                history.index() + 1 < history.entries().len(),
                tab.loading,
            )
        })
        .unwrap_or((false, false, false));
    ChromeViewState {
        bookmarked: bookmarks.is_bookmarked(&url),
        url,
        title: active.map(|tab| tab.title.clone()).unwrap_or_default(),
        js_enabled,
        can_go_back,
        can_go_forward,
        page_loading,
        tabs: tabs
            .tabs()
            .iter()
            .map(|tab| TabView {
                id: tab.id,
                title: tab.title.clone(),
                loading: tab.loading,
            })
            .collect(),
        active_index: tabs.active_index(),
        open_menu,
    }
}

fn build_chrome_page_document(
    view: &ChromeViewState,
    js_enabled: bool,
    load_state: &LoadState,
    chrome_config: DocumentConfig,
) -> HtmlPageDocument {
    HtmlPageDocument(parse::document_from_html_with(
        &chrome_document_from_state(view),
        chrome_config,
        js_enabled,
        |doc| load_state.set_chrome_doc_id(doc.id()),
    ))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Whether a tab title changed enough to warrant redrawing the tab strip.
fn titles_changed(tabs: &TabManager, before: &[String]) -> bool {
    tabs.tabs().len() != before.len()
        || tabs
            .tabs()
            .iter()
            .zip(before.iter())
            .any(|(tab, title)| tab.title != *title)
}

fn capture_active_tab(
    tabs: &mut TabManager,
    doc: &mut BaseDocument,
    bookmarks: &mut BookmarksState,
    load_state: &LoadState,
    committed: &mut Option<(ObserveEffect, String, String)>,
    js: &mut JsHost,
) {
    let Some(actual) = page_url(doc) else {
        return;
    };
    let title = page_title(doc).unwrap_or_default();
    let effect = {
        let Some(tab) = tabs.active_mut() else {
            return;
        };
        tab.find.sync_from_input(doc);
        let effect = tab.nav.observe(&actual);
        tab.title = title.clone();
        tab.loading = load_state.page_loading();
        let shown = tab.nav.shown().to_string();
        sync_committed_address(doc, &shown, &mut tab.address_written);
        if matches!(
            effect,
            ObserveEffect::LinkNavigation | ObserveEffect::Redirect
        ) && tab.find.open
        {
            tab.find.close_bar(doc);
        }
        crate::devtools::console::observe_navigation_console_effect(
            effect,
            tab,
            js,
            &mut reset_tab_console_for_navigation,
        );
        effect
    };
    bookmarks.sync_star(doc, &actual);
    if matches!(
        effect,
        ObserveEffect::Completed | ObserveEffect::LinkNavigation | ObserveEffect::Redirect
    ) {
        *committed = Some((effect, actual, title));
    }
}

fn reset_tab_console_for_navigation(tab: &mut Tab, js: &mut JsHost) {
    tab.console.clear();
    tab.console_epoch = tab.console_epoch.saturating_add(1);
    console_feed::discard_pending_for_tab(tab.id);
    js.set_console_route(tab.id, tab.console_epoch);
}

fn sync_active_tab_console(
    devtools: &mut DevToolsSession,
    tabs: &TabManager,
    doc: &mut BaseDocument,
) -> bool {
    let Some(tab) = tabs.active() else {
        return false;
    };
    devtools.sync_console_pane(doc, &tab.console, tab.id)
}

fn apply_active_tab_to_doc(
    tabs: &mut TabManager,
    doc: &mut BaseDocument,
    js: &mut JsHost,
    tab_switched: bool,
) {
    let url = tabs
        .active()
        .map(Tab::url)
        .unwrap_or("about:blank")
        .to_string();
    let observed = page_url(doc);
    let reload = observed.as_deref() != Some(url.as_str());
    if let Some(tab) = tabs.active_mut() {
        tab.nav.switch_to(&url, observed.as_deref());
        tab.address_written = url.clone();
        set_address(doc, &url);
        let find_open = tab.find.open;
        let find_query = tab.find.query().to_string();
        tab.find.restore(doc, find_open, &find_query);
        tab.zoom.apply_to_chrome(doc);
    }
    if tab_switched {
        if reload {
            if let Some(tab) = tabs.active_mut() {
                bump_tab_console_epoch_for_switch(tab, js);
            }
        } else if let Some(tab) = tabs.active() {
            js.set_console_route(tab.id, tab.console_epoch);
        }
    }
    if reload {
        if !tab_switched && let Some(tab) = tabs.active() {
            js.set_console_route(tab.id, tab.console_epoch);
        }
        js.on_navigation(doc, &url);
        load_iframe(doc, &url);
    }
}

fn bump_tab_console_epoch_for_switch(tab: &mut Tab, js: &mut JsHost) {
    crate::devtools::console::bump_tab_console_epoch_for_switch(tab, js);
}

impl Glide {
    fn new(init: GlideBootstrap) -> Self {
        Self {
            inner: BlitzApplication::new(init.proxy, init.receiver),
            bus: init.bus,
            js: init.js,
            html_provider: init.html_provider,
            tabs: init.tabs,
            last_title: "Glide".to_string(),
            load_state: init.load_state,
            chrome_loading: false,
            open_menu: None,
            session: init.session,
            bookmarks: init.bookmarks,
            bookmark_list: init.bookmark_list,
            history_log: init.history_log,
            devtools: init.devtools,
            devtools_drag: None,
        }
    }

    fn save_settings(&self) -> Result<(), String> {
        let js_enabled = if self.session.persist_js_setting {
            self.html_provider.scripting_enabled.load(Ordering::Relaxed)
        } else {
            self.session.saved_settings.js_enabled
        };
        let (devtools_dock, devtools_size_px) = self.devtools.settings_fields();
        let settings = Settings {
            js_enabled,
            zoom_percent: self
                .tabs
                .active()
                .map(|tab| tab.zoom.percent())
                .unwrap_or_else(|| self.session.default_zoom.percent()),
            devtools_dock,
            devtools_size_px,
        };
        self.session.profile.save_settings(&settings)
    }

    fn save_session_if_allowed(&self) {
        if self.session.defer_session_save {
            return;
        }
        let failed = self.load_state.failed_page_urls();
        let session = Session::from_tabs(&self.tabs, &failed);
        let _ = self.session.profile.save_session(&session);
    }

    fn on_committed_navigation(&mut self, effect: ObserveEffect, url: &str, title: &str) {
        if matches!(
            effect,
            ObserveEffect::LinkNavigation | ObserveEffect::Redirect
        ) {
            self.session.defer_session_save = false;
        }
        if !matches!(
            effect,
            ObserveEffect::Completed | ObserveEffect::LinkNavigation | ObserveEffect::Redirect
        ) {
            return;
        }
        if self.load_state.page_load_failed(url) || !should_persist_url(url) {
            if !self.session.defer_session_save {
                self.save_session_if_allowed();
            }
            return;
        }

        let commit_kind = match effect {
            ObserveEffect::Completed => HistoryCommitKind::Completed,
            ObserveEffect::LinkNavigation => HistoryCommitKind::LinkNavigation,
            ObserveEffect::Redirect => HistoryCommitKind::Redirect,
            ObserveEffect::Unchanged => return,
        };
        if should_append_history_log(&mut self.history_log, commit_kind, url) {
            let entry = HistoryEntry {
                ts_unix: unix_now(),
                url: url.to_string(),
                title: title.to_string(),
            };
            let _ = self.session.profile.append_history(&entry);
        }
        if !self.session.defer_session_save {
            self.save_session_if_allowed();
        }
    }

    fn sync(&mut self) {
        let Some(window_id) = self.inner.windows.keys().next().copied() else {
            return;
        };
        let mut changed = false;
        let mut structure_changed = false;
        let mut save_settings_after = false;
        let mut committed: Option<(ObserveEffect, String, String)> = None;
        let mut pending_history: Option<PendingHistoryCommand> = None;
        let js_enabled = self.html_provider.scripting_enabled.load(Ordering::Relaxed);
        let titles_before = self
            .tabs
            .tabs()
            .iter()
            .map(|tab| tab.title.clone())
            .collect::<Vec<_>>();
        if let Some(view) = self.inner.windows.get_mut(&window_id) {
            let mut doc = view.doc.inner_mut();
            for command in self.bus.take() {
                changed = true;
                if command_closes_menu(&command) && self.open_menu.is_some() {
                    self.open_menu = None;
                    structure_changed = true;
                }
                match &command {
                    NavCommand::Back | NavCommand::Forward | NavCommand::Reload => {
                        let kind = match &command {
                            NavCommand::Back => NavKind::Back,
                            NavCommand::Forward => NavKind::Forward,
                            _ => NavKind::Reload,
                        };
                        let history = match &command {
                            NavCommand::Back => PendingHistoryCommand::Back,
                            NavCommand::Forward => PendingHistoryCommand::Forward,
                            _ => PendingHistoryCommand::Reload,
                        };
                        self.session.defer_session_save = false;
                        pending_history = Some(history);
                        capture_active_tab(
                            &mut self.tabs,
                            &mut doc,
                            &mut self.bookmarks,
                            &self.load_state,
                            &mut committed,
                            &mut self.js,
                        );
                        let target = {
                            let Some(tab) = self.tabs.active_mut() else {
                                continue;
                            };
                            if tab.find.open {
                                tab.find.close_bar(&mut doc);
                            }
                            tab.nav.navigate(kind, None)
                        };
                        if let Some(url) = target {
                            if let Some(tab) = self.tabs.active_mut() {
                                reset_tab_console_for_navigation(tab, &mut self.js);
                            }
                            self.js.on_navigation(&doc, &url);
                            load_iframe(&mut doc, &url);
                            set_address(&mut doc, &url);
                            if let Some(tab) = self.tabs.active_mut() {
                                tab.address_written = tab.nav.shown().to_string();
                            }
                        }
                    }
                    NavCommand::Stop => {
                        self.load_state.stop_page_load();
                        if let Some(url) = page_url(&doc)
                            && let Some(tab) = self.tabs.active_mut()
                        {
                            tab.nav.stop(&url);
                            set_address(&mut doc, &url);
                            tab.address_written = tab.nav.shown().to_string();
                        }
                    }
                    NavCommand::ToggleJs => {
                        self.session.defer_session_save = false;
                        pending_history = Some(PendingHistoryCommand::Reload);
                        capture_active_tab(
                            &mut self.tabs,
                            &mut doc,
                            &mut self.bookmarks,
                            &self.load_state,
                            &mut committed,
                            &mut self.js,
                        );
                        let new = !js_enabled;
                        set_js_runtime_mode(&self.html_provider, &mut self.js, &doc, new);
                        let target = self
                            .tabs
                            .active_mut()
                            .and_then(|tab| tab.nav.navigate(NavKind::Reload, None));
                        if let Some(url) = target {
                            if let Some(tab) = self.tabs.active_mut() {
                                reset_tab_console_for_navigation(tab, &mut self.js);
                            }
                            self.js.on_navigation(&doc, &url);
                            load_iframe(&mut doc, &url);
                            set_js_indicator(&mut doc, new);
                        }
                        self.session.persist_js_setting = true;
                        save_settings_after = true;
                    }
                    NavCommand::ToggleBookmark => {
                        let url = self.tabs.active().map(Tab::url).unwrap_or("").to_string();
                        if should_persist_url(&url) {
                            let title = page_title(&doc).unwrap_or_default();
                            self.bookmarks.toggle(&mut doc, &url, &title, unix_now());
                            if let Ok(mut shared) = self.bookmark_list.write() {
                                *shared = self.bookmarks.entries().to_vec();
                            }
                        }
                    }
                    NavCommand::Go(raw) => {
                        self.session.defer_session_save = false;
                        pending_history = Some(PendingHistoryCommand::Go);
                        capture_active_tab(
                            &mut self.tabs,
                            &mut doc,
                            &mut self.bookmarks,
                            &self.load_state,
                            &mut committed,
                            &mut self.js,
                        );
                        let url = resolve_location(raw);
                        if let Some(tab) = self.tabs.active_mut() {
                            reset_tab_console_for_navigation(tab, &mut self.js);
                        }
                        self.js.on_navigation(&doc, &url);
                        load_iframe(&mut doc, &url);
                        if let Some(tab) = self.tabs.active_mut() {
                            tab.nav.navigate(NavKind::Go, Some(url.clone()));
                            set_address(&mut doc, &url);
                            tab.address_written = tab.nav.shown().to_string();
                        }
                    }
                    NavCommand::Find => {
                        if let Some(tab) = self.tabs.active_mut() {
                            tab.find.open_bar(&mut doc);
                        }
                    }
                    NavCommand::FindClose => {
                        if let Some(tab) = self.tabs.active_mut() {
                            tab.find.close_bar(&mut doc);
                        }
                    }
                    NavCommand::FindNext => {
                        if let Some(tab) = self.tabs.active_mut() {
                            tab.find.step(&mut doc, true);
                        }
                    }
                    NavCommand::FindPrev => {
                        if let Some(tab) = self.tabs.active_mut() {
                            tab.find.step(&mut doc, false);
                        }
                    }
                    NavCommand::DevToolsToggle => {
                        self.devtools.toggle(&mut doc);
                        sync_active_tab_console(&mut self.devtools, &self.tabs, &mut doc);
                        save_settings_after = true;
                    }
                    NavCommand::DevToolsDockBottom => {
                        self.devtools.set_dock(DevToolsDock::Bottom, &mut doc);
                        save_settings_after = true;
                    }
                    NavCommand::DevToolsDockRight => {
                        self.devtools.set_dock(DevToolsDock::Right, &mut doc);
                        save_settings_after = true;
                    }
                    NavCommand::DevToolsTab(tab) => {
                        self.devtools.set_tab(*tab, &mut doc);
                        if *tab == DevToolsTab::Console {
                            sync_active_tab_console(&mut self.devtools, &self.tabs, &mut doc);
                        }
                    }
                    NavCommand::DevToolsConsoleClear => {
                        if let Some(tab) = self.tabs.active_mut() {
                            tab.console.clear();
                            console_feed::discard_pending_for_tab(tab.id);
                        }
                        self.devtools.repl_ui.reset_after_submit();
                        sync_active_tab_console(&mut self.devtools, &self.tabs, &mut doc);
                    }
                    NavCommand::ZoomIn | NavCommand::ZoomOut | NavCommand::ZoomReset => {
                        if let Some(tab) = self.tabs.active_mut() {
                            match &command {
                                NavCommand::ZoomIn => tab.zoom.zoom_in(),
                                NavCommand::ZoomOut => tab.zoom.zoom_out(),
                                _ => tab.zoom.reset(),
                            }
                            tab.zoom.apply_to_chrome(&mut doc);
                        }
                        save_settings_after = true;
                    }
                    NavCommand::NewTab => {
                        capture_active_tab(
                            &mut self.tabs,
                            &mut doc,
                            &mut self.bookmarks,
                            &self.load_state,
                            &mut committed,
                            &mut self.js,
                        );
                        self.tabs.add("about:blank");
                        if let Some(tab) = self.tabs.active_mut() {
                            tab.zoom = self.session.default_zoom;
                        }
                        apply_active_tab_to_doc(&mut self.tabs, &mut doc, &mut self.js, false);
                        sync_active_tab_console(&mut self.devtools, &self.tabs, &mut doc);
                        structure_changed = true;
                    }
                    NavCommand::DuplicateTab => {
                        capture_active_tab(
                            &mut self.tabs,
                            &mut doc,
                            &mut self.bookmarks,
                            &self.load_state,
                            &mut committed,
                            &mut self.js,
                        );
                        if let Some(tab) = self.tabs.active().cloned() {
                            self.tabs.insert(tab);
                            apply_active_tab_to_doc(&mut self.tabs, &mut doc, &mut self.js, false);
                            sync_active_tab_console(&mut self.devtools, &self.tabs, &mut doc);
                            structure_changed = true;
                        }
                    }
                    NavCommand::CloseTab(_) | NavCommand::CloseActiveTab => {
                        let id = match &command {
                            NavCommand::CloseTab(id) => *id,
                            _ => self.tabs.active_id().unwrap_or(0),
                        };
                        capture_active_tab(
                            &mut self.tabs,
                            &mut doc,
                            &mut self.bookmarks,
                            &self.load_state,
                            &mut committed,
                            &mut self.js,
                        );
                        if self.tabs.active_id() == Some(id) {
                            self.tabs.close_active();
                            if self.tabs.is_empty() {
                                let failed = self.load_state.failed_page_urls();
                                let session = Session::from_tabs(&self.tabs, &failed);
                                let _ = self.session.profile.save_session(&session);
                                self.inner
                                    .proxy
                                    .send_event(BlitzShellEvent::CloseWindow { window_id });
                                break;
                            }
                            apply_active_tab_to_doc(&mut self.tabs, &mut doc, &mut self.js, false);
                            sync_active_tab_console(&mut self.devtools, &self.tabs, &mut doc);
                        } else {
                            self.tabs.close(id);
                        }
                        structure_changed = true;
                    }
                    NavCommand::SelectTab(_) | NavCommand::SelectTabIndex(_) => {
                        capture_active_tab(
                            &mut self.tabs,
                            &mut doc,
                            &mut self.bookmarks,
                            &self.load_state,
                            &mut committed,
                            &mut self.js,
                        );
                        let selected = match &command {
                            NavCommand::SelectTab(id) => self.tabs.select(*id),
                            NavCommand::SelectTabIndex(index) => self.tabs.select_index(*index),
                            _ => false,
                        };
                        if selected {
                            apply_active_tab_to_doc(&mut self.tabs, &mut doc, &mut self.js, true);
                            sync_active_tab_console(&mut self.devtools, &self.tabs, &mut doc);
                            structure_changed = true;
                        }
                    }
                    NavCommand::NextTab | NavCommand::PrevTab => {
                        capture_active_tab(
                            &mut self.tabs,
                            &mut doc,
                            &mut self.bookmarks,
                            &self.load_state,
                            &mut committed,
                            &mut self.js,
                        );
                        let moved = if matches!(&command, NavCommand::NextTab) {
                            self.tabs.select_next()
                        } else {
                            self.tabs.select_previous()
                        };
                        if moved {
                            apply_active_tab_to_doc(&mut self.tabs, &mut doc, &mut self.js, true);
                            sync_active_tab_console(&mut self.devtools, &self.tabs, &mut doc);
                            structure_changed = true;
                        }
                    }
                    NavCommand::MenuOpen(menu) => {
                        self.open_menu = Some(*menu);
                        structure_changed = true;
                    }
                    NavCommand::MenuClose => {
                        self.open_menu = None;
                        structure_changed = true;
                    }
                    NavCommand::CloseWindow => {
                        let failed = self.load_state.failed_page_urls();
                        let session = Session::from_tabs(&self.tabs, &failed);
                        let _ = self.session.profile.save_session(&session);
                        self.inner
                            .proxy
                            .send_event(BlitzShellEvent::CloseWindow { window_id });
                        break;
                    }
                }
            }

            capture_active_tab(
                &mut self.tabs,
                &mut doc,
                &mut self.bookmarks,
                &self.load_state,
                &mut committed,
                &mut self.js,
            );

            if titles_changed(&self.tabs, &titles_before) {
                structure_changed = true;
            }
            if let Some(tab) = self.tabs.active() {
                let display = if tab.title.trim().is_empty() {
                    tab.url().to_string()
                } else {
                    tab.title.clone()
                };
                crate::chrome::set_address_title(&mut doc, &display);
            }
            let address_focused = crate::chrome::address_bar_focused(&doc);
            crate::chrome::set_address_focused(&mut doc, address_focused);
            let loading = self.load_state.page_loading();
            if loading != self.chrome_loading {
                let was_loading = self.chrome_loading;
                self.chrome_loading = loading;
                set_loading_ui(&mut doc, loading);
                if was_loading
                    && !loading
                    && let Some(tab) = self.tabs.active_mut()
                {
                    tab.find.rerun_search_after_load(&mut doc);
                }
                changed = true;
                structure_changed = true;
            }

            if structure_changed {
                let view =
                    chrome_view_state(&self.tabs, js_enabled, &self.bookmarks, self.open_menu);
                render_tab_strip(&mut doc, &view);
                render_menu_bar(&mut doc, &view);
            }

            if let Some(title) = page_title(&doc) {
                let window_title = if title.is_empty() {
                    "Glide".to_string()
                } else {
                    format!("{title} — Glide")
                };
                if window_title != self.last_title {
                    self.last_title = window_title.clone();
                    doc.shell_provider.set_window_title(window_title);
                }
            }

            if self.devtools.reclamp_open_panel(&mut doc) {
                changed = true;
                save_settings_after = true;
            }

            // Chrome rebuilds (tab strip, menu bar, address text, loading icon) drop
            // nodes; a surviving paint or stacking entry would panic the next hit test
            // or paint, which the shell runs after this sync.
            crate::js::prune_stale_paint_children(&mut doc);
        }

        if let Some(cmd) = pending_history {
            self.history_log.pending_command = Some(cmd);
        }
        if save_settings_after {
            let _ = self.save_settings();
        }
        if structure_changed {
            self.save_session_if_allowed();
        }
        if let Some((effect, url, title)) = committed {
            self.on_committed_navigation(effect, &url, &title);
        }

        let Some(view) = self.inner.windows.get_mut(&window_id) else {
            return;
        };
        if changed {
            view.request_redraw();
        }

        let page_url_now = {
            let chrome = view.doc.inner();
            page_url(&chrome)
        };
        if let Some(tab) = self.tabs.active() {
            self.js.set_console_route(tab.id, tab.console_epoch);
        }
        if let Some(url) = &page_url_now {
            let handle = tokio::runtime::Handle::current();
            let chrome = view.doc.inner();
            self.js.maybe_start_page(&chrome, url, &handle);
        }
        let mut js_redraw = if let Some(_url) = &page_url_now {
            let mut chrome = view.doc.inner_mut();
            self.js.pump(&mut chrome, Instant::now())
        } else {
            let _ = self.js.drain_console_wire();
            false
        };
        let wire = self.js.drain_console_wire();
        let had_console = apply_console_wire(&mut self.tabs, &wire);
        if had_console {
            let mut chrome = view.doc.inner_mut();
            if sync_active_tab_console(&mut self.devtools, &self.tabs, &mut chrome) {
                js_redraw = true;
            }
            js_redraw = js_redraw
                || (self.devtools.state.open && self.devtools.state.tab == DevToolsTab::Console);
        }
        if js_redraw {
            view.request_redraw();
        }
    }

    fn handle_browser_shortcut(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: &WindowEvent,
    ) -> bool {
        let Some(view) = self.inner.windows.get_mut(&window_id) else {
            return false;
        };
        let Some(shortcut) = keyboard::try_browser_shortcut(view, event) else {
            return false;
        };
        self.apply_browser_shortcut(event_loop, window_id, shortcut)
    }

    fn handle_browser_shortcut_value(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        shortcut: keyboard::BrowserShortcut,
    ) {
        if self.apply_browser_shortcut(event_loop, window_id, shortcut) {
            self.inner
                .proxy
                .send_event(BlitzShellEvent::Poll { window_id });
            self.sync();
        }
    }

    fn apply_browser_shortcut(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        shortcut: keyboard::BrowserShortcut,
    ) -> bool {
        if !keyboard::consume_stop_shortcut(&shortcut, self.load_state.page_loading())
            && matches!(shortcut, keyboard::BrowserShortcut::Stop)
        {
            return false;
        }
        let mut save_zoom_settings = false;
        let mut save_devtools_settings = false;
        let handled = {
            let Some(view) = self.inner.windows.get_mut(&window_id) else {
                return false;
            };
            match shortcut {
                keyboard::BrowserShortcut::FocusAddressBar => {
                    keyboard::focus_address_bar(view);
                    view.request_redraw();
                    true
                }
                keyboard::BrowserShortcut::CloseTab => {
                    let failed = self.load_state.failed_page_urls();
                    let session = Session::from_tabs(&self.tabs, &failed);
                    let _ = self.session.profile.save_session(&session);
                    if self.tabs.len() <= 1 {
                        self.inner
                            .proxy
                            .send_event(BlitzShellEvent::CloseWindow { window_id });
                    } else {
                        self.bus.push(NavCommand::CloseActiveTab);
                    }
                    true
                }
                keyboard::BrowserShortcut::Quit => {
                    let failed = self.load_state.failed_page_urls();
                    let session = Session::from_tabs(&self.tabs, &failed);
                    let _ = self.session.profile.save_session(&session);
                    for id in self.inner.windows.keys().copied().collect::<Vec<_>>() {
                        self.inner
                            .proxy
                            .send_event(BlitzShellEvent::CloseWindow { window_id: id });
                    }
                    event_loop.exit();
                    true
                }
                keyboard::BrowserShortcut::NewTab => {
                    self.bus.push(NavCommand::NewTab);
                    true
                }
                keyboard::BrowserShortcut::NextTab => {
                    self.bus.push(NavCommand::NextTab);
                    true
                }
                keyboard::BrowserShortcut::PreviousTab => {
                    self.bus.push(NavCommand::PrevTab);
                    true
                }
                keyboard::BrowserShortcut::SelectTab(index) => {
                    self.bus.push(NavCommand::SelectTabIndex(index));
                    true
                }
                keyboard::BrowserShortcut::Scroll(action) => {
                    keyboard::scroll_page(view, action);
                    view.request_redraw();
                    true
                }
                keyboard::BrowserShortcut::ZoomIn
                | keyboard::BrowserShortcut::ZoomOut
                | keyboard::BrowserShortcut::ZoomReset => {
                    if let Some(tab) = self.tabs.active_mut() {
                        match shortcut {
                            keyboard::BrowserShortcut::ZoomIn => tab.zoom.zoom_in(),
                            keyboard::BrowserShortcut::ZoomOut => tab.zoom.zoom_out(),
                            _ => tab.zoom.reset(),
                        }
                        {
                            let mut chrome = view.doc.inner_mut();
                            tab.zoom.apply_to_chrome(&mut chrome);
                        }
                    }
                    save_zoom_settings = true;
                    view.request_redraw();
                    true
                }
                keyboard::BrowserShortcut::Find
                | keyboard::BrowserShortcut::FindClose
                | keyboard::BrowserShortcut::FindNext
                | keyboard::BrowserShortcut::FindPrev => {
                    {
                        let mut chrome = view.doc.inner_mut();
                        if let Some(tab) = self.tabs.active_mut() {
                            match shortcut {
                                keyboard::BrowserShortcut::Find => tab.find.open_bar(&mut chrome),
                                keyboard::BrowserShortcut::FindClose => {
                                    tab.find.close_bar(&mut chrome)
                                }
                                keyboard::BrowserShortcut::FindNext => {
                                    tab.find.step(&mut chrome, true)
                                }
                                _ => tab.find.step(&mut chrome, false),
                            }
                        }
                    }
                    view.request_redraw();
                    true
                }
                keyboard::BrowserShortcut::ToggleDevTools => {
                    {
                        let mut chrome = view.doc.inner_mut();
                        self.devtools.toggle(&mut chrome);
                    }
                    save_devtools_settings = true;
                    view.request_redraw();
                    true
                }
                keyboard::BrowserShortcut::DevToolsClose => {
                    {
                        let mut chrome = view.doc.inner_mut();
                        self.devtools.close(&mut chrome);
                    }
                    view.request_redraw();
                    true
                }
                keyboard::BrowserShortcut::DevToolsConsoleSubmit => {
                    {
                        let mut chrome = view.doc.inner_mut();
                        let source = crate::chrome::devtools_console_input_value(&chrome)
                            .unwrap_or_default();
                        let ts = self.js.now_ms();
                        let _ = crate::devtools::repl::submit_repl_expression(
                            &mut self.tabs,
                            &mut self.js,
                            source,
                            ts,
                        );
                        crate::chrome::clear_devtools_console_input(&mut chrome);
                        self.devtools.repl_ui.reset_after_submit();
                        let _ = self.js.pump(&mut chrome, std::time::Instant::now());
                        let wire = self.js.drain_console_wire();
                        let _ = apply_console_wire(&mut self.tabs, &wire);
                        sync_active_tab_console(&mut self.devtools, &self.tabs, &mut chrome);
                    }
                    view.request_redraw();
                    true
                }
                keyboard::BrowserShortcut::DevToolsConsoleHistoryPrev
                | keyboard::BrowserShortcut::DevToolsConsoleHistoryNext => {
                    let older = matches!(
                        shortcut,
                        keyboard::BrowserShortcut::DevToolsConsoleHistoryPrev
                    );
                    {
                        let mut chrome = view.doc.inner_mut();
                        if let Some(tab) = self.tabs.active() {
                            let _ = crate::devtools::repl::apply_history_to_input(
                                &mut chrome,
                                &tab.console_repl_history,
                                &mut self.devtools.repl_ui,
                                older,
                            );
                        }
                    }
                    view.request_redraw();
                    true
                }
                other => {
                    if let Some(command) = keyboard::nav_from_shortcut(other) {
                        self.bus.push(command);
                        true
                    } else {
                        false
                    }
                }
            }
        };
        if save_zoom_settings || save_devtools_settings {
            let _ = self.save_settings();
        }
        handled
    }
}

fn load_iframe(doc: &mut BaseDocument, url: &str) {
    let Some(id) = doc.get_element_by_id("page") else {
        return;
    };
    doc.mutate().set_attribute(id, qual_name!("src"), url);
}

fn set_js_indicator(doc: &mut BaseDocument, on: bool) {
    let Some(id) = doc.get_element_by_id("js-toggle") else {
        return;
    };
    let label = if on { "JS on" } else { "JS off" };
    let mut m = doc.mutate();
    let children: Vec<_> = m.child_ids(id).into_iter().collect();
    for child in children {
        m.remove_and_drop_node(child);
    }
    let text_id = m.create_text_node(label);
    m.append_children(id, &[text_id]);
}

fn page_url(doc: &BaseDocument) -> Option<String> {
    let id = doc.get_element_by_id("page")?;
    let url = doc.get_node(id)?.subdoc()?.inner().url().to_string();
    if url.starts_with("data:") || url.is_empty() {
        None
    } else {
        Some(url)
    }
}

fn page_title(doc: &BaseDocument) -> Option<String> {
    let id = doc.get_element_by_id("page")?;
    let title = doc
        .get_node(id)?
        .subdoc()?
        .inner()
        .find_title_node()
        .map(|node| node.text_content())
        .unwrap_or_default();
    Some(title.trim().to_string())
}

impl ApplicationHandler for Glide {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.can_create_surfaces(event_loop);
        self.sync();
    }

    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.destroy_surfaces(event_loop);
    }

    fn resumed(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.resumed(event_loop);
    }

    fn suspended(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.suspended(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let chrome_shortcut = if let WindowEvent::KeyboardInput { event, .. } = &event {
            self.inner.windows.get_mut(&window_id).and_then(|view| {
                let find_open = crate::chrome::find_bar_open(&view.doc.inner());
                let devtools_open = crate::chrome::devtools_open(&view.doc.inner());
                let mods = view.keyboard_modifiers.state();
                let find_shortcut = {
                    let mut chrome = view.doc.inner_mut();
                    keyboard::dispatch_find_field_keys(&mut chrome, event, mods, find_open)
                };
                let console_tab_active = self.devtools.state.tab == DevToolsTab::Console;
                find_shortcut
                    .or_else(|| {
                        keyboard::dispatch_devtools_console_keys(
                            &view.doc.inner(),
                            event,
                            devtools_open,
                            console_tab_active,
                        )
                    })
                    .or_else(|| {
                        keyboard::dispatch_devtools_field_keys(
                            &view.doc.inner(),
                            event,
                            devtools_open,
                        )
                    })
            })
        } else {
            None
        };
        if let Some(shortcut) = chrome_shortcut {
            self.handle_browser_shortcut_value(event_loop, window_id, shortcut);
            return;
        }

        let devtools_pointer = matches!(
            event,
            WindowEvent::PointerButton { .. } | WindowEvent::PointerMoved { .. }
        );
        if devtools_pointer {
            let outcome = self.inner.windows.get_mut(&window_id).map(|view| {
                handle_devtools_pointer(&mut self.devtools, &mut self.devtools_drag, view, &event)
            });
            match outcome {
                Some(DevToolsPointerOutcome::Consumed) => return,
                Some(DevToolsPointerOutcome::ConsumedSaveSettings) => {
                    let _ = self.save_settings();
                    return;
                }
                Some(DevToolsPointerOutcome::NotConsumed) | None => {}
            }
        }

        if self.devtools_drag.is_some()
            && matches!(
                event,
                WindowEvent::PointerButton { .. } | WindowEvent::PointerMoved { .. }
            )
        {
            return;
        }

        if self.handle_browser_shortcut(event_loop, window_id, &event) {
            self.inner
                .proxy
                .send_event(BlitzShellEvent::Poll { window_id });
            self.sync();
            return;
        }

        if let WindowEvent::KeyboardInput { event, .. } = &event
            && let Some(view) = self.inner.windows.get_mut(&window_id)
            && keyboard::dispatch_text_keys(view, event)
        {
            view.request_redraw();
            self.inner
                .proxy
                .send_event(BlitzShellEvent::Poll { window_id });
            self.sync();
            return;
        }

        self.inner.window_event(event_loop, window_id, event);
        self.sync();
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.inner.proxy_wake_up(event_loop);
        self.sync();
    }

    #[cfg(target_os = "macos")]
    fn macos_handler(
        &mut self,
    ) -> Option<&mut dyn winit::platform::macos::ApplicationHandlerExtMacOS> {
        Some(&mut self.inner)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use blitz_net::Provider;
    use blitz_traits::navigation::{NavigationOptions, NavigationProvider};
    use blitz_traits::net::NetWaker;

    use super::*;
    use url::Url;

    #[test]
    fn titles_changed_detects_a_page_title_update() {
        let mut tabs = TabManager::new();
        tabs.add("https://a/");
        let before = vec![String::new()];
        assert!(!titles_changed(&tabs, &before));
        tabs.active_mut().expect("active tab").title = "Example".to_string();
        assert!(titles_changed(&tabs, &before));
    }

    #[test]
    fn chrome_commands_ignored_from_other_documents() {
        assert!(!chrome_accepts_navigation(1, 5));
        assert!(chrome_accepts_navigation(5, 5));
        assert!(!chrome_accepts_navigation(5, 0));
        let cmd = command_from_url(&Url::parse("glide://back").unwrap());
        assert_eq!(cmd, Some(NavCommand::Back));
    }

    #[test]
    fn initial_iframe_load_uses_glide_net_error_page() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();

        let load_state = Arc::new(LoadState::default());
        let record = Arc::new(Mutex::new(None));
        let record2 = record.clone();
        let waker: Arc<dyn NetWaker> = Arc::new(move |_| {
            *record2.lock().expect("lock") = Some(());
        });
        let inner = Arc::new(Provider::new(Some(waker.clone())));
        let net = GlideNet::new(
            inner,
            load_state.clone(),
            waker,
            Arc::new(|| crate::profile::bookmarks_page_html(&[])),
        );

        let html_provider = GlideHtmlProvider {
            scripting_enabled: Arc::new(AtomicBool::new(false)),
            js_tap: JsTap::new_disabled(),
        };
        let missing = std::path::Path::new("/tmp/glide-chrome-start-missing-page.html");
        let start = url::Url::from_file_path(missing)
            .expect("file url")
            .to_string();

        let chrome_config = DocumentConfig {
            base_url: Some(CHROME_BASE.to_string()),
            net_provider: Some(net),
            navigation_provider: Some(Arc::new(DummyNav)),
            html_parser_provider: Some(Arc::new(html_provider)),
            ..Default::default()
        };

        let view = ChromeViewState {
            url: start.clone(),
            js_enabled: false,
            bookmarked: false,
            tabs: vec![TabView {
                id: 0,
                title: String::new(),
                loading: false,
            }],
            active_index: Some(0),
            open_menu: None,
            ..ChromeViewState::default()
        };
        let mut doc = build_chrome_page_document(&view, false, &load_state, chrome_config);
        assert_ne!(load_state.chrome_doc_id(), 0);

        // The error page reaches the iframe as a document event that a later
        // `resolve()` applies, so wait for the title, not just the network state.
        let mut title = String::new();
        for _ in 0..300 {
            doc.0.resolve(0.0);
            title = page_title_of(&doc.0);
            if load_state.page_loads_started() >= 1
                && record.lock().expect("lock").is_some()
                && !load_state.page_loading()
                && !title.is_empty()
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(
            load_state.page_loads_started() >= 1,
            "chrome doc id must be set before parsing so the first load is a page load"
        );
        assert!(record.lock().expect("lock").is_some());
        assert!(!load_state.page_loading());

        assert!(
            title.contains("Can't open page"),
            "expected error page title, got {:?}",
            title
        );
    }

    struct DummyNav;

    impl NavigationProvider for DummyNav {
        fn navigate_to(&self, _options: NavigationOptions) {}
    }

    fn page_title_of(chrome: &blitz_dom::BaseDocument) -> String {
        let Some(page_id) = chrome.get_element_by_id("page") else {
            return String::new();
        };
        chrome
            .get_node(page_id)
            .and_then(|n| n.subdoc())
            .and_then(|sub| {
                sub.inner()
                    .find_title_node()
                    .map(|node| node.text_content())
            })
            .unwrap_or_default()
    }
}
