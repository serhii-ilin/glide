//! ES module specifier resolution helpers (F-074).

use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Bare(String),
    Invalid(String),
}

/// Resolve a module specifier against the referrer's URL.
pub fn resolve_specifier(referrer: &Url, specifier: &str) -> Result<Url, ResolveError> {
    let specifier = specifier.trim();
    if specifier.is_empty() {
        return Err(ResolveError::Invalid("empty specifier".into()));
    }
    if is_bare_specifier(specifier) {
        return Err(ResolveError::Bare(specifier.to_string()));
    }
    if let Ok(abs) = Url::parse(specifier) {
        return Ok(abs);
    }
    referrer
        .join(specifier)
        .map_err(|err| ResolveError::Invalid(err.to_string()))
}

pub fn resolve_specifier_from_str(referrer: &str, specifier: &str) -> Result<Url, ResolveError> {
    let referrer = Url::parse(referrer).map_err(|e| ResolveError::Invalid(e.to_string()))?;
    resolve_specifier(&referrer, specifier)
}

/// A bare specifier has no relative prefix and no scheme (import maps would map these).
pub fn is_bare_specifier(specifier: &str) -> bool {
    let s = specifier.trim();
    if s.is_empty() {
        return false;
    }
    if s.starts_with('.') || s.starts_with('/') {
        return false;
    }
    if s.contains("://") || s.starts_with("data:") {
        return false;
    }
    true
}

pub fn log_module_fetch_failure(page_url: Option<&str>, url: &str, detail: String) {
    crate::js::diagnostics::record_loader_failure();
    let kind = crate::js::diagnostics::classify(&detail)
        .as_str()
        .to_string();
    crate::logging::error(
        "js.loader",
        "module fetch failed",
        &[
            ("page_url", page_url.unwrap_or("unknown").to_string()),
            ("script_src", url.to_string()),
            ("error", detail),
            ("error_kind", kind),
            (
                "hint",
                "The module graph stopped; check the module URL and its response.".into(),
            ),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_specifier() {
        let base = Url::parse("https://example.test/dir/page.html").unwrap();
        let resolved = resolve_specifier(&base, "./dep.js").unwrap();
        assert_eq!(resolved.as_str(), "https://example.test/dir/dep.js");
    }

    #[test]
    fn rejects_bare_specifier() {
        let base = Url::parse("https://example.test/").unwrap();
        assert!(matches!(
            resolve_specifier(&base, "react"),
            Err(ResolveError::Bare(_))
        ));
    }

    #[test]
    fn accepts_absolute_specifier() {
        let base = Url::parse("file:///page.html").unwrap();
        let resolved =
            resolve_specifier(&base, "data:text/javascript,export%20default%201").unwrap();
        assert!(resolved.scheme() == "data");
    }
}
