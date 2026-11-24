//! Headless page load: fetch, parse, settle network and JavaScript (no window).

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::{Duration, Instant};

use blitz_dom::node::NodeData;
use blitz_dom::{BaseDocument, DocumentConfig, local_name};
use blitz_net::Provider;
use blitz_traits::navigation::NavigationOptions;
use blitz_traits::net::http::HeaderValue;
use blitz_traits::net::{NetProvider, Request};
use blitz_traits::node_id::NodeId;
use blitz_traits::shell::{ColorScheme, Viewport};

use crate::html::{GlideHtmlProvider, HtmlPageDocument};
use crate::js::{JsHost, JsTap};
use crate::logging;
use crate::parse;

pub const VIEWPORT_WIDTH: u32 = 1100;
pub const VIEWPORT_HEIGHT: u32 = 760;

/// Limits for the headless settle loop (network quiet, JS quiet, maximum wait).
#[derive(Debug, Clone)]
pub struct SettleOptions {
    pub min_js: Duration,
    pub max: Duration,
    pub quiet_ticks: u32,
}

impl Default for SettleOptions {
    fn default() -> Self {
        Self {
            min_js: Duration::from_secs(1),
            max: Duration::from_secs(12),
            quiet_ticks: 20,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettleOutcome {
    pub timed_out: bool,
}

/// Fetch document bytes for a URL without building a DOM.
#[cfg(test)]
pub(crate) static PROVIDER_FETCH_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

pub fn fetch_page(
    url: &str,
    js_enabled: bool,
    handle: &tokio::runtime::Handle,
    net: &Arc<Provider>,
) -> Result<(String, Vec<u8>), String> {
    let parsed = url::Url::parse(url).map_err(|err| err.to_string())?;
    fetch_page_with_request(Request::get(parsed), js_enabled, handle, net)
}

pub fn fetch_page_with_request(
    mut request: Request,
    js_enabled: bool,
    handle: &tokio::runtime::Handle,
    net: &Arc<Provider>,
) -> Result<(String, Vec<u8>), String> {
    request.url.set_fragment(None);
    let parsed = request.url.clone();
    let url = parsed.to_string();
    if parsed.scheme() == "about" {
        if let Some(html) = crate::about::about_document(&parsed) {
            return Ok((parsed.to_string(), html.as_bytes().to_vec()));
        }
        let html = crate::net::error_page_html(&url, crate::about::ABOUT_LOCAL_NETWORK_ERROR);
        return Ok((parsed.to_string(), html.into_bytes()));
    }
    if parsed.scheme() == "file" {
        let path = parsed
            .to_file_path()
            .map_err(|_| format!("invalid file URL `{url}`"))?;
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let mut canonical = parsed;
        canonical.set_fragment(None);
        return Ok((canonical.to_string(), bytes));
    }
    if parsed.scheme() == "http" || parsed.scheme() == "https" {
        match handle.block_on(crate::http::fetch_document(
            request,
            js_enabled,
            crate::http::DocumentNavigationContext::default(),
        )) {
            crate::http::DocumentFetch::Page { url, body, .. } => Ok((url, body.to_vec())),
            crate::http::DocumentFetch::Failed { url, reason } => {
                Err(format!("fetch {url}: {reason}"))
            }
        }
    } else {
        #[cfg(test)]
        PROVIDER_FETCH_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        handle
            .block_on(net.fetch_async(request))
            .map(|(final_url, bytes)| (final_url, bytes.into()))
            .map_err(|err| format!("fetch {url}: {err}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleBreak {
    Settled,
    TimedOut,
}

/// Pure settle-loop exit condition (unit-tested).
pub fn settle_decision(
    net_done: bool,
    js_done: bool,
    elapsed: Duration,
    min_js: Duration,
    max: Duration,
) -> Option<SettleBreak> {
    let min_js = min_js.min(max);
    if net_done && js_done && elapsed >= min_js {
        return Some(SettleBreak::Settled);
    }
    if elapsed > max {
        return Some(SettleBreak::TimedOut);
    }
    None
}

/// Pump layout, network, and JS until the page quiets or `opts.max` elapses.
pub fn settle<F>(
    doc: &mut BaseDocument,
    js_host: &mut JsHost,
    net: &Provider,
    js_enabled: bool,
    opts: SettleOptions,
    mut on_tick: F,
) -> SettleOutcome
where
    F: FnMut(),
{
    let start = Instant::now();
    let min_js = if js_enabled {
        opts.min_js.min(opts.max)
    } else {
        Duration::ZERO
    };
    let mut js_quiet_ticks = 0_u32;
    loop {
        doc.resolve(0.0);
        let js_redraw = js_host.pump(doc, Instant::now());
        if js_redraw {
            doc.resolve(0.0);
            js_quiet_ticks = 0;
        } else if js_enabled {
            js_quiet_ticks = js_quiet_ticks.saturating_add(1);
        }
        let elapsed = start.elapsed();
        let net_done = net.is_empty();
        let js_done = !js_enabled || js_quiet_ticks > opts.quiet_ticks;
        match settle_decision(net_done, js_done, elapsed, min_js, opts.max) {
            Some(SettleBreak::TimedOut) => {
                doc.resolve(0.0);
                return SettleOutcome { timed_out: true };
            }
            Some(SettleBreak::Settled) => {
                doc.resolve(0.0);
                return SettleOutcome { timed_out: false };
            }
            None => {}
        }
        on_tick();
        thread::sleep(Duration::from_millis(30));
    }
}

/// Upper bound on page-initiated navigations followed during one headless load.
pub const MAX_JS_NAVIGATIONS: u32 = 5;

/// Records page-initiated navigations (`location.assign`, form submission).
///
/// A headless document has no iframe and therefore no navigation provider of
/// its own; this one keeps the target so the loader can follow it, mirroring
/// what the window does when the iframe reports a navigation.
#[derive(Default)]
pub struct JsNavigation {
    pending: std::sync::Mutex<Option<Request>>,
}

impl JsNavigation {
    /// Take the last requested navigation, if any.
    pub fn take(&self) -> Option<Request> {
        self.pending.lock().ok().and_then(|mut req| req.take())
    }
}

impl blitz_traits::navigation::NavigationProvider for JsNavigation {
    fn navigate_to(&self, options: NavigationOptions) {
        let mut headers = blitz_traits::net::http::HeaderMap::new();
        if let Some(ct) = options.content_type.as_deref()
            && let Ok(value) = HeaderValue::from_str(ct)
        {
            headers.insert(blitz_traits::net::http::header::CONTENT_TYPE, value);
        }
        let mut request = Request::get(options.url);
        request.method = options.method;
        request.content_type = options.content_type;
        request.headers = headers;
        request.body = options.document_resource;
        if let Ok(mut pending) = self.pending.lock() {
            *pending = Some(request);
        }
    }
}

/// Load a page headlessly and wait until subresources and scripts (when enabled) settle.
///
/// Follows navigations the page starts itself (a JavaScript challenge that
/// submits a form, `location.assign`) for up to [`MAX_JS_NAVIGATIONS`] hops.
pub fn load(url: &str, js_enabled: bool) -> Result<HtmlPageDocument, String> {
    let mut target = url.to_string();
    let mut next_request: Option<Request> = None;
    let mut hops = 0;
    loop {
        let (document, pending) = load_once(&target, js_enabled, next_request.take())?;
        match pending {
            Some(req) if hops < MAX_JS_NAVIGATIONS => {
                hops += 1;
                target = req.url.to_string();
                next_request = Some(req);
            }
            _ => return Ok(document),
        }
    }
}

/// Parse `html` into a document wired to `net`, with scripting on or off.
fn page_document(
    html: &str,
    final_url: &str,
    net_impl: &Arc<Provider>,
    navigation: &Arc<JsNavigation>,
    scripting: bool,
) -> (HtmlPageDocument, JsTap) {
    let net_provider: Arc<dyn NetProvider> = net_impl.clone();
    let tap = if scripting {
        JsTap::new_enabled()
    } else {
        JsTap::new_disabled()
    };
    let html_provider = GlideHtmlProvider {
        scripting_enabled: Arc::new(AtomicBool::new(scripting)),
        js_tap: tap.clone(),
    };
    let config = DocumentConfig {
        base_url: Some(final_url.to_string()),
        net_provider: Some(net_provider),
        navigation_provider: Some(navigation.clone()),
        html_parser_provider: Some(Arc::new(html_provider)),
        viewport: Some(Viewport::new(
            VIEWPORT_WIDTH,
            VIEWPORT_HEIGHT,
            1.0,
            ColorScheme::Light,
        )),
        ..Default::default()
    };
    (
        HtmlPageDocument(parse::document_from_html(html, config, scripting)),
        tap,
    )
}

/// Re-render `html` with scripting off after the scripted render came out blank.
///
/// Returns the static document only when it shows something, so a page that is
/// blank either way keeps its scripted document and its blank-page record.
pub fn static_fallback(
    html: &str,
    final_url: &str,
    net_impl: &Arc<Provider>,
    navigation: &Arc<JsNavigation>,
) -> Option<(HtmlPageDocument, JsHost)> {
    let (mut document, tap) = page_document(html, final_url, net_impl, navigation, false);
    let mut js_host = JsHost::new(false, tap, Some(net_impl.clone()));
    let handle = tokio::runtime::Handle::current();
    js_host.maybe_start_page(&document.0, final_url, &handle);
    let _outcome = settle(
        &mut document.0,
        &mut js_host,
        net_impl,
        false,
        SettleOptions::default(),
        || {},
    );
    document.0.resolve(0.0);
    has_visible_content(&document.0).then_some((document, js_host))
}

/// Record that a blank scripted render was replaced by the static document (F-087).
pub fn log_static_fallback(final_url: &str) {
    logging::warn(
        "js.page",
        "scripted render was blank; showing static content",
        &[
            ("page_url", final_url.to_string()),
            ("status", "fallback".into()),
            (
                "hint",
                "Showing the server-rendered HTML parsed with scripting off (noscript content included); the page's own scripts failed to render it (F-087)."
                    .into(),
            ),
        ],
    );
}

fn load_once(
    url: &str,
    js_enabled: bool,
    navigation_request: Option<Request>,
) -> Result<(HtmlPageDocument, Option<Request>), String> {
    crate::cookies::CookieJar::ensure_process_global(None);
    crate::web_storage::WebStorage::ensure_process_global(None);
    let net_impl = Arc::new(Provider::new(None));
    let handle = tokio::runtime::Handle::current();
    let (final_url, bytes) = match navigation_request {
        Some(request) => fetch_page_with_request(request, js_enabled, &handle, &net_impl)?,
        None => fetch_page(url, js_enabled, &handle, &net_impl)?,
    };
    let html = String::from_utf8_lossy(&bytes);

    let navigation = Arc::new(JsNavigation::default());
    let (mut document, tap) = page_document(&html, &final_url, &net_impl, &navigation, js_enabled);
    let mut js_host = JsHost::new(js_enabled, tap, Some(net_impl.clone()));

    js_host.set_console_route(0, 1);
    js_host.maybe_start_page(&document.0, &final_url, &handle);
    let _outcome = settle(
        &mut document.0,
        &mut js_host,
        &net_impl,
        js_enabled,
        SettleOptions::default(),
        || {},
    );
    let _ = crate::js::console_feed::drain_pending();

    if js_enabled {
        let _ = js_host.drain_console_wire();
        js_host.flush_dom_rpc_until_idle(
            &mut document.0,
            Duration::from_secs(4),
            Duration::from_millis(50),
        );
        document.0.resolve(0.0);
        if !html.trim().is_empty() && !has_visible_content(&document.0) {
            logging::warn(
                "js.page",
                "page rendered no visible text",
                &[
                    ("page_url", final_url.clone()),
                    (
                        "hint",
                        "Scripts ran without errors; the app may depend on a capability listed by js.caps (GLIDE_LOG=debug) or on an unimplemented navigation API."
                            .into(),
                    ),
                ],
            );
            if let Some((fallback, _)) = static_fallback(&html, &final_url, &net_impl, &navigation)
            {
                log_static_fallback(&final_url);
                document = fallback;
            }
        }
    }

    Ok((document, navigation.take()))
}

/// Whether the laid-out page shows any text or replaced content.
///
/// Hidden subtrees (`display:none`) have no layout box, so a shell whose only text
/// sits in a hidden fallback counts as blank. Used to report script apps that take
/// a silent fallback path instead of rendering.
pub fn has_visible_content(doc: &BaseDocument) -> bool {
    let Some(body) = doc.find_body_node() else {
        return false;
    };
    let size = body.final_layout().size;
    let laid_out = size.width > 0.0 && size.height > 0.0;
    body.children
        .iter()
        .any(|child| visible_in_subtree(doc, *child, laid_out))
}

/// `parent_laid_out` carries the nearest box ancestor: text nodes have no layout
/// data of their own, and hidden subtrees (`display:none`) lay out to zero size.
fn visible_in_subtree(doc: &BaseDocument, id: NodeId, parent_laid_out: bool) -> bool {
    let Some(node) = doc.get_node(id) else {
        return false;
    };
    match &node.data {
        NodeData::Text(data) => parent_laid_out && !data.content.trim().is_empty(),
        NodeData::Element(..) | NodeData::AnonymousBlock(..) => {
            let size = node.final_layout().size;
            let laid_out = size.width > 0.0 && size.height > 0.0;
            let replaced = node.data.is_element_with_tag_name(&local_name!("img"))
                || node.data.is_element_with_tag_name(&local_name!("svg"))
                || node.data.is_element_with_tag_name(&local_name!("canvas"))
                || node.data.is_element_with_tag_name(&local_name!("video"))
                || node.data.is_element_with_tag_name(&local_name!("iframe"));
            (laid_out && replaced)
                || node
                    .children
                    .iter()
                    .any(|child| visible_in_subtree(doc, *child, laid_out))
        }
        _ => false,
    }
}

fn has_element_child(doc: &BaseDocument, node: &blitz_dom::Node) -> bool {
    node.children
        .iter()
        .any(|id| doc.get_node(*id).is_some_and(|child| child.is_element()))
}

/// Collapse whitespace in each line, drop blank lines, and append a trailing newline.
pub fn format_dump_text(doc: &BaseDocument, scripting_enabled: bool) -> String {
    let raw = body_text_content(doc, scripting_enabled);
    let lines: Vec<String> = raw
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return "\n".to_string();
    }
    format!("{}\n", lines.join("\n"))
}

fn body_text_content(doc: &BaseDocument, scripting_enabled: bool) -> String {
    let Some(body) = doc.find_body_node() else {
        return String::new();
    };
    let mut out = String::new();
    append_visible_text(doc, body.id, scripting_enabled, &mut out);
    out
}

fn push_line_break(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

fn is_block_element(node: &blitz_dom::node::Node) -> bool {
    node.data.is_element_with_tag_name(&local_name!("p"))
        || node.data.is_element_with_tag_name(&local_name!("div"))
        || node.data.is_element_with_tag_name(&local_name!("li"))
        || node.data.is_element_with_tag_name(&local_name!("ul"))
        || node.data.is_element_with_tag_name(&local_name!("ol"))
        || node.data.is_element_with_tag_name(&local_name!("h1"))
        || node.data.is_element_with_tag_name(&local_name!("h2"))
        || node.data.is_element_with_tag_name(&local_name!("h3"))
        || node.data.is_element_with_tag_name(&local_name!("h4"))
        || node.data.is_element_with_tag_name(&local_name!("h5"))
        || node.data.is_element_with_tag_name(&local_name!("h6"))
        || node.data.is_element_with_tag_name(&local_name!("section"))
        || node.data.is_element_with_tag_name(&local_name!("article"))
        || node.data.is_element_with_tag_name(&local_name!("header"))
        || node.data.is_element_with_tag_name(&local_name!("footer"))
        || node.data.is_element_with_tag_name(&local_name!("nav"))
        || node.data.is_element_with_tag_name(&local_name!("main"))
        || node.data.is_element_with_tag_name(&local_name!("table"))
        || node.data.is_element_with_tag_name(&local_name!("tr"))
        || node.data.is_element_with_tag_name(&local_name!("pre"))
        || node
            .data
            .is_element_with_tag_name(&local_name!("blockquote"))
}

fn append_visible_text(
    doc: &BaseDocument,
    node: NodeId,
    scripting_enabled: bool,
    out: &mut String,
) {
    let Some(node_ref) = doc.get_node(node) else {
        return;
    };
    match &node_ref.data {
        NodeData::Text(data) => out.push_str(&data.content),
        NodeData::Element(..) => {
            if node_ref
                .data
                .is_element_with_tag_name(&local_name!("script"))
                || node_ref
                    .data
                    .is_element_with_tag_name(&local_name!("style"))
            {
                return;
            }
            // A scripting-on parse keeps `<noscript>` content as raw text; a
            // scripting-off parse (the F-087 static fallback) yields elements, which show.
            if scripting_enabled
                && node_ref
                    .data
                    .is_element_with_tag_name(&local_name!("noscript"))
                && !has_element_child(doc, node_ref)
            {
                return;
            }
            if node_ref.data.is_element_with_tag_name(&local_name!("br")) {
                push_line_break(out);
                return;
            }
            let block = is_block_element(node_ref);
            if block {
                push_line_break(out);
            }
            for child in node_ref.children.iter() {
                append_visible_text(doc, *child, scripting_enabled, out);
            }
            if block {
                push_line_break(out);
            }
        }
        NodeData::AnonymousBlock(..) => {
            for child in node_ref.children.iter() {
                append_visible_text(doc, *child, scripting_enabled, out);
            }
        }
        _ => {}
    }
}

pub fn dump_text(url: &str, js_enabled: bool) -> Result<String, String> {
    let doc = load(url, js_enabled)?;
    Ok(format_dump_text(&doc.0, js_enabled))
}

#[cfg(test)]
mod tests {
    use super::*;
    use blitz_dom::DocumentConfig;

    fn doc_from_html(html: &str, scripting: bool) -> BaseDocument {
        parse::document_from_html(html, DocumentConfig::default(), scripting)
    }

    fn laid_out_doc(html: &str) -> BaseDocument {
        let config = DocumentConfig {
            viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
            ..DocumentConfig::default()
        };
        let mut doc = parse::document_from_html(html, config, true);
        doc.resolve(0.0);
        doc
    }

    #[test]
    fn visible_content_ignores_display_none_text() {
        let hidden = laid_out_doc(
            r#"<!DOCTYPE html><html><body><div style="display:none">hidden</div></body></html>"#,
        );
        assert!(!has_visible_content(&hidden));

        let shown = laid_out_doc("<!DOCTYPE html><html><body><p>shown</p></body></html>");
        assert!(has_visible_content(&shown));
    }

    #[test]
    fn visible_content_counts_a_sized_image() {
        let doc = laid_out_doc(
            r#"<!DOCTYPE html><html><body><img width="10" height="10"></body></html>"#,
        );
        assert!(has_visible_content(&doc));
    }

    #[test]
    fn noscript_text_hidden_when_scripting_parse_but_shown_for_static_parse() {
        let html = "<!DOCTYPE html><html><body><noscript><p>NS-TEXT</p></noscript></body></html>";
        let scripted = doc_from_html(html, true);
        assert!(!format_dump_text(&scripted, true).contains("NS-TEXT"));
        let static_doc = doc_from_html(html, false);
        assert!(format_dump_text(&static_doc, true).contains("NS-TEXT"));
    }

    #[test]
    fn dump_lines_collapses_whitespace() {
        let doc = doc_from_html(
            "<!DOCTYPE html><html><body><p>  hello   world</p><p>foo  bar</p></body></html>",
            true,
        );
        assert_eq!(format_dump_text(&doc, true), "hello world\nfoo bar\n");
    }

    #[test]
    fn block_elements_are_separated() {
        let doc = doc_from_html(
            "<!DOCTYPE html><html><body><p>a</p><p>b</p></body></html>",
            true,
        );
        assert_eq!(format_dump_text(&doc, true), "a\nb\n");
    }

    #[test]
    fn br_breaks_lines() {
        let doc = doc_from_html(
            "<!DOCTYPE html><html><body><p>a<br>b</p></body></html>",
            true,
        );
        assert_eq!(format_dump_text(&doc, true), "a\nb\n");
    }

    #[test]
    fn fetch_about_blank_without_network() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();
        PROVIDER_FETCH_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
        let net = Arc::new(Provider::new(None));
        let (resolved, body) =
            fetch_page("about:blank", false, rt.handle(), &net).expect("about:blank loads");
        assert_eq!(resolved, "about:blank");
        let text = String::from_utf8(body).expect("utf8");
        assert!(text.contains("<body></body>"));
        assert_eq!(
            PROVIDER_FETCH_COUNT.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }

    #[test]
    fn fetch_unsupported_about_urls_local_error_without_provider() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();
        PROVIDER_FETCH_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
        let net = Arc::new(Provider::new(None));
        for url in ["about:src", "about:blank/", "about://blank", "ABOUT:BLANK"] {
            let (resolved, body) = fetch_page(url, false, rt.handle(), &net).expect("loads");
            let expected = url::Url::parse(url).expect("url").to_string();
            assert_eq!(resolved, expected);
            let text = String::from_utf8(body).expect("utf8");
            assert!(text.contains("Can&apos;t open page"), "{url}");
            assert!(!text.contains("<body></body>"), "{url}");
        }
        assert_eq!(
            PROVIDER_FETCH_COUNT.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }

    #[test]
    fn settle_decision_times_out_only_past_max() {
        let max = Duration::from_secs(12);
        assert_eq!(
            settle_decision(
                false,
                true,
                max + Duration::from_millis(1),
                Duration::ZERO,
                max
            ),
            Some(SettleBreak::TimedOut)
        );
        assert_eq!(
            settle_decision(true, true, max, Duration::ZERO, max),
            Some(SettleBreak::Settled)
        );
        let short_max = Duration::from_millis(800);
        assert_eq!(
            settle_decision(
                true,
                true,
                Duration::from_millis(830),
                Duration::from_secs(1),
                short_max
            ),
            Some(SettleBreak::Settled)
        );
    }

    #[test]
    fn fetch_page_decodes_percent_encoded_file_path() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let _enter = rt.enter();
        let dir = std::env::temp_dir().join(format!("glide-fetch-space-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let nested = dir.join("with space");
        std::fs::create_dir_all(&nested).expect("nested");
        let file = nested.join("page.html");
        std::fs::write(&file, b"<html><body>space ok</body></html>").expect("write");
        let file_url = url::Url::from_file_path(&file).expect("file url");
        let net = Arc::new(Provider::new(None));
        let (url, body) = fetch_page(file_url.as_str(), false, rt.handle(), &net).expect("fetch");
        assert!(!url.contains('#'));
        assert!(body.windows(8).any(|w| w == b"space ok"));
        let with_frag = format!("{}#frag", file_url);
        let (_, body2) = fetch_page(&with_frag, false, rt.handle(), &net).expect("fetch frag");
        assert!(body2.windows(8).any(|w| w == b"space ok"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn headless_load_drains_console_queue() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let _enter = rt.enter();
        let _lock = crate::js::console_feed::test_console_serial_lock();
        let _ = crate::js::console_feed::drain_pending();
        let url = "data:text/html,<!DOCTYPE html><html><body><script>console.log('headless-drain')</script></body></html>";
        let _doc = load(url, true).expect("headless load");
        assert!(
            crate::js::console_feed::drain_pending().is_empty(),
            "headless load should drain console wire queue"
        );
    }
}
