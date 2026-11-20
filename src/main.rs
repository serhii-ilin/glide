//! Glide, a small desktop browser.
//!
//! The window, input, and rasterizer come from Blitz (the same engine
//! Cloudflare's Kitesurf embeds in WebAssembly). See the README for why the
//! Kitesurf crates themselves are not linked here.

mod chrome;
mod keyboard;
mod location;
mod shot;

use std::sync::{Arc, Mutex};

use blitz_dom::{DocumentConfig, qual_name};
use blitz_html::{HtmlDocument, HtmlProvider};
use blitz_net::Provider;
use blitz_shell::{
    BlitzApplication, BlitzShellEvent, BlitzShellProxy, WindowConfig, create_default_event_loop,
};
use blitz_traits::navigation::{NavigationOptions, NavigationProvider};
use chrome::{NavCommand, chrome_document, command_from_url};
use location::resolve_location;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{WindowAttributes, WindowId};

const CHROME_BASE: &str = "https://glide.invalid/chrome";

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--screenshot") {
        let url = args.next().unwrap_or_else(|| "https://example.com".into());
        let path = args.next().unwrap_or_else(|| "glide-screenshot.png".into());
        let url = resolve_location(&url);
        let runtime = tokio_runtime();
        let _guard = runtime.enter();
        match shot::render_url(&url, std::path::Path::new(&path)) {
            Ok(()) => println!("wrote {path}"),
            Err(err) => {
                eprintln!("screenshot failed: {err}");
                std::process::exit(1);
            }
        }
        return;
    }

    let start = std::env::args()
        .nth(1)
        .map(|arg| resolve_location(&arg))
        .unwrap_or_else(|| "https://example.com/".to_string());

    let runtime = tokio_runtime();
    let _guard = runtime.enter();

    let event_loop = create_default_event_loop();
    let (proxy, receiver) = BlitzShellProxy::new(event_loop.create_proxy());
    let net = Provider::shared(Some(Arc::new(proxy.clone())));
    let bus = NavBus::default();

    let doc = HtmlDocument::from_html(
        &chrome_document(&start),
        DocumentConfig {
            base_url: Some(CHROME_BASE.to_string()),
            net_provider: Some(net),
            navigation_provider: Some(Arc::new(ChromeNav {
                bus: bus.clone(),
                proxy: proxy.clone(),
            })),
            html_parser_provider: Some(Arc::new(HtmlProvider)),
            ..Default::default()
        },
    );

    let attrs = WindowAttributes::default()
        .with_title("Glide")
        .with_surface_size(winit::dpi::LogicalSize::new(1100.0, 760.0));
    let window = WindowConfig::with_attributes(Box::new(doc), Renderer::new(), attrs);

    let mut app = Glide::new(proxy, receiver, bus, start);
    app.inner.add_window(window);
    if let Err(err) = event_loop.run_app(app) {
        eprintln!("glide exited: {err}");
        std::process::exit(1);
    }
}

fn tokio_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
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

struct History {
    entries: Vec<String>,
    index: usize,
}

impl History {
    fn new(url: String) -> Self {
        Self {
            entries: vec![url],
            index: 0,
        }
    }

    fn current(&self) -> String {
        self.entries[self.index].clone()
    }

    fn push(&mut self, url: String) {
        if self.entries[self.index] == url {
            self.entries.truncate(self.index + 1);
            return;
        }
        self.entries.truncate(self.index + 1);
        self.entries.push(url);
        self.index = self.entries.len() - 1;
    }

    fn back(&mut self) -> Option<String> {
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        Some(self.current())
    }

    fn forward(&mut self) -> Option<String> {
        if self.index + 1 >= self.entries.len() {
            return None;
        }
        self.index += 1;
        Some(self.current())
    }
}

type Renderer = anyrender_vello_cpu::VelloCpuWindowRenderer;

struct Glide {
    inner: BlitzApplication<Renderer>,
    bus: NavBus,
    history: History,
    /// URL we last asked the iframe to load. Observations of that load,
    /// including redirects, update the current history entry in place.
    expecting: Option<String>,
    last_seen: Option<String>,
    shown: String,
    last_title: String,
}

impl Glide {
    fn new(
        proxy: BlitzShellProxy,
        receiver: std::sync::mpsc::Receiver<BlitzShellEvent>,
        bus: NavBus,
        start: String,
    ) -> Self {
        Self {
            inner: BlitzApplication::new(proxy, receiver),
            bus,
            history: History::new(start.clone()),
            expecting: Some(start.clone()),
            last_seen: None,
            shown: start,
            last_title: "Glide".to_string(),
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
                        if let Some(url) = self.history.back() {
                            load_iframe(&mut doc, &url);
                            self.expecting = Some(url.clone());
                            self.shown.clone_from(&url);
                            set_address(&mut doc, &url);
                        }
                    }
                    NavCommand::Forward => {
                        if let Some(url) = self.history.forward() {
                            load_iframe(&mut doc, &url);
                            self.expecting = Some(url.clone());
                            self.shown.clone_from(&url);
                            set_address(&mut doc, &url);
                        }
                    }
                    NavCommand::Reload => {
                        let url = self.history.current();
                        load_iframe(&mut doc, &url);
                        self.expecting = Some(url);
                    }
                    NavCommand::Go(raw) => {
                        let url = resolve_location(&raw);
                        self.history.push(url.clone());
                        load_iframe(&mut doc, &url);
                        self.expecting = Some(url.clone());
                        self.shown.clone_from(&url);
                        set_address(&mut doc, &url);
                    }
                }
            }

            if let Some(actual) = page_url(&doc) {
                if let Some(expected) = self.expecting.clone() {
                    if actual == expected {
                        self.expecting = None;
                        self.last_seen = Some(actual);
                    } else {
                        self.history.entries[self.history.index] = actual.clone();
                        self.expecting = Some(actual.clone());
                        if actual != self.shown {
                            self.shown.clone_from(&actual);
                            set_address(&mut doc, &actual);
                        }
                        self.last_seen = Some(actual);
                    }
                } else if self.last_seen.as_ref() != Some(&actual) {
                    self.history.push(actual.clone());
                    self.last_seen = Some(actual.clone());
                    if actual != self.shown {
                        self.shown.clone_from(&actual);
                        set_address(&mut doc, &actual);
                    }
                }
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

fn load_iframe(doc: &mut blitz_dom::BaseDocument, url: &str) {
    let Some(id) = doc.get_element_by_id("page") else {
        return;
    };
    doc.mutate().set_attribute(id, qual_name!("src"), url);
}

fn set_address(doc: &mut blitz_dom::BaseDocument, url: &str) {
    let Some(id) = doc.get_element_by_id("url") else {
        return;
    };
    doc.mutate().set_attribute(id, qual_name!("value"), url);
}

fn page_url(doc: &blitz_dom::BaseDocument) -> Option<String> {
    let id = doc.get_element_by_id("page")?;
    let url = doc.get_node(id)?.subdoc()?.inner().url().to_string();
    if url.starts_with("data:") || url.is_empty() {
        None
    } else {
        Some(url)
    }
}

fn page_title(doc: &blitz_dom::BaseDocument) -> Option<String> {
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

        if let WindowEvent::KeyboardInput { event, .. } = &event {
            if let Some(view) = self.inner.windows.get_mut(&window_id) {
                if keyboard::dispatch_text_keys(view, event) {
                    view.request_redraw();
                    self.inner
                        .proxy
                        .send_event(BlitzShellEvent::Poll { window_id });
                    self.sync();
                    return;
                }
            }
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
