//! Built-in `about:` documents served without network I/O.

use url::Url;

/// Minimal HTML for [about:blank](https://html.spec.whatwg.org/#the-about:blank-document).
pub const ABOUT_BLANK_HTML: &str = "<!DOCTYPE html><html><head></head><body></body></html>";

/// Reason shown on Glide's error page for unsupported `about:` navigations (Fetch local network error).
pub const ABOUT_LOCAL_NETWORK_ERROR: &str = "A local network error occurred";

/// Resolved document bytes for a supported `about:` URL, if any.
pub fn about_document(url: &Url) -> Option<&'static str> {
    if url.scheme() != "about" {
        return None;
    }
    match url.path() {
        "blank" => Some(ABOUT_BLANK_HTML),
        _ => None,
    }
}

/// Whether this `about:` URL is the [about:blank](https://html.spec.whatwg.org/#the-about:blank-document) document.
pub fn is_about_blank(url: &Url) -> bool {
    about_document(url).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_blank_url_resolves_to_empty_html() {
        let url = url::Url::parse("about:blank").expect("about url");
        assert_eq!(about_document(&url), Some(ABOUT_BLANK_HTML));
    }

    #[test]
    fn unknown_about_urls_are_not_supported_yet() {
        let url = url::Url::parse("about:legacy-compat").expect("about url");
        assert_eq!(about_document(&url), None);
    }

    #[test]
    fn uppercase_about_blank_path_is_not_blank_document() {
        let url = url::Url::parse("ABOUT:BLANK").expect("about url");
        assert!(!is_about_blank(&url));
        assert_eq!(about_document(&url), None);
    }
}
