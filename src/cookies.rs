//! Profile-wide HTTP cookie jar shared by navigation, `fetch`/XHR, and `document.cookie`.

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use blitz_traits::net::http::{self, HeaderMap, HeaderValue};
use cookie::Cookie as RawCookie;
use cookie_store::CookieStore;
use url::Url;

const COOKIES_FILE: &str = "cookies.json";

static GLOBAL: OnceLock<Arc<CookieJar>> = OnceLock::new();

/// One jar for every tab in the process (per profile when persistence is enabled).
#[derive(Debug)]
pub struct CookieJar {
    inner: RwLock<CookieStore>,
    persist_path: Option<PathBuf>,
}

impl CookieJar {
    pub fn empty() -> Self {
        Self {
            inner: RwLock::new(CookieStore::default()),
            persist_path: None,
        }
    }

    pub fn with_persist_path(path: PathBuf) -> Self {
        Self {
            inner: RwLock::new(CookieStore::default()),
            persist_path: Some(path),
        }
    }

    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        let store = if path.is_file() {
            let file = File::open(path).map_err(|e| e.to_string())?;
            cookie_store::serde::json::load(BufReader::new(file)).map_err(|e| e.to_string())?
        } else {
            CookieStore::default()
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

    /// Install this jar as the process-global store (window, headless, control).
    pub fn install_global(self: Arc<Self>) {
        let _ = GLOBAL.set(self);
    }

    pub fn global() -> Arc<Self> {
        GLOBAL
            .get()
            .cloned()
            .unwrap_or_else(|| Arc::new(Self::empty()))
    }

    pub fn cookies_path(profile_dir: &Path) -> PathBuf {
        profile_dir.join(COOKIES_FILE)
    }

    pub fn append_request_cookies(&self, url: &Url, headers: &mut HeaderMap) {
        if !matches!(url.scheme(), "http" | "https") {
            return;
        }
        let store = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let cookie_header: String = store
            .get_request_values(url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        if cookie_header.is_empty() {
            return;
        }
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

    /// `document.cookie` getter string (excludes HttpOnly cookies).
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

    /// Apply a `document.cookie` assignment (`name=value` or deletion via `Max-Age=0`).
    pub fn set_document_cookie(&self, page_url: &Url, assignment: &str) -> Result<(), String> {
        let assignment = assignment.trim();
        if assignment.is_empty() {
            return Ok(());
        }
        let mut store = self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        store
            .parse(assignment, page_url)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(test)]
    pub fn replace_global(jar: Arc<Self>) {
        let _ = GLOBAL.set(jar);
    }
}

/// Load from a profile directory when present; otherwise an in-memory jar.
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
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
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
    fn document_set_visible_to_subsequent_requests() {
        let jar = CookieJar::empty();
        let page = url("http://example.test/app");
        jar.set_document_cookie(&page, "session=abc; Path=/")
            .expect("set");
        let mut headers = HeaderMap::new();
        jar.append_request_cookies(&page, &mut headers);
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
        jar.append_request_cookies(&page, &mut headers);
        assert!(!headers.contains_key(http::header::COOKIE));
    }

    #[test]
    fn max_age_zero_deletes_cookie() {
        let jar = CookieJar::empty();
        let page = url("http://example.test/");
        jar.store_response_headers(&page, &[("Set-Cookie".into(), "gone=1; Path=/".into())]);
        assert!(jar.document_cookie(&page).contains("gone=1"));
        jar.set_document_cookie(&page, "gone=; Max-Age=0; Path=/")
            .expect("delete");
        assert!(!jar.document_cookie(&page).contains("gone=1"));
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
}
