//! Glide network layer: error pages for failed top-level loads, loading state, and stop.

use blitz_net::{Provider, ProviderError};
use blitz_traits::net::{
    AbortController, AbortSignal, Bytes, NetHandler, NetProvider, NetWaker, Request,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

/// Tracks in-flight page and subresource loads for chrome UI and stop.
#[derive(Default)]
pub struct LoadState {
    chrome_doc_id: AtomicUsize,
    page_in_flight: AtomicU32,
    subresource_in_flight: AtomicU32,
    page_cancel: Mutex<Option<AbortController>>,
    #[cfg(test)]
    pub(crate) test_page_delay_ms: AtomicUsize,
}

impl LoadState {
    pub fn set_chrome_doc_id(&self, id: usize) {
        self.chrome_doc_id.store(id, Ordering::Release);
    }

    pub fn chrome_doc_id(&self) -> usize {
        self.chrome_doc_id.load(Ordering::Acquire)
    }

    pub fn is_loading(&self) -> bool {
        self.page_in_flight.load(Ordering::Relaxed) > 0
            || self.subresource_in_flight.load(Ordering::Relaxed) > 0
    }

    pub fn stop_page_load(&self) {
        let mut guard = lock_cancel(&self.page_cancel);
        if let Some(controller) = guard.take() {
            controller.abort();
        }
    }

    fn page_load_started(&self) -> AbortSignal {
        let mut guard = lock_cancel(&self.page_cancel);
        if let Some(controller) = guard.take() {
            controller.abort();
        }
        let controller = AbortController::default();
        let signal = controller.signal.clone();
        *guard = Some(controller);
        self.page_in_flight.fetch_add(1, Ordering::Relaxed);
        signal
    }

    fn page_load_finished(&self) {
        self.page_in_flight.fetch_sub(1, Ordering::Relaxed);
        let mut guard = lock_cancel(&self.page_cancel);
        guard.take();
    }

    fn subresource_started(&self) {
        self.subresource_in_flight.fetch_add(1, Ordering::Relaxed);
    }

    fn subresource_finished(&self) {
        self.subresource_in_flight.fetch_sub(1, Ordering::Relaxed);
    }
}

fn lock_cancel(m: &Mutex<Option<AbortController>>) -> MutexGuard<'_, Option<AbortController>> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Wraps [`Provider`] so top-level (chrome `doc_id`) loads show an HTML error page on failure.
pub struct GlideNet {
    inner: Arc<Provider>,
    state: Arc<LoadState>,
    waker: Arc<dyn NetWaker>,
}

impl GlideNet {
    pub fn new(inner: Arc<Provider>, state: Arc<LoadState>, waker: Arc<dyn NetWaker>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            state,
            waker,
        })
    }

    pub fn load_state(&self) -> &Arc<LoadState> {
        &self.state
    }

    pub fn inner_provider(&self) -> &Arc<Provider> {
        &self.inner
    }

    fn fetch_page(&self, doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let original_url = request.url.to_string();
        let blitz_abort = request.signal.clone();
        let glide_abort = self.state.page_load_started();
        let inner = self.inner.clone();
        let waker = self.waker.clone();
        let state = self.state.clone();

        #[cfg(test)]
        let test_delay_ms = state.test_page_delay_ms.load(Ordering::Relaxed);

        tokio::spawn(async move {
            #[cfg(test)]
            if test_delay_ms > 0 {
                let abort = wait_for_abort(blitz_abort.as_ref(), Some(&glide_abort));
                tokio::pin!(abort);
                tokio::select! {
                    () = tokio::time::sleep(std::time::Duration::from_millis(
                        test_delay_ms as u64,
                    )) => {}
                    () = &mut abort => {
                        waker.wake(doc_id);
                        state.page_load_finished();
                        drop(handler);
                        return;
                    }
                }
            }

            let result =
                fetch_racing_aborts(&inner, request, blitz_abort.as_ref(), Some(&glide_abort))
                    .await;

            waker.wake(doc_id);
            state.page_load_finished();

            match result {
                Ok((url, bytes)) => handler.bytes(url, bytes),
                Err(ProviderError::Abort) => drop(handler),
                Err(err) => {
                    let reason = provider_error_reason(&err);
                    let html = error_page_html(&original_url, &reason);
                    handler.bytes(original_url, Bytes::from(html));
                }
            }
        });
    }
}

impl NetProvider for GlideNet {
    fn fetch(&self, doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let chrome_id = self.state.chrome_doc_id();
        if chrome_id != 0 && doc_id == chrome_id {
            self.fetch_page(doc_id, request, handler);
            return;
        }

        self.state.subresource_started();
        let state = self.state.clone();
        let wrapped = Box::new(SubresourceHandler {
            state,
            inner: Some(handler),
        });
        self.inner.fetch(doc_id, request, wrapped);
    }
}

struct SubresourceHandler {
    state: Arc<LoadState>,
    inner: Option<Box<dyn NetHandler>>,
}

impl NetHandler for SubresourceHandler {
    fn bytes(mut self: Box<Self>, resolved_url: String, bytes: Bytes) {
        if self.inner.is_some() {
            self.state.subresource_finished();
        }
        if let Some(inner) = self.inner.take() {
            inner.bytes(resolved_url, bytes);
        }
    }
}

impl Drop for SubresourceHandler {
    fn drop(&mut self) {
        if self.inner.is_some() {
            self.state.subresource_finished();
        }
    }
}

async fn fetch_racing_aborts(
    provider: &Provider,
    request: Request,
    blitz: Option<&AbortSignal>,
    glide: Option<&AbortSignal>,
) -> Result<(String, Bytes), ProviderError> {
    tokio::select! {
        res = provider.fetch_async(request) => res,
        () = wait_for_abort(blitz, glide) => Err(ProviderError::Abort),
    }
}

async fn wait_for_abort(blitz: Option<&AbortSignal>, glide: Option<&AbortSignal>) {
    loop {
        if blitz.is_some_and(AbortSignal::aborted) || glide.is_some_and(AbortSignal::aborted) {
            return;
        }
        tokio::task::yield_now().await;
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
        ProviderError::ReqwestError(e) if e.is_connect() || e.is_timeout() || e.is_request() => {
            "Couldn't reach host".to_string()
        }
        ProviderError::ReqwestError(_) => "Couldn't reach host".to_string(),
        ProviderError::DataUrl(_) => "Invalid data URL".to_string(),
        ProviderError::DataUrlBase64(_) => "Invalid data URL".to_string(),
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
<p><a href="glide://reload" target="_top">Retry</a></p>
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
    use reqwest::StatusCode;
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
        assert!(html.contains("glide://reload"));
        assert!(html.contains("Retry"));
    }

    #[test]
    fn maps_http_status_to_reason() {
        let err = ProviderError::HttpStatus {
            status: StatusCode::NOT_FOUND,
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
        let net = GlideNet::new(inner, state.clone(), waker);

        let record = Arc::new(StdMutex::new(None));
        let handler = Box::new(RecordHandler {
            done: record.clone(),
        });
        let missing = file_url("/tmp/glide-net-missing-no-such-file-xyz.html");
        net.fetch(chrome_id, Request::get(missing), handler);

        for _ in 0..200 {
            if record.lock().expect("record").is_some() && !state.is_loading() {
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
        assert!(!state.is_loading());
        assert!(wake_count.load(Ordering::Relaxed) > 0);
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
        let net = GlideNet::new(inner, state, waker);

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
    fn glide_net_subresource_failure_delivers_nothing() {
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();

        let waker: Arc<dyn NetWaker> = Arc::new(|_| {});
        let inner = Arc::new(Provider::new(Some(waker.clone())));
        let state = Arc::new(LoadState::default());
        state.set_chrome_doc_id(1);
        let net = GlideNet::new(inner, state.clone(), waker);

        let record = Arc::new(StdMutex::new(None));
        let handler = Box::new(RecordHandler {
            done: record.clone(),
        });
        let missing = file_url("/tmp/glide-subresource-missing-zzz.css");
        net.fetch(99, Request::get(missing), handler);

        for _ in 0..200 {
            if !state.is_loading() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(record.lock().expect("record").is_none());
        assert!(!state.is_loading());
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
        let net = GlideNet::new(inner, state.clone(), waker);

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
            if state.is_loading() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(state.is_loading());

        state.stop_page_load();

        for _ in 0..200 {
            if !state.is_loading() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(!state.is_loading());
        assert!(record.lock().expect("record").is_none());
    }
}
