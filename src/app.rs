//! Windowed browser: chrome shell, history, keyboard shortcuts, and JS pump.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use blitz_dom::{BaseDocument, DocumentConfig, qual_name};
use blitz_net::Provider;
use blitz_shell::{
    BlitzApplication, BlitzShellEvent, BlitzShellProxy, WindowConfig, create_default_event_loop,
};
use blitz_traits::navigation::{NavigationOptions, NavigationProvider};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{WindowAttributes, WindowId};

use std::sync::atomic::Ordering;

use crate::chrome::{
    NavCommand, chrome_document, command_from_url, set_address, set_loading_ui,
    sync_committed_address,
};
use crate::html::{GlideHtmlProvider, HtmlPageDocument, set_js_runtime_mode};
use crate::js::{JsHost, JsTap};
use crate::keyboard;
use crate::location::resolve_location;
use crate::nav::{NavKind, NavTracker};
use crate::net::{GlideNet, LoadState};
use crate::parse;

const CHROME_BASE: &str = "https://glide.invalid/chrome";

type Renderer = anyrender_vello_cpu::VelloCpuWindowRenderer;

pub fn run_window(start: String, js_enabled: bool) -> Result<(), String> {
    let event_loop = create_default_event_loop();
    let (proxy, receiver) = BlitzShellProxy::new(event_loop.create_proxy());
    let load_state = Arc::new(LoadState::default());
    let net_impl = Arc::new(Provider::new(Some(Arc::new(proxy.clone()))));
    let waker: Arc<dyn blitz_traits::net::NetWaker> = Arc::new(proxy.clone());
    let net = GlideNet::new(net_impl.clone(), load_state.clone(), waker);
    let bus = NavBus::default();

    let js_tap = if js_enabled {
        JsTap::new_enabled()
    } else {
        JsTap::new_disabled()
    };
    let html_provider = GlideHtmlProvider {
        scripting_enabled: Arc::new(AtomicBool::new(js_enabled)),
        js_tap: js_tap.clone(),
    };
    let js_host = JsHost::new(js_enabled, js_tap, Some(net_impl));

    let chrome_config = DocumentConfig {
        base_url: Some(CHROME_BASE.to_string()),
        net_provider: Some(net.clone()),
        navigation_provider: Some(Arc::new(ChromeNav {
            bus: bus.clone(),
            proxy: proxy.clone(),
        })),
        html_parser_provider: Some(Arc::new(html_provider.clone())),
        ..Default::default()
    };

    let doc = build_chrome_page_document(&start, js_enabled, &load_state, chrome_config);

    let attrs = WindowAttributes::default()
        .with_title("Glide")
        .with_surface_size(winit::dpi::LogicalSize::new(1100.0, 760.0));
    let window = WindowConfig::with_attributes(Box::new(doc), Renderer::new(), attrs);

    let mut app = Glide::new(
        proxy,
        receiver,
        bus,
        start,
        js_host,
        html_provider,
        load_state,
    );
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
}

impl NavigationProvider for ChromeNav {
    fn navigate_to(&self, options: NavigationOptions) {
        if let Some(command) = command_from_url(&options.url) {
            self.bus.push(command);
            self.proxy.wake_up();
        }
    }
}

struct Glide {
    inner: BlitzApplication<Renderer>,
    bus: NavBus,
    js: JsHost,
    html_provider: GlideHtmlProvider,
    nav: NavTracker,
    last_title: String,
    load_state: Arc<LoadState>,
    chrome_loading: bool,
    address_written: String,
}

fn build_chrome_page_document(
    start: &str,
    js_enabled: bool,
    load_state: &LoadState,
    chrome_config: DocumentConfig,
) -> HtmlPageDocument {
    HtmlPageDocument(parse::document_from_html_with(
        &chrome_document(start, js_enabled),
        chrome_config,
        js_enabled,
        |doc| load_state.set_chrome_doc_id(doc.id()),
    ))
}

impl Glide {
    fn new(
        proxy: BlitzShellProxy,
        receiver: std::sync::mpsc::Receiver<BlitzShellEvent>,
        bus: NavBus,
        start: String,
        js: JsHost,
        html_provider: GlideHtmlProvider,
        load_state: Arc<LoadState>,
    ) -> Self {
        Self {
            inner: BlitzApplication::new(proxy, receiver),
            bus,
            js,
            html_provider,
            nav: NavTracker::new(start.clone()),
            last_title: "Glide".to_string(),
            load_state,
            chrome_loading: false,
            address_written: start,
        }
    }

    fn sync(&mut self) {
        let Some(view) = self.inner.windows.values_mut().next() else {
            return;
        };
        let mut changed = false;
        {
            let mut doc = view.doc.inner_mut();
            for command in self.bus.take() {
                changed = true;
                match command {
                    NavCommand::Back => {
                        if let Some(url) = self.nav.navigate(NavKind::Back, None) {
                            self.js.on_navigation(&doc, &url);
                            load_iframe(&mut doc, &url);
                            set_address(&mut doc, &url);
                            self.address_written = self.nav.shown().to_string();
                        }
                    }
                    NavCommand::Forward => {
                        if let Some(url) = self.nav.navigate(NavKind::Forward, None) {
                            self.js.on_navigation(&doc, &url);
                            load_iframe(&mut doc, &url);
                            set_address(&mut doc, &url);
                            self.address_written = self.nav.shown().to_string();
                        }
                    }
                    NavCommand::Reload => {
                        if let Some(url) = self.nav.navigate(NavKind::Reload, None) {
                            self.js.on_navigation(&doc, &url);
                            load_iframe(&mut doc, &url);
                            set_address(&mut doc, &url);
                            self.address_written = self.nav.shown().to_string();
                        }
                    }
                    NavCommand::Stop => {
                        self.load_state.stop_page_load();
                        if let Some(url) = page_url(&doc) {
                            self.nav.stop(&url);
                            set_address(&mut doc, &url);
                            self.address_written = self.nav.shown().to_string();
                        }
                    }
                    NavCommand::ToggleJs => {
                        let new = !self.html_provider.scripting_enabled.load(Ordering::Relaxed);
                        set_js_runtime_mode(&self.html_provider, &mut self.js, &doc, new);
                        if let Some(url) = self.nav.navigate(NavKind::Reload, None) {
                            load_iframe(&mut doc, &url);
                            set_js_indicator(&mut doc, new);
                        }
                    }
                    NavCommand::Go(raw) => {
                        let url = resolve_location(&raw);
                        self.js.on_navigation(&doc, &url);
                        load_iframe(&mut doc, &url);
                        if self.nav.navigate(NavKind::Go, Some(url.clone())).is_some() {
                            set_address(&mut doc, &url);
                            self.address_written = self.nav.shown().to_string();
                        }
                    }
                }
            }

            if let Some(actual) = page_url(&doc) {
                let _ = self.nav.observe(&actual);
            }
            sync_committed_address(&mut doc, self.nav.shown(), &mut self.address_written);
            let loading = self.load_state.page_loading();
            if loading != self.chrome_loading {
                self.chrome_loading = loading;
                set_loading_ui(&mut doc, loading);
                changed = true;
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
        }
        if changed {
            view.request_redraw();
        }

        let page_url_now = {
            let chrome = view.doc.inner();
            page_url(&chrome)
        };
        if let Some(url) = page_url_now {
            let handle = tokio::runtime::Handle::current();
            {
                let chrome = view.doc.inner();
                self.js.maybe_start_page(&chrome, &url, &handle);
            }
            let js_redraw = {
                let mut chrome = view.doc.inner_mut();
                self.js.pump(&mut chrome, Instant::now())
            };
            if js_redraw {
                view.request_redraw();
            }
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
        if !keyboard::consume_stop_shortcut(&shortcut, self.load_state.page_loading())
            && matches!(shortcut, keyboard::BrowserShortcut::Stop)
        {
            return false;
        }

        match shortcut {
            keyboard::BrowserShortcut::FocusAddressBar => {
                keyboard::focus_address_bar(view);
                view.request_redraw();
                true
            }
            keyboard::BrowserShortcut::CloseWindow => {
                if let Some(win) = self.inner.windows.remove(&window_id) {
                    drop(win);
                }
                if self.inner.windows.is_empty() {
                    event_loop.exit();
                }
                true
            }
            keyboard::BrowserShortcut::Quit => {
                self.inner.windows.clear();
                event_loop.exit();
                true
            }
            keyboard::BrowserShortcut::Scroll(action) => {
                keyboard::scroll_page(view, action);
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
        let net = GlideNet::new(inner, load_state.clone(), waker);

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

        let mut doc = build_chrome_page_document(&start, false, &load_state, chrome_config);
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
