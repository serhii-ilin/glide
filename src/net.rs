//! Glide network layer: error pages for failed top-level loads, loading state, and stop.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use blitz_net::{Provider, ProviderError};
use blitz_traits::net::{
    AbortController, AbortSignal, Bytes, NetHandler, NetProvider, NetWaker, Request,
};
use tokio::sync::Notify;
use url::Url;

use crate::devtools::network::NetworkInitiator;
use crate::network_feed::{self, NetworkCompleted, NetworkRoute, NetworkStamp};

/// Tracks in-flight page and subresource loads for chrome UI and stop.
#[derive(Default)]
pub struct LoadState {
    chrome_doc_id: AtomicUsize,
    document_url: std::sync::Mutex<Option<url::Url>>,
    page_in_flight: AtomicU32,
    subresource_in_flight: AtomicU32,
    load_generation: AtomicU64,
    page_cancel: Mutex<Option<ActivePageLoad>>,
    page_loads_started: AtomicUsize,
    page_load_failed: Mutex<HashSet<String>>,
    #[cfg(test)]
    pub(crate) test_page_delay_ms: AtomicUsize,
}

struct ActivePageLoad {
    generation: u64,
    controller: AbortController,
    notify: Arc<Notify>,
}

struct PageLoadToken {
    generation: u64,
    glide_signal: AbortSignal,
    notify: Arc<Notify>,
}

impl LoadState {
    pub fn set_chrome_doc_id(&self, id: usize) {
        self.chrome_doc_id.store(id, Ordering::Release);
    }

    pub fn chrome_doc_id(&self) -> usize {
        self.chrome_doc_id.load(Ordering::Acquire)
    }

    pub fn page_loading(&self) -> bool {
        self.page_in_flight.load(Ordering::Relaxed) > 0
    }

    pub fn page_loads_started(&self) -> usize {
        self.page_loads_started.load(Ordering::Relaxed)
    }

    pub fn mark_page_load_failed(&self, url: &str) {
        let mut failed = lock_failed(&self.page_load_failed);
        failed.insert(url.to_string());
    }

    pub fn mark_page_load_succeeded(&self, url: &str) {
        let mut failed = lock_failed(&self.page_load_failed);
        failed.remove(url);
    }

    pub fn page_load_failed(&self, url: &str) -> bool {
        let failed = lock_failed(&self.page_load_failed);
        failed.contains(url)
    }

    pub fn failed_page_urls(&self) -> HashSet<String> {
        lock_failed(&self.page_load_failed).clone()
    }

    pub fn load_generation(&self) -> u64 {
        self.load_generation.load(Ordering::Relaxed)
    }

    pub fn stop_page_load(&self) {
        let mut guard = lock_cancel(&self.page_cancel);
        if let Some(active) = guard.take() {
            active.controller.abort();
            active.notify.notify_waiters();
        }
    }

    fn begin_page_load(&self) -> PageLoadToken {
        let generation = self.load_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let notify = Arc::new(Notify::new());
        let controller = AbortController::default();
        let glide_signal = controller.signal.clone();

        let mut guard = lock_cancel(&self.page_cancel);
        if let Some(mut prev) = guard.take() {
            let controller = std::mem::take(&mut prev.controller);
            controller.abort();
            prev.notify.notify_waiters();
        }
        *guard = Some(ActivePageLoad {
            generation,
            controller,
            notify: notify.clone(),
        });
        self.page_in_flight.fetch_add(1, Ordering::Relaxed);
        self.page_loads_started.fetch_add(1, Ordering::Relaxed);

        PageLoadToken {
            generation,
            glide_signal,
            notify,
        }
    }

    fn finish_page_load(&self, generation: u64) {
        self.page_in_flight.fetch_sub(1, Ordering::Relaxed);
        let mut guard = lock_cancel(&self.page_cancel);
        if guard
            .as_ref()
            .is_some_and(|active| active.generation == generation)
        {
            guard.take();
        }
    }

    /// Count in-flight subresource fetches so we can wake the shell when the last one ends.
    fn subresource_started(&self) {
        self.subresource_in_flight.fetch_add(1, Ordering::Relaxed);
    }

    fn subresource_finished(&self) {
        self.subresource_in_flight.fetch_sub(1, Ordering::Relaxed);
    }
}

fn lock_cancel(m: &Mutex<Option<ActivePageLoad>>) -> MutexGuard<'_, Option<ActivePageLoad>> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn lock_failed(m: &Mutex<HashSet<String>>) -> MutexGuard<'_, HashSet<String>> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Wraps [`Provider`] so top-level (chrome `doc_id`) loads show an HTML error page on failure.
pub struct GlideNet {
    inner: Arc<Provider>,
    state: Arc<LoadState>,
    waker: Arc<dyn NetWaker>,
    bookmarks: Arc<dyn Fn() -> String + Send + Sync>,
    /// Live scripting mode, shared with the HTML provider. It decides whether a
    /// `<noscript>` meta refresh may redirect the first navigation.
    scripting_enabled: Arc<AtomicBool>,
    network_route: Arc<NetworkRoute>,
}

fn record_network_complete(stamp: NetworkStamp, started: Instant, completed: NetworkCompleted) {
    network_feed::publish_completed(
        stamp,
        NetworkCompleted {
            duration_ms: started.elapsed().as_millis() as u64,
            ..completed
        },
    );
}

struct PageLoadSuccess {
    url: String,
    bytes: Bytes,
    status: u16,
    headers: Vec<(String, String)>,
}

impl GlideNet {
    /// Provider with scripting treated as enabled, for tests and callers that do
    /// not care about the toggle.
    pub fn new(
        inner: Arc<Provider>,
        state: Arc<LoadState>,
        waker: Arc<dyn NetWaker>,
        bookmarks: Arc<dyn Fn() -> String + Send + Sync>,
    ) -> Arc<Self> {
        Self::with_scripting(
            inner,
            state,
            waker,
            bookmarks,
            Arc::new(AtomicBool::new(true)),
            Arc::new(NetworkRoute::default()),
        )
    }

    /// Provider that follows the live scripting mode for navigation decisions.
    pub fn with_scripting(
        inner: Arc<Provider>,
        state: Arc<LoadState>,
        waker: Arc<dyn NetWaker>,
        bookmarks: Arc<dyn Fn() -> String + Send + Sync>,
        scripting_enabled: Arc<AtomicBool>,
        network_route: Arc<NetworkRoute>,
    ) -> Arc<Self> {
        Arc::new(Self {
            inner,
            state,
            waker,
            bookmarks,
            scripting_enabled,
            network_route,
        })
    }

    pub fn network_route(&self) -> Arc<NetworkRoute> {
        self.network_route.clone()
    }

    fn deliver_internal_page(
        &self,
        doc_id: usize,
        resolved_url: String,
        html: String,
        handler: Box<dyn NetHandler>,
        network_error: Option<String>,
    ) {
        let token = self.state.begin_page_load();
        let state = self.state.clone();
        let waker = self.waker.clone();
        let generation = token.generation;
        let stamp = self.network_route.begin_document_load();
        let started = Instant::now();
        let (status, error) = match &network_error {
            Some(reason) => (None, Some(reason.clone())),
            None => (Some(200), None),
        };
        tokio::spawn(async move {
            let _guard = PageLoadGuard {
                state: &state,
                generation,
                doc_id,
                waker: &waker,
            };
            record_network_complete(
                stamp,
                started,
                NetworkCompleted {
                    method: "GET".to_string(),
                    url: resolved_url.clone(),
                    initiator: NetworkInitiator::Document,
                    status,
                    error,
                    size_bytes: Some(html.len() as u64),
                    response_headers: Vec::new(),
                    duration_ms: 0,
                },
            );
            handler.bytes(resolved_url, Bytes::from(html));
        });
    }

    fn try_internal_glide_page(&self, url: &Url) -> Option<String> {
        if url.scheme() != "glide" {
            return None;
        }
        match url.host_str() {
            Some("bookmarks") => Some((self.bookmarks)()),
            _ => None,
        }
    }

    fn fetch_page(&self, doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        if request.url.scheme() == "about" {
            let resolved = request.url.to_string();
            if let Some(html) = crate::about::about_document(&request.url) {
                self.deliver_internal_page(doc_id, resolved, html.to_string(), handler, None);
                return;
            }
            let html = error_page_html(&resolved, crate::about::ABOUT_LOCAL_NETWORK_ERROR);
            self.deliver_internal_page(
                doc_id,
                resolved,
                html,
                handler,
                Some(crate::about::ABOUT_LOCAL_NETWORK_ERROR.to_string()),
            );
            return;
        }

        if request.url.scheme() == "glide" {
            if let Some(html) = self.try_internal_glide_page(&request.url) {
                let resolved = request.url.to_string();
                self.deliver_internal_page(doc_id, resolved, html, handler, None);
                return;
            }
            let token = self.state.begin_page_load();
            let state = self.state.clone();
            let waker = self.waker.clone();
            let load_gen = token.generation;
            tokio::spawn(async move {
                let _guard = PageLoadGuard {
                    state: &state,
                    generation: load_gen,
                    doc_id,
                    waker: &waker,
                };
                drop(handler);
            });
            return;
        }

        *self.state.document_url.lock().unwrap() = Some(request.url.clone());
        let original_url = request.url.to_string();
        let blitz_abort = request.signal.clone();
        let token = self.state.begin_page_load();
        let inner = self.inner.clone();
        let waker = self.waker.clone();
        let state = self.state.clone();
        let generation = token.generation;
        let glide_signal = token.glide_signal;
        let cancel_notify = token.notify;
        let scripting_enabled = self.scripting_enabled.load(Ordering::Relaxed);
        let network_route = self.network_route.clone();
        let stamp = network_route.begin_document_load();
        let method = request.method.to_string();
        let started = Instant::now();
        #[cfg(test)]
        let test_delay_ms = state.test_page_delay_ms.load(Ordering::Relaxed);
        #[cfg(not(test))]
        let test_delay_ms = 0;

        tokio::spawn(async move {
            let _guard = PageLoadGuard {
                state: &state,
                generation,
                doc_id,
                waker: &waker,
            };

            let result = fetch_racing_aborts(
                &inner,
                request,
                blitz_abort.as_ref(),
                &glide_signal,
                &cancel_notify,
                test_delay_ms,
                scripting_enabled,
            )
            .await;

            match result {
                Ok(page) => {
                    state.mark_page_load_succeeded(&original_url);
                    record_network_complete(
                        stamp,
                        started,
                        NetworkCompleted {
                            method,
                            url: page.url.clone(),
                            initiator: NetworkInitiator::Document,
                            status: Some(page.status),
                            error: None,
                            size_bytes: Some(page.bytes.len() as u64),
                            response_headers: page.headers,
                            duration_ms: 0,
                        },
                    );
                    handler.bytes(page.url, page.bytes)
                }
                Err(LoadFail::Abort) => drop(handler),
                Err(LoadFail::Failed(reason)) => {
                    state.mark_page_load_failed(&original_url);
                    record_network_complete(
                        stamp,
                        started,
                        NetworkCompleted {
                            method,
                            url: original_url.clone(),
                            initiator: NetworkInitiator::Document,
                            status: None,
                            error: Some(reason.clone()),
                            size_bytes: None,
                            response_headers: Vec::new(),
                            duration_ms: 0,
                        },
                    );
                    let html = error_page_html(&original_url, &reason);
                    handler.bytes(original_url, Bytes::from(html));
                }
            }
        });
    }

    fn fetch_http_subresource(
        &self,
        doc_id: usize,
        request: Request,
        handler: Box<dyn NetHandler>,
    ) {
        self.state.subresource_started();
        let state = self.state.clone();
        let waker = self.waker.clone();
        let stamp = self.network_route.capture_stamp();
        let method = request.method.to_string();
        let url = request.url.to_string();
        let started = Instant::now();
        let wrapped = Box::new(SubresourceHandler {
            state,
            inner: Some(handler),
            waker,
            doc_id,
            stamp,
            method,
            url,
            started,
            initiator: NetworkInitiator::Subresource,
        });
        let document_url = self.state.document_url.lock().ok().and_then(|g| g.clone());
        tokio::spawn(async move {
            match crate::http::fetch_followed_with_cookie_context(
                request,
                document_url,
                crate::cookies::CookieRequestKind::Subresource,
                crate::cookies::FetchCredentials::Include,
            )
            .await
            {
                Ok(resp) if (200..300).contains(&resp.status) => {
                    wrapped.deliver_http_success(resp);
                }
                Ok(resp) => {
                    wrapped.record_failure(Some(resp.status), None, resp.headers);
                    drop(wrapped);
                }
                Err(err) => {
                    wrapped.record_failure(None, Some(err), Vec::new());
                    drop(wrapped);
                }
            }
        });
    }
}

struct PageLoadGuard<'a> {
    state: &'a LoadState,
    generation: u64,
    doc_id: usize,
    waker: &'a Arc<dyn NetWaker>,
}

impl Drop for PageLoadGuard<'_> {
    fn drop(&mut self) {
        self.state.finish_page_load(self.generation);
        self.waker.wake(self.doc_id);
    }
}

impl NetProvider for GlideNet {
    fn fetch(&self, doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let chrome_id = self.state.chrome_doc_id();
        if chrome_id != 0 && doc_id == chrome_id {
            self.fetch_page(doc_id, request, handler);
            return;
        }

        if request.url.scheme() == "glide" {
            drop(handler);
            return;
        }

        if request.url.scheme() == "about" {
            if let Some(html) = crate::about::about_document(&request.url) {
                handler.bytes(request.url.to_string(), Bytes::from(html));
                return;
            }
            drop(handler);
            return;
        }

        if crate::http::is_navigable_http(&request) {
            self.fetch_http_subresource(doc_id, request, handler);
            return;
        }

        self.state.subresource_started();
        let state = self.state.clone();
        let waker = self.waker.clone();
        let stamp = self.network_route.capture_stamp();
        let method = request.method.to_string();
        let url = request.url.to_string();
        let started = Instant::now();
        let wrapped = Box::new(SubresourceHandler {
            state,
            inner: Some(handler),
            waker,
            doc_id,
            stamp,
            method,
            url,
            started,
            initiator: NetworkInitiator::Subresource,
        });
        self.inner.fetch(doc_id, request, wrapped);
    }
}

struct SubresourceHandler {
    state: Arc<LoadState>,
    inner: Option<Box<dyn NetHandler>>,
    waker: Arc<dyn NetWaker>,
    doc_id: usize,
    stamp: NetworkStamp,
    method: String,
    url: String,
    started: Instant,
    initiator: NetworkInitiator,
}

impl SubresourceHandler {
    fn deliver_http_success(mut self, resp: crate::http::Followed) {
        if self.inner.is_some() {
            self.finish_counting();
        }
        record_network_complete(
            self.stamp,
            self.started,
            NetworkCompleted {
                method: self.method.clone(),
                url: resp.url.clone(),
                initiator: self.initiator,
                status: Some(resp.status),
                error: None,
                size_bytes: Some(resp.body.len() as u64),
                response_headers: resp.headers,
                duration_ms: 0,
            },
        );
        if let Some(inner) = self.inner.take() {
            inner.bytes(resp.url, resp.body);
        }
    }

    fn record_success(
        &self,
        url: String,
        size: usize,
        status: u16,
        headers: Vec<(String, String)>,
    ) {
        record_network_complete(
            self.stamp,
            self.started,
            NetworkCompleted {
                method: self.method.clone(),
                url,
                initiator: self.initiator,
                status: Some(status),
                error: None,
                size_bytes: Some(size as u64),
                response_headers: headers,
                duration_ms: 0,
            },
        );
    }

    fn record_failure(
        &self,
        status: Option<u16>,
        error: Option<String>,
        headers: Vec<(String, String)>,
    ) {
        record_network_complete(
            self.stamp,
            self.started,
            NetworkCompleted {
                method: self.method.clone(),
                url: self.url.clone(),
                initiator: self.initiator,
                status,
                error,
                size_bytes: None,
                response_headers: headers,
                duration_ms: 0,
            },
        );
    }
}

impl SubresourceHandler {
    fn finish_counting(&self) {
        self.state.subresource_finished();
        if self.state.subresource_in_flight.load(Ordering::Relaxed) == 0 {
            self.waker.wake(self.doc_id);
        }
    }
}

impl NetHandler for SubresourceHandler {
    fn bytes(mut self: Box<Self>, resolved_url: String, bytes: Bytes) {
        if self.inner.is_some() {
            self.finish_counting();
        }
        self.record_success(resolved_url.clone(), bytes.len(), 200, Vec::new());
        if let Some(inner) = self.inner.take() {
            inner.bytes(resolved_url, bytes);
        }
    }
}

impl Drop for SubresourceHandler {
    fn drop(&mut self) {
        if self.inner.is_some() {
            self.finish_counting();
        }
    }
}

enum LoadFail {
    Abort,
    Failed(String),
}

async fn fetch_racing_aborts(
    provider: &Provider,
    request: Request,
    blitz: Option<&AbortSignal>,
    glide_signal: &AbortSignal,
    cancel_notify: &Notify,
    test_delay_ms: usize,
    scripting_enabled: bool,
) -> Result<PageLoadSuccess, LoadFail> {
    let fetch_fut = page_fetch(provider, request, test_delay_ms, scripting_enabled);
    tokio::pin!(fetch_fut);
    tokio::select! {
        res = &mut fetch_fut => res,
        () = wait_for_cancel(blitz, glide_signal, cancel_notify) => Err(LoadFail::Abort),
    }
}

async fn page_fetch(
    provider: &Provider,
    request: Request,
    test_delay_ms: usize,
    scripting_enabled: bool,
) -> Result<PageLoadSuccess, LoadFail> {
    if test_delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(test_delay_ms as u64)).await;
    }
    if matches!(request.url.scheme(), "http" | "https")
        && !matches!(request.body, blitz_traits::net::Body::Form(_))
    {
        if crate::http::is_navigable_http(&request) {
            return match crate::http::fetch_document(request, scripting_enabled).await {
                crate::http::DocumentFetch::Page {
                    url,
                    body,
                    status,
                    headers,
                } => Ok(PageLoadSuccess {
                    url,
                    bytes: body,
                    status,
                    headers,
                }),
                crate::http::DocumentFetch::Failed { reason, .. } => Err(LoadFail::Failed(reason)),
            };
        }
        return match crate::http::fetch_followed(request).await {
            Ok(resp) => Ok(PageLoadSuccess {
                url: resp.url,
                bytes: resp.body,
                status: resp.status,
                headers: resp.headers,
            }),
            Err(reason) => Err(LoadFail::Failed(reason)),
        };
    }
    match provider.fetch_async(request).await {
        Ok((url, bytes)) => Ok(PageLoadSuccess {
            url,
            bytes,
            status: 200,
            headers: Vec::new(),
        }),
        Err(ProviderError::Abort) => Err(LoadFail::Abort),
        Err(err) => Err(LoadFail::Failed(provider_error_reason(&err))),
    }
}

async fn wait_for_cancel(
    blitz: Option<&AbortSignal>,
    glide_signal: &AbortSignal,
    cancel_notify: &Notify,
) {
    let mut interval = tokio::time::interval(Duration::from_millis(50));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            () = cancel_notify.notified() => return,
            _ = interval.tick() => {
                if blitz.is_some_and(AbortSignal::aborted) || glide_signal.aborted() {
                    return;
                }
            }
        }
    }
}

/// Human-readable failure text for an error page.
pub fn provider_error_reason(err: &ProviderError) -> String {
    match err {
        ProviderError::HttpStatus { status, .. } => {
            let phrase = status.canonical_reason().unwrap_or("Error");
            format!("Server answered {} {}", status.as_u16(), phrase)
        }
        ProviderError::Io(io) if io.kind() == std::io::ErrorKind::NotFound => {
            "File not found".to_string()
        }
        ProviderError::Io(_) => "Couldn't read file".to_string(),
        ProviderError::ReqwestError(_) => "Couldn't reach host".to_string(),
        ProviderError::DataUrl(_) | ProviderError::DataUrlBase64(_) => {
            "Invalid data URL".to_string()
        }
        ProviderError::Abort => "Request stopped".to_string(),
    }
}

/// Minimal HTML error document with escaped URL/reason and a Retry link.
pub fn error_page_html(url: &str, reason: &str) -> String {
    let url_esc = escape_html(url);
    let reason_esc = escape_html(reason);
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Can&apos;t open page</title>
<style>
  body {{ font-family: system-ui, sans-serif; margin: 2rem; color: #1c1915; }}
  h1 {{ font-size: 1.25rem; }}
  .url {{ word-break: break-all; color: #555; }}
  a {{ color: #2563eb; }}
</style>
</head>
<body>
<h1>Can&apos;t open page</h1>
<p class="url">{url_esc}</p>
<p>{reason_esc}</p>
<p><a href="{url_esc}">Retry</a></p>
</body>
</html>
"#
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

    use blitz_traits::net::Request;
    use url::Url;

    use super::*;

    struct RecordHandler {
        done: Arc<StdMutex<Option<(String, Bytes)>>>,
    }

    impl NetHandler for RecordHandler {
        fn bytes(self: Box<Self>, resolved_url: String, bytes: Bytes) {
            *self.done.lock().expect("record") = Some((resolved_url, bytes));
        }
    }

    fn file_url(path: &str) -> Url {
        Url::from_file_path(path).expect("file url")
    }

    #[test]
    fn error_page_escapes_url_and_reason_and_has_retry() {
        let url = "https://evil.example/<script>";
        let reason = "Oops<script>";
        let html = error_page_html(url, reason);
        assert!(html.contains("https://evil.example/&lt;script&gt;"));
        assert!(html.contains("Oops&lt;script&gt;"));
        assert!(html.contains(r#"<a href="https://evil.example/&lt;script&gt;">Retry</a>"#));
    }

    #[test]
    fn maps_http_status_to_reason() {
        let status = blitz_traits::net::http::StatusCode::from_u16(404).expect("404");
        let err = ProviderError::HttpStatus {
            status,
            url: "https://example.com/missing".into(),
        };
        assert_eq!(provider_error_reason(&err), "Server answered 404 Not Found");
    }

    #[test]
    fn maps_file_not_found_io() {
        let err = ProviderError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"));
        assert_eq!(provider_error_reason(&err), "File not found");
    }

    #[test]
    fn maps_data_url_error() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();
        let provider = Provider::new(None);
        let url = Url::parse("data:text/html;base64,%%%").expect("data url");
        let err = rt
            .block_on(provider.fetch_async(Request::get(url)))
            .expect_err("invalid data");
        assert_eq!(provider_error_reason(&err), "Invalid data URL");
    }

    #[test]
    fn maps_other_io_to_read_error() {
        let err = ProviderError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, "bad"));
        assert_eq!(provider_error_reason(&err), "Couldn't read file");
    }

    #[test]
    fn glide_net_missing_file_shows_error_page_and_clears_loading() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();

        let wake_count = Arc::new(AtomicUsize::new(0));
        let waker: Arc<dyn NetWaker> = Arc::new({
            let wake_count = wake_count.clone();
            move |_| {
                wake_count.fetch_add(1, Ordering::Relaxed);
            }
        });

        let inner = Arc::new(Provider::new(Some(waker.clone())));
        let state = Arc::new(LoadState::default());
        let chrome_id = 42;
        state.set_chrome_doc_id(chrome_id);
        let net = GlideNet::new(inner, state.clone(), waker, test_bookmarks_supplier());

        let record = Arc::new(StdMutex::new(None));
        let handler = Box::new(RecordHandler {
            done: record.clone(),
        });
        let missing = file_url("/tmp/glide-net-missing-no-such-file-xyz.html");
        net.fetch(chrome_id, Request::get(missing), handler);

        for _ in 0..200 {
            if record.lock().expect("record").is_some() && !state.page_loading() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let body = record
            .lock()
            .expect("record")
            .as_ref()
            .expect("handler should receive bytes")
            .1
            .clone();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("File not found"));
        assert!(!state.page_loading());
        assert!(wake_count.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn about_blank_delivers_empty_document() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();

        let waker: Arc<dyn NetWaker> = Arc::new(|_| {});
        let inner = Arc::new(Provider::new(Some(waker.clone())));
        let state = Arc::new(LoadState::default());
        state.set_chrome_doc_id(7);
        let route = Arc::new(NetworkRoute::default());
        route.set(3, 1);
        let net = GlideNet::with_scripting(
            inner,
            state.clone(),
            waker,
            test_bookmarks_supplier(),
            Arc::new(AtomicBool::new(true)),
            route,
        );

        let _ = crate::network_feed::drain_pending();

        let record = Arc::new(StdMutex::new(None));
        let handler = Box::new(RecordHandler {
            done: record.clone(),
        });
        let url = Url::parse("about:blank?x=1#frag").expect("about url");
        net.fetch(7, Request::get(url), handler);

        for _ in 0..200 {
            if record.lock().expect("record").is_some() && !state.page_loading() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let (resolved, body) = record
            .lock()
            .expect("record")
            .take()
            .expect("handler should receive bytes");
        assert_eq!(resolved, "about:blank?x=1#frag");
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("<body></body>"));
        assert!(!text.contains("Can&apos;t open page"));
        assert!(!state.page_loading());

        let events = crate::network_feed::drain_pending();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].url, "about:blank?x=1#frag");
        assert_eq!(events[0].status, Some(200));
        assert_eq!(events[0].initiator, NetworkInitiator::Document);
    }

    #[test]
    fn unsupported_about_urls_serve_local_error_page_without_provider() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();

        let waker: Arc<dyn NetWaker> = Arc::new(|_| {});
        let inner = Arc::new(Provider::new(Some(waker.clone())));
        let state = Arc::new(LoadState::default());
        state.set_chrome_doc_id(8);
        let net = GlideNet::new(inner, state.clone(), waker, test_bookmarks_supplier());

        let cases = ["about:src", "about:blank/", "about://blank", "ABOUT:BLANK"];
        for case in cases {
            let record = Arc::new(StdMutex::new(None));
            let handler = Box::new(RecordHandler {
                done: record.clone(),
            });
            let url = Url::parse(case).expect("about url");
            net.fetch(8, Request::get(url), handler);

            for _ in 0..200 {
                if record.lock().expect("record").is_some() && !state.page_loading() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }

            let (resolved, body) = record
                .lock()
                .expect("record")
                .take()
                .expect("handler should receive bytes");
            let expected = Url::parse(case).expect("about url").to_string();
            assert_eq!(resolved, expected, "resolved url");
            let text = String::from_utf8_lossy(&body);
            assert!(text.contains("Can&apos;t open page"), "{case}");
            assert!(
                text.contains(crate::about::ABOUT_LOCAL_NETWORK_ERROR),
                "{case}"
            );
            assert!(!text.contains("<body></body>"), "{case}");
        }
    }

    #[test]
    fn glide_scheme_page_load_delivers_nothing() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();

        let waker: Arc<dyn NetWaker> = Arc::new(|_| {});
        let inner = Arc::new(Provider::new(Some(waker.clone())));
        let state = Arc::new(LoadState::default());
        state.set_chrome_doc_id(3);
        let net = GlideNet::new(inner, state.clone(), waker, test_bookmarks_supplier());

        let record = Arc::new(StdMutex::new(None));
        let handler = Box::new(RecordHandler {
            done: record.clone(),
        });
        let url = Url::parse("glide://reload").expect("glide url");
        net.fetch(3, Request::get(url), handler);

        for _ in 0..200 {
            if !state.page_loading() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(record.lock().expect("record").is_none());
        assert!(!state.page_loading());
    }

    #[test]
    fn glide_net_existing_fixture_returns_bytes() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();

        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/input.html");
        assert!(path.is_file());

        let waker: Arc<dyn NetWaker> = Arc::new(|_| {});
        let inner = Arc::new(Provider::new(Some(waker.clone())));
        let state = Arc::new(LoadState::default());
        let chrome_id = 7;
        state.set_chrome_doc_id(chrome_id);
        let net = GlideNet::new(inner, state, waker, test_bookmarks_supplier());

        let record = Arc::new(StdMutex::new(None));
        let handler = Box::new(RecordHandler {
            done: record.clone(),
        });
        net.fetch(
            chrome_id,
            Request::get(file_url(path.to_str().expect("utf8"))),
            handler,
        );

        for _ in 0..200 {
            if record.lock().expect("record").is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let bytes = record.lock().expect("record").clone().expect("bytes").1;
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("input"));
    }

    #[test]
    fn network_log_records_subresource() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/network-subresources.css");
        assert!(path.is_file(), "network-subresources.css fixture");

        let waker: Arc<dyn NetWaker> = Arc::new(|_| {});
        let inner = Arc::new(Provider::new(Some(waker.clone())));
        let state = Arc::new(LoadState::default());
        state.set_chrome_doc_id(1);
        let route = Arc::new(NetworkRoute::default());
        route.set(9, 2);
        let net = GlideNet::with_scripting(
            inner,
            state.clone(),
            waker,
            test_bookmarks_supplier(),
            Arc::new(AtomicBool::new(true)),
            route,
        );

        let _ = crate::network_feed::drain_pending();

        let record = Arc::new(StdMutex::new(None));
        let handler = Box::new(RecordHandler {
            done: record.clone(),
        });
        net.fetch(
            99,
            Request::get(file_url(path.to_str().expect("utf8"))),
            handler,
        );

        for _ in 0..200 {
            if record.lock().expect("record").is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(record.lock().expect("record").is_some());

        let events = crate::network_feed::drain_pending();
        assert_eq!(events.len(), 1);
        let ev = &events[0];
        assert_eq!(ev.tab_id, 9);
        assert_eq!(ev.network_epoch, 2);
        assert_eq!(ev.initiator, NetworkInitiator::Subresource);
        assert_eq!(ev.status, Some(200));
        assert!(ev.url.contains("network-subresources.css"));
        assert!(ev.size_bytes.unwrap_or(0) > 0);
        assert!(ev.started_ms > 0);
    }

    #[test]
    fn http_subresource_records_status_and_headers() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let port = addr.port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = b"nf";
            let response = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nX-Test: a&b\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).expect("write");
            stream.write_all(body).expect("body");
        });

        let waker: Arc<dyn NetWaker> = Arc::new(|_| {});
        let inner = Arc::new(Provider::new(Some(waker.clone())));
        let state = Arc::new(LoadState::default());
        state.set_chrome_doc_id(1);
        let route = Arc::new(NetworkRoute::default());
        route.set(1, 1);
        let net = GlideNet::with_scripting(
            inner,
            state,
            waker,
            test_bookmarks_supplier(),
            Arc::new(AtomicBool::new(true)),
            route,
        );
        let _ = crate::network_feed::drain_pending();
        let record = Arc::new(StdMutex::new(None));
        let url = Url::parse(&format!("http://127.0.0.1:{port}/missing.css")).expect("url");
        net.fetch(
            2,
            Request::get(url),
            Box::new(RecordHandler {
                done: record.clone(),
            }),
        );
        let mut events = Vec::new();
        for _ in 0..200 {
            events = crate::network_feed::drain_pending();
            if !events.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            events.iter().any(|e| e.status == Some(404)),
            "events: {events:?}"
        );
        assert!(
            events.iter().any(|e| {
                e.response_headers
                    .iter()
                    .any(|(k, v)| k == "x-test" && v == "a&b")
            }),
            "events: {events:?}"
        );
        server.join().expect("server");
    }

    #[test]
    fn http_subresource_no_content_records_204() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 512];
            let _ = stream.read(&mut buf);
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nX-Empty: 1\r\n\r\n")
                .expect("write");
        });
        let waker: Arc<dyn NetWaker> = Arc::new(|_| {});
        let inner = Arc::new(Provider::new(Some(waker.clone())));
        let state = Arc::new(LoadState::default());
        state.set_chrome_doc_id(1);
        let route = Arc::new(NetworkRoute::default());
        route.set(1, 1);
        let net = GlideNet::with_scripting(
            inner,
            state,
            waker,
            test_bookmarks_supplier(),
            Arc::new(AtomicBool::new(true)),
            route,
        );
        let _ = crate::network_feed::drain_pending();
        let url = Url::parse(&format!("http://127.0.0.1:{port}/empty.css")).expect("url");
        net.fetch(
            2,
            Request::get(url),
            Box::new(RecordHandler {
                done: Arc::new(StdMutex::new(None)),
            }),
        );
        for _ in 0..300 {
            let batch = crate::network_feed::drain_pending();
            if batch.iter().any(|e| e.status == Some(204)) {
                assert!(
                    batch
                        .iter()
                        .any(|e| { e.response_headers.iter().any(|(k, _)| k == "x-empty") })
                );
                server.join().expect("server");
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for 204 record");
    }

    #[test]
    fn glide_net_subresource_failure_delivers_nothing() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();

        let waker: Arc<dyn NetWaker> = Arc::new(|_| {});
        let inner = Arc::new(Provider::new(Some(waker.clone())));
        let state = Arc::new(LoadState::default());
        state.set_chrome_doc_id(1);
        let net = GlideNet::new(inner, state.clone(), waker, test_bookmarks_supplier());

        let record = Arc::new(StdMutex::new(None));
        let handler = Box::new(RecordHandler {
            done: record.clone(),
        });
        let missing = file_url("/tmp/glide-subresource-missing-zzz.css");
        net.fetch(99, Request::get(missing), handler);

        for _ in 0..200 {
            if !state.page_loading() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(record.lock().expect("record").is_none());
    }

    #[test]
    fn stop_cancels_delayed_page_load_without_bytes() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();

        let waker: Arc<dyn NetWaker> = Arc::new(|_| {});
        let inner = Arc::new(Provider::new(Some(waker.clone())));
        let state = Arc::new(LoadState::default());
        state.set_chrome_doc_id(5);
        state.test_page_delay_ms.store(30_000, Ordering::Relaxed);
        let net = GlideNet::new(inner, state.clone(), waker, test_bookmarks_supplier());

        let record = Arc::new(StdMutex::new(None));
        let handler = Box::new(RecordHandler {
            done: record.clone(),
        });
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/input.html");
        net.fetch(
            5,
            Request::get(file_url(path.to_str().expect("utf8"))),
            handler,
        );

        for _ in 0..100 {
            if state.page_loading() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(state.page_loading());

        state.stop_page_load();

        for _ in 0..200 {
            if !state.page_loading() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(!state.page_loading());
        assert!(record.lock().expect("record").is_none());
    }

    #[test]
    fn finishing_old_load_keeps_new_load_cancellable() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();

        let waker: Arc<dyn NetWaker> = Arc::new(|_| {});
        let inner = Arc::new(Provider::new(Some(waker.clone())));
        let state = Arc::new(LoadState::default());
        state.set_chrome_doc_id(8);
        state.test_page_delay_ms.store(5_000, Ordering::Relaxed);
        let net = GlideNet::new(inner, state.clone(), waker, test_bookmarks_supplier());

        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/input.html");
        let req = || Request::get(file_url(path.to_str().expect("utf8")));

        net.fetch(
            8,
            req(),
            Box::new(RecordHandler {
                done: Arc::new(StdMutex::new(None)),
            }),
        );
        std::thread::sleep(Duration::from_millis(50));

        net.fetch(
            8,
            req(),
            Box::new(RecordHandler {
                done: Arc::new(StdMutex::new(None)),
            }),
        );

        std::thread::sleep(Duration::from_millis(100));
        assert!(state.page_loading());

        state.stop_page_load();

        for _ in 0..200 {
            if !state.page_loading() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!state.page_loading());
    }

    fn test_bookmarks_supplier() -> Arc<dyn Fn() -> String + Send + Sync> {
        Arc::new(|| crate::profile::bookmarks_page_html(&[]))
    }

    #[test]
    fn provider_with_cookies_feature_builds() {
        let _provider = Provider::new(None);
    }

    #[test]
    fn glide_bookmarks_page_delivers_html() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();

        let waker: Arc<dyn NetWaker> = Arc::new(|_| {});
        let inner = Arc::new(Provider::new(Some(waker.clone())));
        let state = Arc::new(LoadState::default());
        state.set_chrome_doc_id(9);
        let net = GlideNet::new(
            inner,
            state.clone(),
            waker,
            Arc::new(|| "<html><title>Bookmarks</title></html>".to_string()),
        );

        let record = Arc::new(StdMutex::new(None));
        let handler = Box::new(RecordHandler {
            done: record.clone(),
        });
        let url = Url::parse("glide://bookmarks").expect("url");
        net.fetch(9, Request::get(url), handler);

        for _ in 0..200 {
            if record.lock().expect("record").is_some() && !state.page_loading() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let body = record
            .lock()
            .expect("record")
            .as_ref()
            .expect("body")
            .1
            .clone();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("Bookmarks"));
    }

    #[test]
    fn glide_bookmarks_subresource_is_silent() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();

        let waker: Arc<dyn NetWaker> = Arc::new(|_| {});
        let inner = Arc::new(Provider::new(Some(waker.clone())));
        let state = Arc::new(LoadState::default());
        state.set_chrome_doc_id(9);
        let gen_before = state.load_generation();
        let net = GlideNet::new(inner, state.clone(), waker, test_bookmarks_supplier());

        let record = Arc::new(StdMutex::new(None));
        let handler = Box::new(RecordHandler {
            done: record.clone(),
        });
        let url = Url::parse("glide://bookmarks").expect("url");
        net.fetch(99, Request::get(url), handler);

        assert!(record.lock().expect("record").is_none());
        assert!(!state.page_loading());
        assert_eq!(state.load_generation(), gen_before);
    }
}
