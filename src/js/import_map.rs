//! Parse, merge, and resolve HTML `<script type="importmap">` JSON (F-084).

use serde_json::Value;
use std::collections::BTreeMap;
use url::Url;

/// One merged import map for the active document.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImportMapRegistry {
    imports: BTreeMap<String, String>,
    scopes: BTreeMap<String, ScopeMap>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct ScopeMap {
    imports: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportMapError {
    pub kind: &'static str,
    pub detail: String,
}

impl ImportMapRegistry {
    pub const fn empty() -> Self {
        Self {
            imports: BTreeMap::new(),
            scopes: BTreeMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.imports.is_empty() && self.scopes.is_empty()
    }

    /// Merge a parsed map into this registry (HTML import map merge rules).
    pub fn merge(&mut self, other: ImportMapRegistry) {
        for (key, value) in other.imports {
            if value.is_empty() {
                self.imports.remove(&key);
            } else {
                self.imports.insert(key, value);
            }
        }
        for (scope_key, scope_map) in other.scopes {
            let entry = self.scopes.entry(scope_key).or_default();
            for (key, value) in scope_map.imports {
                if value.is_empty() {
                    entry.imports.remove(&key);
                } else {
                    entry.imports.insert(key, value);
                }
            }
        }
    }

    /// Resolve a bare module specifier using scopes then imports.
    pub fn resolve_bare(&self, referrer: &Url, specifier: &str) -> Option<Url> {
        let spec = specifier.trim();
        let referrer_str = referrer.as_str();
        let mut scope_keys: Vec<&String> = self.scopes.keys().collect();
        scope_keys.sort_by_key(|k| std::cmp::Reverse(k.len()));
        for scope_prefix in scope_keys {
            if !referrer_str.starts_with(scope_prefix.as_str()) {
                continue;
            }
            let scope = &self.scopes[scope_prefix];
            if let Some(url) = resolve_in_map(&scope.imports, spec) {
                return Some(url);
            }
        }
        resolve_in_map(&self.imports, spec)
    }
}

fn resolve_in_map(imports: &BTreeMap<String, String>, specifier: &str) -> Option<Url> {
    if let Some(url) = imports.get(specifier) {
        return Url::parse(url).ok();
    }
    let mut prefix_keys: Vec<&String> = imports.keys().filter(|k| k.ends_with('/')).collect();
    prefix_keys.sort_by_key(|k| std::cmp::Reverse(k.len()));
    for prefix in prefix_keys {
        if specifier.starts_with(prefix.as_str()) {
            let suffix = &specifier[prefix.len()..];
            let mapped = imports.get(prefix)?;
            let joined = if mapped.ends_with('/') {
                format!("{mapped}{suffix}")
            } else {
                format!("{mapped}/{suffix}")
            };
            return Url::parse(&joined).ok();
        }
    }
    None
}

/// Parse JSON text and normalize URLs against `base_url`.
pub fn parse_import_map_json(
    json: &str,
    base_url: &str,
) -> Result<ImportMapRegistry, ImportMapError> {
    let value: Value = serde_json::from_str(json).map_err(|err| ImportMapError {
        kind: "json",
        detail: err.to_string(),
    })?;
    let object = value.as_object().ok_or_else(|| ImportMapError {
        kind: "invalid",
        detail: "top-level JSON value must be an object".into(),
    })?;
    for key in object.keys() {
        if key != "imports" && key != "scopes" {
            return Err(ImportMapError {
                kind: "invalid",
                detail: format!("unknown top-level key {key:?}"),
            });
        }
    }
    let base = Url::parse(base_url).map_err(|err| ImportMapError {
        kind: "invalid",
        detail: format!("base URL invalid: {err}"),
    })?;
    let mut registry = ImportMapRegistry::default();
    if let Some(imports) = object.get("imports") {
        registry.imports = parse_imports_object(imports, &base)?;
    }
    if let Some(scopes) = object.get("scopes") {
        registry.scopes = parse_scopes_object(scopes, &base)?;
    }
    Ok(registry)
}

fn parse_scopes_object(
    value: &Value,
    base: &Url,
) -> Result<BTreeMap<String, ScopeMap>, ImportMapError> {
    let object = value.as_object().ok_or_else(|| ImportMapError {
        kind: "invalid",
        detail: "scopes must be an object".into(),
    })?;
    let mut out = BTreeMap::new();
    for (scope_key, scope_value) in object {
        let scope_prefix = normalize_scope_prefix(scope_key, base)?;
        let imports = parse_imports_object(scope_value, base)?;
        if !imports.is_empty() {
            out.insert(scope_prefix, ScopeMap { imports });
        }
    }
    Ok(out)
}

fn parse_imports_object(
    value: &Value,
    base: &Url,
) -> Result<BTreeMap<String, String>, ImportMapError> {
    let object = value.as_object().ok_or_else(|| ImportMapError {
        kind: "invalid",
        detail: "imports must be an object".into(),
    })?;
    let mut out = BTreeMap::new();
    for (specifier, mapped) in object {
        validate_specifier_key(specifier)?;
        if mapped.is_null() {
            out.insert(specifier.clone(), String::new());
            continue;
        }
        let (address, integrity) = parse_mapping_value(mapped)?;
        if let Some(integrity) = integrity {
            log_integrity_ignored(specifier, integrity);
        }
        let url = normalize_address(&address, base)?;
        out.insert(specifier.clone(), url);
    }
    Ok(out)
}

fn parse_mapping_value(value: &Value) -> Result<(String, Option<String>), ImportMapError> {
    if let Some(s) = value.as_str() {
        return Ok((s.to_string(), None));
    }
    let object = value.as_object().ok_or_else(|| ImportMapError {
        kind: "invalid",
        detail: "import map entry must be a string or object".into(),
    })?;
    let target = object
        .get("target")
        .or_else(|| object.get("address"))
        .and_then(Value::as_str)
        .ok_or_else(|| ImportMapError {
            kind: "invalid",
            detail: "import map object entry requires target or address".into(),
        })?;
    let integrity = object
        .get("integrity")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok((target.to_string(), integrity))
}

fn validate_specifier_key(key: &str) -> Result<(), ImportMapError> {
    if key.is_empty() {
        return Err(ImportMapError {
            kind: "invalid",
            detail: "import map key must not be empty".into(),
        });
    }
    if key.contains('\\') {
        return Err(ImportMapError {
            kind: "invalid",
            detail: format!("invalid specifier key {key:?}"),
        });
    }
    if key.starts_with("://") {
        return Err(ImportMapError {
            kind: "invalid",
            detail: format!("invalid specifier key {key:?}"),
        });
    }
    // URL-like keys must parse as absolute URLs; bare keys must not contain spaces.
    if key.contains("://") || key.starts_with('/') {
        Url::parse(key).map_err(|_| ImportMapError {
            kind: "invalid",
            detail: format!("invalid URL specifier key {key:?}"),
        })?;
        return Ok(());
    }
    if key.contains(char::is_whitespace) {
        return Err(ImportMapError {
            kind: "invalid",
            detail: format!("invalid specifier key {key:?}"),
        });
    }
    Ok(())
}

fn normalize_scope_prefix(prefix: &str, base: &Url) -> Result<String, ImportMapError> {
    let resolved = normalize_address(prefix, base)?;
    Ok(resolved)
}

fn normalize_address(address: &str, base: &Url) -> Result<String, ImportMapError> {
    let trimmed = address.trim();
    if trimmed.is_empty() {
        return Err(ImportMapError {
            kind: "invalid",
            detail: "address must not be empty".into(),
        });
    }
    if let Ok(abs) = Url::parse(trimmed) {
        return Ok(abs.to_string());
    }
    base.join(trimmed)
        .map(|u| u.to_string())
        .map_err(|err| ImportMapError {
            kind: "invalid",
            detail: err.to_string(),
        })
}

fn log_integrity_ignored(specifier: &str, integrity: String) {
    crate::logging::warn(
        "js.loader",
        "import map integrity ignored",
        &[
            (
                "page_url",
                crate::js::diagnostics::current()
                    .page_url
                    .clone()
                    .unwrap_or_else(|| "unknown".into()),
            ),
            ("script", specifier.to_string()),
            ("detail", format!("integrity={integrity}")),
            (
                "hint",
                "Subresource integrity on import maps is not enforced (F-084).".into(),
            ),
        ],
    );
}

/// Structured log when an import map script fails to register.
pub fn log_import_map_failure(page_url: &str, detail: String) {
    crate::logging::error(
        "js.loader",
        "import map registration failed",
        &[
            ("page_url", page_url.to_string()),
            ("error_kind", "invalid".into()),
            ("error", detail.clone()),
            (
                "hint",
                "The import map script was ignored; fix the JSON or specifier keys.".into(),
            ),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://example.test/app/page.html";

    #[test]
    fn parses_simple_imports() {
        let json = r#"{"imports":{"lit":"./vendor/lit.js"}}"#;
        let map = parse_import_map_json(json, BASE).expect("parse");
        let referrer = Url::parse(BASE).unwrap();
        let resolved = map.resolve_bare(&referrer, "lit").unwrap();
        assert_eq!(resolved.as_str(), "https://example.test/app/vendor/lit.js");
    }

    #[test]
    fn longest_prefix_match_for_trailing_slash() {
        let json = r#"{"imports":{"pkg/":"./pkgs/generic/","pkg/foo/":"./pkgs/foo/"}}"#;
        let map = parse_import_map_json(json, BASE).expect("parse");
        let referrer = Url::parse(BASE).unwrap();
        let hit = map.resolve_bare(&referrer, "pkg/foo/bar.js").unwrap();
        assert_eq!(hit.as_str(), "https://example.test/app/pkgs/foo/bar.js");
        let other = map.resolve_bare(&referrer, "pkg/bar.js").unwrap();
        assert_eq!(
            other.as_str(),
            "https://example.test/app/pkgs/generic/bar.js"
        );
    }

    #[test]
    fn scope_prefix_prefers_longest_match() {
        let json = r#"{
          "imports": {"root": "./root.js"},
          "scopes": {
            "https://example.test/app/": {"scoped": "./a.js"},
            "https://example.test/app/nested/": {"scoped": "./nested/b.js"}
          }
        }"#;
        let map = parse_import_map_json(json, BASE).expect("parse");
        let in_nested = Url::parse("https://example.test/app/nested/page.html").unwrap();
        let in_app = Url::parse("https://example.test/app/other.html").unwrap();
        assert_eq!(
            map.resolve_bare(&in_nested, "scoped").unwrap().as_str(),
            "https://example.test/app/nested/b.js"
        );
        assert_eq!(
            map.resolve_bare(&in_app, "scoped").unwrap().as_str(),
            "https://example.test/app/a.js"
        );
    }

    #[test]
    fn merge_overrides_and_deletes_keys() {
        let first = parse_import_map_json(r#"{"imports":{"a":"./1.js","b":"./2.js"}}"#, BASE)
            .expect("first");
        let second =
            parse_import_map_json(r#"{"imports":{"a":"./9.js","b":null}}"#, BASE).expect("second");
        let mut merged = first;
        merged.merge(second);
        let referrer = Url::parse(BASE).unwrap();
        assert_eq!(
            merged.resolve_bare(&referrer, "a").unwrap().as_str(),
            "https://example.test/app/9.js"
        );
        assert!(merged.resolve_bare(&referrer, "b").is_none());
    }

    #[test]
    fn rejects_invalid_json() {
        let err = parse_import_map_json("{", BASE).unwrap_err();
        assert_eq!(err.kind, "json");
    }

    #[test]
    fn rejects_unknown_top_level_key() {
        let err = parse_import_map_json(r#"{"imports":{},"meta":true}"#, BASE).unwrap_err();
        assert_eq!(err.kind, "invalid");
    }

    #[test]
    fn unknown_bare_specifier_stays_unresolved() {
        let map = parse_import_map_json(r#"{"imports":{"lit":"./lit.js"}}"#, BASE).expect("parse");
        let referrer = Url::parse(BASE).unwrap();
        assert!(map.resolve_bare(&referrer, "missing").is_none());
    }

    #[test]
    fn object_entry_with_integrity_parses_target() {
        let json = r#"{"imports":{"pkg":{"target":"./p.js","integrity":"sha384-abc"}}}"#;
        let map = parse_import_map_json(json, BASE).expect("parse");
        let referrer = Url::parse(BASE).unwrap();
        assert_eq!(
            map.resolve_bare(&referrer, "pkg").unwrap().as_str(),
            "https://example.test/app/p.js"
        );
    }
}
