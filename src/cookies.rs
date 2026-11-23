//! Profile-wide HTTP cookie jar shared by navigation, `fetch`/XHR, and `document.cookie`.

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use blitz_traits::net::http::{self, HeaderMap, HeaderValue, Method};
use cookie::Cookie as RawCookie;
use cookie::SameSite;
use cookie_store::CookieStore;
use url::Url;

const COOKIES_FILE: &str = "cookies.json";
const PSL_BYTES: &[u8] = include_bytes!("../third_party/publicsuffix/public_suffix_list.dat");

static GLOBAL: RwLock<Option<Arc<CookieJar>>> = RwLock::new(None);
static PSL: OnceLock<publicsuffix::List> = OnceLock::new();

#[cfg(test)]
static GLOBAL_JAR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn public_suffix_list() -> publicsuffix::List {
    PSL.get_or_init(|| {
        publicsuffix::List::from_bytes(PSL_BYTES).expect("embedded public suffix list")
    })
    .clone()
}

fn default_store() -> CookieStore {
    CookieStore::new_with_public_suffix(Some(public_suffix_list()))
}

/// How a request is being made (for SameSite and credentials policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieRequestKind {
    /// Top-level navigation (`<a>`, `location`, document load).
    Navigation,
    /// Parser-inserted or layout subresource (`<img>`, `<script>` src, CSS).
    Subresource,
    /// Page `fetch()` / `XMLHttpRequest`.
    Fetch,
    /// Page `WebSocket` handshake.
    WebSocket,
}

/// Map a `ws:` / `wss:` URL to the `http:` / `https:` origin used for cookies.
pub fn ws_to_http_url(ws: &Url) -> Option<Url> {
    let scheme = match ws.scheme() {
        "ws" => "http",
        "wss" => "https",
        _ => return None,
    };
    let mut http = ws.clone();
    http.set_scheme(scheme).ok()?;
    Some(http)
}

/// Fetch credentials mode for cookie attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FetchCredentials {
    Omit,
    #[default]
    SameOrigin,
    Include,
}

impl FetchCredentials {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "omit" => Self::Omit,
            "include" => Self::Include,
            _ => Self::SameOrigin,
        }
    }
}

/// One jar for every tab in the process (per profile when persistence is enabled).
#[derive(Debug)]
pub struct CookieJar {
    inner: RwLock<CookieStore>,
    persist_path: Option<PathBuf>,
}

impl CookieJar {
    pub fn empty() -> Self {
        Self {
            inner: RwLock::new(default_store()),
            persist_path: None,
        }
    }

    pub fn with_persist_path(path: PathBuf) -> Self {
        Self {
            inner: RwLock::new(default_store()),
            persist_path: Some(path),
        }
    }

    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        let store = if path.is_file() {
            let file = File::open(path).map_err(|e| e.to_string())?;
            let loaded =
                cookie_store::serde::json::load(BufReader::new(file)).map_err(|e| e.to_string())?;
            loaded.with_suffix_list(public_suffix_list())
        } else {
            default_store()
        };
        Ok(Self {
            inner: RwLock::new(store),
            persist_path: Some(path.to_path_buf()),
        })
    }

    pub fn save(&self) -> Result<(), String> {
        let Some(path) = self.persist_path.as_ref() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let file = File::create(path).map_err(|e| e.to_string())?;
        let store = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cookie_store::serde::json::save(&store, &mut BufWriter::new(file))
            .map_err(|e| e.to_string())
    }

    fn maybe_persist(&self) {
        if self.persist_path.is_some() {
            let _ = self.save();
        }
    }

    pub fn install_global(jar: Arc<Self>) {
        *GLOBAL.write().unwrap() = Some(jar);
    }

    pub fn global() -> Arc<Self> {
        if let Ok(guard) = GLOBAL.read()
            && let Some(jar) = guard.as_ref()
        {
            return Arc::clone(jar);
        }
        let mut guard = GLOBAL.write().unwrap();
        guard.get_or_insert_with(|| Arc::new(Self::empty())).clone()
    }

    /// Ensure headless, MCP, and CLI paths share one jar even without `run_window`.
    pub fn ensure_process_global(profile_dir: Option<&Path>) {
        if GLOBAL.read().unwrap().is_some() {
            return;
        }
        let mut guard = GLOBAL.write().unwrap();
        if guard.is_none() {
            *guard = Some(bootstrap_from_profile(profile_dir));
        }
    }

    pub fn cookies_path(profile_dir: &Path) -> PathBuf {
        profile_dir.join(COOKIES_FILE)
    }

    pub fn append_request_cookies(
        &self,
        request_url: &Url,
        document_url: Option<&Url>,
        kind: CookieRequestKind,
        credentials: FetchCredentials,
        method: Method,
        headers: &mut HeaderMap,
    ) {
        if !matches!(request_url.scheme(), "http" | "https") {
            return;
        }
        if !credentials_allow(credentials, request_url, document_url) {
            return;
        }
        let store = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let doc = document_url.unwrap_or(request_url);
        let pairs: Vec<(String, String)> = store
            .matches(request_url)
            .into_iter()
            .filter(|c| cookie_allowed_on_request(c, request_url, doc, kind, method.clone()))
            .map(|c| {
                let (name, value) = c.name_value();
                (name.to_string(), value.to_string())
            })
            .collect();
        if pairs.is_empty() {
            return;
        }
        let cookie_header = pairs
            .iter()
            .map(|(n, v)| format!("{n}={v}"))
            .collect::<Vec<_>>()
            .join("; ");
        if let Ok(value) = HeaderValue::from_str(&cookie_header) {
            headers.insert(http::header::COOKIE, value);
        }
    }

    pub fn store_response_headers(&self, url: &Url, headers: &[(String, String)]) {
        if !matches!(url.scheme(), "http" | "https") {
            return;
        }
        let mut raw: Vec<RawCookie<'static>> = Vec::new();
        for (name, value) in headers {
            if !name.eq_ignore_ascii_case("set-cookie") {
                continue;
            }
            if let Ok(parsed) = RawCookie::parse(value.clone()) {
                raw.push(parsed.into_owned());
            }
        }
        if raw.is_empty() {
            return;
        }
        let mut store = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        store.store_response_cookies(raw.into_iter(), url);
        drop(store);
        self.maybe_persist();
    }

    pub fn store_response_header_map(&self, url: &Url, headers: &HeaderMap) {
        let pairs: Vec<(String, String)> = headers
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.as_str().to_string(), v.to_string()))
            })
            .collect();
        self.store_response_headers(url, &pairs);
    }

    pub fn document_cookie(&self, page_url: &Url) -> String {
        let store = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        store
            .matches(page_url)
            .into_iter()
            .filter(|c| !c.http_only().unwrap_or(false))
            .map(|c| {
                let (name, value) = c.name_value();
                format!("{name}={value}")
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub fn set_document_cookie(&self, page_url: &Url, assignment: &str) {
        let assignment = assignment.trim();
        if assignment.is_empty() {
            return;
        }
        if assignment_has_httponly_flag(assignment) {
            return;
        }
        let Some(name) = cookie_assignment_name(assignment) else {
            return;
        };
        {
            let store = self
                .inner
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if store
                .matches(page_url)
                .into_iter()
                .any(|c| c.name() == name && c.http_only().unwrap_or(false))
            {
                return;
            }
        }
        let mut store = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = store.parse(assignment, page_url);
        drop(store);
        self.maybe_persist();
    }

    #[cfg(test)]
    pub fn lock_global_for_test() -> std::sync::MutexGuard<'static, ()> {
        GLOBAL_JAR_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(test)]
    pub fn replace_global(jar: Arc<Self>) {
        *GLOBAL.write().unwrap() = Some(jar);
    }

    #[cfg(test)]
    pub fn reset_global() {
        *GLOBAL.write().unwrap() = None;
    }
}

fn assignment_has_httponly_flag(assignment: &str) -> bool {
    assignment
        .split(';')
        .skip(1)
        .any(|part| part.trim().eq_ignore_ascii_case("httponly"))
}

fn cookie_assignment_name(assignment: &str) -> Option<&str> {
    let first = assignment.split(';').next()?.trim();
    let (name, _) = first.split_once('=')?;
    Some(name.trim())
}

fn credentials_allow(
    credentials: FetchCredentials,
    request_url: &Url,
    document_url: Option<&Url>,
) -> bool {
    match credentials {
        FetchCredentials::Omit => false,
        FetchCredentials::Include => true,
        FetchCredentials::SameOrigin => {
            let doc = document_url.unwrap_or(request_url);
            same_site(doc, request_url)
        }
    }
}

fn same_site(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

fn cookie_allowed_on_request(
    cookie: &cookie_store::Cookie<'_>,
    request_url: &Url,
    document_url: &Url,
    kind: CookieRequestKind,
    method: Method,
) -> bool {
    match cookie.same_site() {
        None | Some(SameSite::None) => true,
        Some(SameSite::Strict) => same_site(request_url, document_url),
        Some(SameSite::Lax) => match kind {
            CookieRequestKind::Navigation => {
                !(method == Method::POST && !same_site(request_url, document_url))
            }
            CookieRequestKind::Subresource
            | CookieRequestKind::Fetch
            | CookieRequestKind::WebSocket => same_site(request_url, document_url),
        },
    }
}

pub fn bootstrap_from_profile(profile_dir: Option<&Path>) -> Arc<CookieJar> {
    match profile_dir {
        Some(dir) => {
            let path = CookieJar::cookies_path(dir);
            Arc::new(
                CookieJar::load_from_path(&path)
                    .unwrap_or_else(|_| CookieJar::with_persist_path(path)),
            )
        }
        None => Arc::new(CookieJar::empty()),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn ws_to_http_url_maps_schemes() {
        let ws = url("ws://example.test/socket");
        let http = ws_to_http_url(&ws).expect("http");
        assert_eq!(http.scheme(), "http");
        assert_eq!(http.path(), "/socket");
        let wss = url("wss://example.test/socket");
        let https = ws_to_http_url(&wss).expect("https");
        assert_eq!(https.scheme(), "https");
    }

    #[test]
    fn global_without_install_shares_one_jar() {
        let _jar_guard = CookieJar::lock_global_for_test();
        CookieJar::reset_global();
        let a = CookieJar::global();
        a.set_document_cookie(&url("http://example.test/"), "a=1; Path=/");
        let b = CookieJar::global();
        assert!(
            b.document_cookie(&url("http://example.test/"))
                .contains("a=1")
        );
        CookieJar::reset_global();
    }

    #[test]
    fn httponly_hidden_from_document_cookie() {
        let jar = CookieJar::empty();
        let page = url("http://example.test/");
        jar.store_response_headers(&page, &[("Set-Cookie".into(), "visible=1; Path=/".into())]);
        jar.store_response_headers(
            &page,
            &[("Set-Cookie".into(), "secret=2; Path=/; HttpOnly".into())],
        );
        let doc = jar.document_cookie(&page);
        assert!(doc.contains("visible=1"));
        assert!(!doc.contains("secret"));
    }

    #[test]
    fn document_cannot_set_httponly_attribute() {
        let jar = CookieJar::empty();
        let page = url("http://example.test/");
        jar.set_document_cookie(&page, "secret=1; HttpOnly; Path=/");
        assert!(!jar.document_cookie(&page).contains("secret"));
        let mut headers = HeaderMap::new();
        jar.append_request_cookies(
            &page,
            Some(&page),
            CookieRequestKind::Fetch,
            FetchCredentials::Include,
            Method::GET,
            &mut headers,
        );
        assert!(!headers.contains_key(http::header::COOKIE));
    }

    #[test]
    fn document_cannot_overwrite_existing_httponly_cookie() {
        let jar = CookieJar::empty();
        let page = url("http://example.test/");
        jar.store_response_headers(
            &page,
            &[("Set-Cookie".into(), "hidden=1; Path=/; HttpOnly".into())],
        );
        jar.set_document_cookie(&page, "hidden=nope; Path=/");
        let mut headers = HeaderMap::new();
        jar.append_request_cookies(
            &page,
            Some(&page),
            CookieRequestKind::Fetch,
            FetchCredentials::Include,
            Method::GET,
            &mut headers,
        );
        let cookie = headers
            .get(http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(cookie.contains("hidden=1"));
        assert!(!cookie.contains("nope"));
    }

    #[test]
    fn document_set_visible_to_subsequent_requests() {
        let jar = CookieJar::empty();
        let page = url("http://example.test/app");
        jar.set_document_cookie(&page, "session=abc; Path=/");
        let mut headers = HeaderMap::new();
        jar.append_request_cookies(
            &page,
            Some(&page),
            CookieRequestKind::Fetch,
            FetchCredentials::SameOrigin,
            Method::GET,
            &mut headers,
        );
        let cookie = headers
            .get(http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(cookie.contains("session=abc"));
    }

    #[test]
    fn secure_cookie_not_sent_on_http() {
        let jar = CookieJar::empty();
        let page = url("http://example.test/");
        jar.store_response_headers(
            &page,
            &[("Set-Cookie".into(), "sec=1; Path=/; Secure".into())],
        );
        let mut headers = HeaderMap::new();
        jar.append_request_cookies(
            &page,
            Some(&page),
            CookieRequestKind::Navigation,
            FetchCredentials::Include,
            Method::GET,
            &mut headers,
        );
        assert!(!headers.contains_key(http::header::COOKIE));
    }

    #[test]
    fn expired_cookie_assignment_is_ignored() {
        let jar = CookieJar::empty();
        let page = url("http://example.test/");
        jar.set_document_cookie(
            &page,
            "old=1; expires=Thu, 01 Jan 1970 00:00:00 GMT; Path=/",
        );
        assert!(!jar.document_cookie(&page).contains("old=1"));
    }

    #[test]
    fn rejected_document_cookie_assignment_is_ignored() {
        let jar = CookieJar::empty();
        let page = url("http://example.test/");
        jar.set_document_cookie(&page, "a=1; Domain=example.com; Path=/");
        assert!(!jar.document_cookie(&page).contains("a=1"));
    }

    #[test]
    fn expired_assignment_deletes_existing_cookie() {
        let jar = CookieJar::empty();
        let page = url("http://example.test/");
        jar.set_document_cookie(&page, "stay=1; Path=/");
        assert!(jar.document_cookie(&page).contains("stay=1"));
        jar.set_document_cookie(
            &page,
            "stay=; expires=Thu, 01 Jan 1970 00:00:00 GMT; Path=/",
        );
        assert!(!jar.document_cookie(&page).contains("stay=1"));
    }

    #[test]
    fn max_age_zero_deletes_cookie() {
        let jar = CookieJar::empty();
        let page = url("http://example.test/");
        jar.store_response_headers(&page, &[("Set-Cookie".into(), "gone=1; Path=/".into())]);
        assert!(jar.document_cookie(&page).contains("gone=1"));
        jar.set_document_cookie(&page, "gone=; Max-Age=0; Path=/");
        assert!(!jar.document_cookie(&page).contains("gone=1"));
    }

    #[test]
    fn expired_expires_cookie_not_sent() {
        let jar = CookieJar::empty();
        let page = url("http://example.test/");
        jar.store_response_headers(
            &page,
            &[(
                "Set-Cookie".into(),
                "old=1; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT".into(),
            )],
        );
        assert!(!jar.document_cookie(&page).contains("old=1"));
        let mut headers = HeaderMap::new();
        jar.append_request_cookies(
            &page,
            Some(&page),
            CookieRequestKind::Navigation,
            FetchCredentials::Include,
            Method::GET,
            &mut headers,
        );
        assert!(!headers.contains_key(http::header::COOKIE));
    }

    #[test]
    fn domain_suffix_cookie_sent_only_to_matching_host() {
        let jar = CookieJar::empty();
        jar.store_response_headers(
            &url("http://www.example.com/"),
            &[(
                "Set-Cookie".into(),
                "site=1; Domain=example.com; Path=/".into(),
            )],
        );
        let mut www = HeaderMap::new();
        jar.append_request_cookies(
            &url("http://www.example.com/"),
            Some(&url("http://www.example.com/")),
            CookieRequestKind::Navigation,
            FetchCredentials::Include,
            Method::GET,
            &mut www,
        );
        assert!(www.contains_key(http::header::COOKIE));
        let mut other = HeaderMap::new();
        jar.append_request_cookies(
            &url("http://other.com/"),
            Some(&url("http://other.com/")),
            CookieRequestKind::Navigation,
            FetchCredentials::Include,
            Method::GET,
            &mut other,
        );
        assert!(!other.contains_key(http::header::COOKIE));
    }

    #[test]
    fn rejects_domain_public_suffix() {
        let jar = CookieJar::empty();
        let page = url("http://example.com/");
        jar.store_response_headers(
            &page,
            &[("Set-Cookie".into(), "a=1; Domain=com; Path=/".into())],
        );
        assert!(!jar.document_cookie(&page).contains("a=1"));
    }

    #[test]
    fn samesite_strict_omitted_on_cross_site_request() {
        let jar = CookieJar::empty();
        jar.store_response_headers(
            &url("https://a.test/"),
            &[(
                "Set-Cookie".into(),
                "tight=1; Path=/; SameSite=Strict".into(),
            )],
        );
        let mut same = HeaderMap::new();
        jar.append_request_cookies(
            &url("https://a.test/resource"),
            Some(&url("https://a.test/page")),
            CookieRequestKind::Fetch,
            FetchCredentials::SameOrigin,
            Method::GET,
            &mut same,
        );
        assert!(same.contains_key(http::header::COOKIE));
        let mut cross = HeaderMap::new();
        jar.append_request_cookies(
            &url("https://a.test/resource"),
            Some(&url("https://b.test/page")),
            CookieRequestKind::Fetch,
            FetchCredentials::SameOrigin,
            Method::GET,
            &mut cross,
        );
        assert!(!cross.contains_key(http::header::COOKIE));
    }

    #[test]
    fn cross_origin_fetch_omits_cookies_by_default() {
        let jar = CookieJar::empty();
        jar.store_response_headers(
            &url("https://other.test/"),
            &[("Set-Cookie".into(), "id=1; Path=/".into())],
        );
        let mut headers = HeaderMap::new();
        jar.append_request_cookies(
            &url("https://other.test/"),
            Some(&url("https://example.test/")),
            CookieRequestKind::Fetch,
            FetchCredentials::SameOrigin,
            Method::GET,
            &mut headers,
        );
        assert!(!headers.contains_key(http::header::COOKIE));
        jar.store_response_headers(
            &url("https://example.test/"),
            &[("Set-Cookie".into(), "home=1; Path=/".into())],
        );
        let mut same = HeaderMap::new();
        jar.append_request_cookies(
            &url("https://example.test/"),
            Some(&url("https://example.test/")),
            CookieRequestKind::Fetch,
            FetchCredentials::SameOrigin,
            Method::GET,
            &mut same,
        );
        assert!(same.contains_key(http::header::COOKIE));
    }

    #[test]
    fn persist_round_trips_through_json() {
        let path =
            std::env::temp_dir().join(format!("glide-cookies-test-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let jar = CookieJar::with_persist_path(path.clone());
        let page = url("http://example.test/");
        jar.store_response_headers(
            &page,
            &[(
                "Set-Cookie".into(),
                "persisted=1; Path=/; Max-Age=3600".into(),
            )],
        );
        jar.save().expect("save");
        let loaded = CookieJar::load_from_path(&path).expect("load");
        assert!(loaded.document_cookie(&page).contains("persisted=1"));
    }

    #[test]
    fn path_matching_limits_document_cookie() {
        let jar = CookieJar::empty();
        let root = url("http://example.test/");
        jar.store_response_headers(
            &root,
            &[("Set-Cookie".into(), "narrow=1; Path=/app".into())],
        );
        assert!(!jar.document_cookie(&root).contains("narrow=1"));
        let app = url("http://example.test/app/page");
        assert!(jar.document_cookie(&app).contains("narrow=1"));
    }

    #[test]
    fn loopback_set_cookie_echoed_without_replace_global() {
        let _jar_guard = CookieJar::lock_global_for_test();
        CookieJar::reset_global();
        CookieJar::ensure_process_global(None);
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let seen: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let seen2 = seen.clone();
        thread::spawn(move || {
            for i in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).expect("read");
                let req = String::from_utf8_lossy(&buf[..n]);
                if i == 0 {
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nSet-Cookie: sid=alpha; Path=/\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                    )
                    .expect("write");
                } else {
                    *seen2.lock().expect("lock") = req
                        .lines()
                        .find(|l| l.to_ascii_lowercase().starts_with("cookie:"))
                        .map(|l| l.to_ascii_lowercase());
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                    )
                    .expect("write");
                }
            }
        });
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let set = url(&format!("http://127.0.0.1:{port}/set"));
        rt.block_on(crate::http::fetch_followed(
            blitz_traits::net::Request::get(set),
        ))
        .expect("set");
        thread::sleep(Duration::from_millis(50));
        let next = url(&format!("http://127.0.0.1:{port}/next"));
        rt.block_on(crate::http::fetch_followed(
            blitz_traits::net::Request::get(next),
        ))
        .expect("next");
        thread::sleep(Duration::from_millis(50));
        let header = seen.lock().expect("lock").clone().unwrap_or_default();
        assert!(
            header.contains("sid=alpha"),
            "missing cookie on second request: {header}"
        );
        CookieJar::reset_global();
    }

    #[test]
    fn script_set_cookie_sent_on_next_same_origin_fetch() {
        let _jar_guard = CookieJar::lock_global_for_test();
        CookieJar::reset_global();
        let jar = Arc::new(CookieJar::empty());
        CookieJar::replace_global(Arc::clone(&jar));
        let page = url("http://example.test/");
        jar.set_document_cookie(&page, "fromScript=1; Path=/");
        let mut headers = HeaderMap::new();
        jar.append_request_cookies(
            &url("http://example.test/api"),
            Some(&page),
            CookieRequestKind::Fetch,
            FetchCredentials::SameOrigin,
            Method::GET,
            &mut headers,
        );
        let cookie = headers
            .get(http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(cookie.contains("fromScript=1"));
        CookieJar::reset_global();
    }

    #[test]
    fn lax_cookie_omitted_on_cross_site_post_navigation() {
        let jar = CookieJar::empty();
        jar.store_response_headers(
            &url("http://127.0.0.1:8001/"),
            &[("Set-Cookie".into(), "lax=1; Path=/; SameSite=Lax".into())],
        );
        let mut cross = HeaderMap::new();
        jar.append_request_cookies(
            &url("http://127.0.0.1:8002/submit"),
            Some(&url("http://127.0.0.1:8001/page")),
            CookieRequestKind::Navigation,
            FetchCredentials::Include,
            Method::POST,
            &mut cross,
        );
        assert!(!cross.contains_key(http::header::COOKIE));
    }

    #[test]
    fn lax_cookie_sent_on_same_port_post_navigation() {
        let jar = CookieJar::empty();
        let page = url("http://127.0.0.1:9000/page");
        jar.store_response_headers(
            &page,
            &[("Set-Cookie".into(), "lax=1; Path=/; SameSite=Lax".into())],
        );
        let mut headers = HeaderMap::new();
        jar.append_request_cookies(
            &url("http://127.0.0.1:9000/submit"),
            Some(&page),
            CookieRequestKind::Navigation,
            FetchCredentials::Include,
            Method::POST,
            &mut headers,
        );
        let cookie = headers
            .get(http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(cookie.contains("lax=1"));
    }

    #[test]
    fn subresource_request_includes_matching_cookie() {
        let jar = CookieJar::empty();
        let page = url("http://example.test/page");
        jar.store_response_headers(&page, &[("Set-Cookie".into(), "res=1; Path=/".into())]);
        let mut headers = HeaderMap::new();
        jar.append_request_cookies(
            &url("http://example.test/app.js"),
            Some(&page),
            CookieRequestKind::Subresource,
            FetchCredentials::Include,
            Method::GET,
            &mut headers,
        );
        let cookie = headers
            .get(http::header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(cookie.contains("res=1"));
    }
}
