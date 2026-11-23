//! ES module specifier resolution helpers (F-074, F-080 import maps).

use super::import_map::ImportMapRegistry;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Bare(String),
    Invalid(String),
}

/// Resolve a module specifier against the referrer's URL.
pub fn resolve_specifier(
    referrer: &Url,
    specifier: &str,
    import_map: Option<&ImportMapRegistry>,
) -> Result<Url, ResolveError> {
    let specifier = specifier.trim();
    if specifier.is_empty() {
        return Err(ResolveError::Invalid("empty specifier".into()));
    }
    if let Ok(abs) = Url::parse(specifier) {
        return Ok(abs);
    }
    if specifier.starts_with('.') || specifier.starts_with('/') {
        return referrer
            .join(specifier)
            .map_err(|err| ResolveError::Invalid(err.to_string()));
    }
    if is_bare_specifier(specifier) {
        if let Some(map) = import_map
            && let Some(url) = map.resolve_bare(referrer, specifier)
        {
            return Ok(url);
        }
        return Err(ResolveError::Bare(specifier.to_string()));
    }
    referrer
        .join(specifier)
        .map_err(|err| ResolveError::Invalid(err.to_string()))
}

pub fn resolve_specifier_from_str(
    referrer: &str,
    specifier: &str,
    import_map: Option<&ImportMapRegistry>,
) -> Result<Url, ResolveError> {
    let referrer = Url::parse(referrer).map_err(|e| ResolveError::Invalid(e.to_string()))?;
    resolve_specifier(&referrer, specifier, import_map)
}

pub fn bare_specifier_rejection_message(spec: &str, import_map_active: bool) -> String {
    if import_map_active {
        format!("Failed to resolve module specifier '{spec}'")
    } else {
        format!("bare specifier {spec} (import maps not supported)")
    }
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
        let resolved = resolve_specifier(&base, "./dep.js", None).unwrap();
        assert_eq!(resolved.as_str(), "https://example.test/dir/dep.js");
    }

    #[test]
    fn rejects_bare_specifier() {
        let base = Url::parse("https://example.test/").unwrap();
        assert!(matches!(
            resolve_specifier(&base, "react", None),
            Err(ResolveError::Bare(_))
        ));
    }

    #[test]
    fn accepts_absolute_specifier() {
        let base = Url::parse("file:///page.html").unwrap();
        let resolved =
            resolve_specifier(&base, "data:text/javascript,export%20default%201", None).unwrap();
        assert!(resolved.scheme() == "data");
    }

    #[test]
    fn bare_specifier_uses_import_map() {
        let base = Url::parse("https://example.test/app/page.html").unwrap();
        let map = crate::js::import_map::parse_import_map_json(
            r#"{"imports":{"lit":"./lit.js"}}"#,
            base.as_str(),
        )
        .expect("map");
        let resolved = resolve_specifier(&base, "lit", Some(&map)).unwrap();
        assert_eq!(resolved.as_str(), "https://example.test/app/lit.js");
    }
}
