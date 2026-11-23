//! Turn an address-bar string into a URL.
//!
//! Absolute `http`, `https`, and `file` URLs are used as typed. Anything else
//! that looks like a host (`example.com`, `localhost:8080`) gets an `https`
//! scheme. A phrase with spaces is sent to DuckDuckGo's HTML frontend, which
//! renders without page JavaScript.

use url::Url;

const SEARCH: &str = "https://duckduckgo.com/html/";

pub fn resolve_location(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return "https://example.com/".to_string();
    }

    if let Ok(url) = Url::parse(raw) {
        if matches!(url.scheme(), "http" | "https" | "file" | "about") {
            return url.to_string();
        }
        if url.as_str() == "glide://bookmarks" {
            return url.to_string();
        }
    }

    if !raw.contains([' ', '\t'])
        && let Some(url) = host_like(raw)
    {
        return url;
    }

    let mut url = Url::parse(SEARCH).expect("search url is valid");
    url.query_pairs_mut().append_pair("q", raw);
    url.to_string()
}

fn host_like(raw: &str) -> Option<String> {
    let looks_like_host = raw.starts_with("localhost")
        || raw.starts_with("127.")
        || raw.starts_with('[')
        || raw.contains('.');
    if !looks_like_host {
        return None;
    }
    let url = Url::parse(&format!("https://{raw}")).ok()?;
    url.host()?;
    Some(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::resolve_location;

    #[test]
    fn keeps_absolute_http_urls() {
        assert_eq!(
            resolve_location("https://example.com/docs"),
            "https://example.com/docs"
        );
        assert_eq!(
            resolve_location("  http://example.com  "),
            "http://example.com/"
        );
    }

    #[test]
    fn adds_https_to_bare_hosts() {
        assert_eq!(resolve_location("example.com"), "https://example.com/");
        assert_eq!(
            resolve_location("example.com/path"),
            "https://example.com/path"
        );
        assert_eq!(
            resolve_location("localhost:8080"),
            "https://localhost:8080/"
        );
    }

    #[test]
    fn searches_phrases() {
        let url = resolve_location("rust blog");
        assert!(url.starts_with("https://duckduckgo.com/html/?q="), "{url}");
        assert!(url.contains("rust"));
        assert!(url.contains("blog"));
    }

    #[test]
    fn empty_input_opens_example() {
        assert_eq!(resolve_location("   "), "https://example.com/");
    }

    #[test]
    fn glide_bookmarks_passes_through() {
        assert_eq!(resolve_location("glide://bookmarks"), "glide://bookmarks");
        let search = resolve_location("glide://back");
        assert!(
            search.starts_with("https://duckduckgo.com/html/?q="),
            "{search}"
        );
    }

    #[test]
    fn keeps_about_urls() {
        assert_eq!(resolve_location("about:blank"), "about:blank");
        assert_eq!(resolve_location("about:blank#frag"), "about:blank#frag");
        assert_eq!(resolve_location("about:blank?x=1"), "about:blank?x=1");
        let search = resolve_location("about back");
        assert!(
            search.starts_with("https://duckduckgo.com/html/?q="),
            "{search}"
        );
    }
}
