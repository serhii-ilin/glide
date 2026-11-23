//! Built-in `about:` documents served without network I/O.

use url::Url;

/// Minimal HTML for [about:blank](https://html.spec.whatwg.org/#the-about:blank-document).
pub const ABOUT_BLANK_HTML: &str = "<!DOCTYPE html><html><head></head><body></body></html>";

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
}
