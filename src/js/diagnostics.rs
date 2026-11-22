//! Classify page-script failures and carry page/script context into V8 callbacks.
//!
//! The V8 message listener runs on the `glide-js` thread, so the current page URL and
//! script identity are kept in a thread-local set by the worker before each execution.

use std::cell::RefCell;

use crate::logging::{self, Level};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    MissingApi,
    IncompatibleDescriptor,
    InvalidUrl,
    DomRpc,
    ScriptTimeout,
    Other,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingApi => "missing_api",
            Self::IncompatibleDescriptor => "incompatible_descriptor",
            Self::InvalidUrl => "invalid_url",
            Self::DomRpc => "dom_rpc",
            Self::ScriptTimeout => "script_timeout",
            Self::Other => "other",
        }
    }
}

/// Best-effort root-cause classification for a V8 or console error message.
pub fn classify(message: &str) -> ErrorKind {
    let lower = message.to_ascii_lowercase();
    if lower.contains("cannot redefine property")
        || lower.contains("cannot assign to read only property")
    {
        ErrorKind::IncompatibleDescriptor
    } else if lower.contains("invalid url")
        || lower.contains("invalid base")
        || lower.contains("relative url without a base")
    {
        ErrorKind::InvalidUrl
    } else if lower.contains("dom rpc")
        || lower.contains("dom channel closed")
        || lower.contains("unknown dom op")
    {
        ErrorKind::DomRpc
    } else if lower.contains("timed out") || lower.contains("terminated") {
        ErrorKind::ScriptTimeout
    } else if lower.contains("is not defined")
        || lower.contains("is not a function")
        || lower.contains("is not a constructor")
        || lower.contains("reading 'getitem'")
        || lower.contains("reading \"getitem\"")
        || lower.contains("reading 'setitem'")
        || lower.contains("reading \"setitem\"")
        || lower.contains("reading 'navigationstart'")
        || lower.contains("reading \"navigationstart\"")
        || lower.contains("right-hand side of 'instanceof' is not an object")
        || lower.contains("right-hand side of \"instanceof\" is not an object")
    {
        ErrorKind::MissingApi
    } else {
        ErrorKind::Other
    }
}

/// A concrete hint where one is known, otherwise a hint for the error class.
pub fn hint(kind: ErrorKind, message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("atob") || lower.contains("btoa") {
        "Base64 helpers are bootstrap shims; check the input alphabet and padding."
    } else if lower.contains("mutationobserver") {
        "Glide installs a no-op MutationObserver shim; real observation is tracked by F-038."
    } else if lower.contains("matchmedia") {
        "matchMedia is a shim that always returns matches=false; real media evaluation is not wired."
    } else if lower.contains("console.info") || lower.contains("console.debug") {
        "console.info and console.debug are provided by the bootstrap console shim."
    } else if lower.contains("localstorage")
        || lower.contains("sessionstorage")
        || lower.contains("getitem")
        || lower.contains("setitem")
    {
        "Web Storage is an in-memory shim; values do not survive a reload or restart."
    } else if lower.contains("abortcontroller") || lower.contains("abortsignal") {
        "AbortController is a signal-only shim; it does not cancel fetch or other host work."
    } else if lower.contains("right-hand side of 'instanceof' is not an object")
        || lower.contains("right-hand side of \"instanceof\" is not an object")
    {
        "A constructor used with instanceof is undefined; expose the missing interface (for example DOMStringMap) or guard with typeof first."
    } else if lower.contains("navigationstart") || lower.contains("performance.timing") {
        "performance.timing/navigationStart is not implemented; use performance.now() where possible."
    } else if lower.contains("reading 'src'") || lower.contains("reading \"src\"") {
        "A DOM element or event target was undefined; check currentScript, event.target and img.src before use."
    } else if lower.contains("webworker") || lower.contains("worker was not initialized") {
        "Web Workers are not supported by Glide; third-party analytics may only degrade."
    } else if lower.contains("innerHTML") {
        "Domino defines innerHTML as a non-configurable prototype property; assign to it instead of redefining it."
    } else {
        match kind {
            ErrorKind::MissingApi => {
                "A Web API used by the page is absent; check src/js/bootstrap.js and F-032."
            }
            ErrorKind::IncompatibleDescriptor => {
                "A script tried to redefine a non-configurable DOM property."
            }
            ErrorKind::InvalidUrl => {
                "A URL was resolved with a missing or malformed base; check URL(relative, base)."
            }
            ErrorKind::DomRpc => {
                "The page DOM bridge failed; check the glide-js and UI thread logs."
            }
            ErrorKind::ScriptTimeout => {
                "The script watchdog terminated execution after five seconds."
            }
            ErrorKind::Other => {
                "No specific hint; use the stack and page context to locate the call."
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScriptContext {
    pub page_url: Option<String>,
    pub script_name: Option<String>,
    pub script_node: Option<u64>,
    pub script_kind: Option<String>,
}

thread_local! {
    static CURRENT: RefCell<ScriptContext> = const {
        RefCell::new(ScriptContext {
            page_url: None,
            script_name: None,
            script_node: None,
            script_kind: None,
        })
    };
}

/// Per-page failure tally emitted as one `js.page` record once the load settles.
#[derive(Debug, Clone, Default)]
pub struct PageCounts {
    pub uncaught: u64,
    pub console_errors: u64,
    pub unsupported: u64,
    pub kinds: std::collections::BTreeSet<String>,
}

thread_local! {
    static COUNTS: RefCell<PageCounts> = RefCell::new(PageCounts::default());
}

pub fn reset_counts() {
    COUNTS.with(|slot| *slot.borrow_mut() = PageCounts::default());
}

pub fn counts() -> PageCounts {
    COUNTS.with(|slot| slot.borrow().clone())
}

fn count_kind(kind: &str) {
    COUNTS.with(|slot| {
        let mut counts = slot.borrow_mut();
        counts.kinds.insert(kind.to_string());
    });
}

pub fn set_page_url(url: impl Into<String>) {
    COUNTS.with(|slot| *slot.borrow_mut() = PageCounts::default());
    CURRENT.with(|slot| slot.borrow_mut().page_url = Some(url.into()));
}

pub fn set_script(name: Option<&str>, node: Option<u64>, kind: Option<&str>) {
    CURRENT.with(|slot| {
        let mut ctx = slot.borrow_mut();
        ctx.script_name = name.map(str::to_string);
        ctx.script_node = node;
        ctx.script_kind = kind.map(str::to_string);
    });
}

pub fn clear_script() {
    CURRENT.with(|slot| {
        let mut ctx = slot.borrow_mut();
        ctx.script_name = None;
        ctx.script_kind = None;
        ctx.script_node = None;
    });
}

pub fn current() -> ScriptContext {
    CURRENT.with(|slot| slot.borrow().clone())
}

pub(crate) fn uncaught_fields(
    message: &str,
    line: Option<usize>,
    resource: Option<&str>,
    stack: &str,
    ctx: &ScriptContext,
) -> Vec<(&'static str, String)> {
    let kind = classify(message);
    let mut fields = context_fields(ctx);
    if let Some(line) = line {
        fields.push(("line", line.to_string()));
    }
    if let Some(resource) = resource
        && !resource.is_empty()
    {
        fields.push(("source", resource.to_string()));
    }
    if !stack.is_empty() {
        fields.push(("stack", stack.to_string()));
    }
    fields.push(("error_kind", kind.as_str().to_string()));
    fields.push(("hint", hint(kind, message).to_string()));
    fields
}

pub fn record_uncaught(message: &str, line: Option<usize>, resource: Option<&str>, stack: &str) {
    let ctx = current();
    let message = if message.starts_with("Uncaught ") {
        message.to_string()
    } else {
        format!("Uncaught {message}")
    };
    let kind = classify(&message);
    COUNTS.with(|slot| slot.borrow_mut().uncaught += 1);
    count_kind(kind.as_str());
    let fields = uncaught_fields(&message, line, resource, stack, &ctx);
    logging::error("js.uncaught", message, &fields);
}

pub fn record_console(level: Level, message: &str) {
    let ctx = current();
    let mut fields = context_fields(&ctx);
    if matches!(level, Level::Error | Level::Warn) {
        let kind = classify(message);
        fields.push(("error_kind", kind.as_str().to_string()));
        fields.push(("hint", hint(kind, message).to_string()));
        if level == Level::Error {
            COUNTS.with(|slot| slot.borrow_mut().console_errors += 1);
            count_kind(kind.as_str());
        }
    }
    logging::emit(level, "js.console", message, &fields);
}

/// A page feature Glide does not implement yet, reported by the DOM glue.
///
/// These are not exceptions, so they never reach `js.uncaught`; without this record
/// a page can stay blank while every script "succeeds".
pub fn record_unsupported(feature: &str, detail: &str) {
    COUNTS.with(|slot| slot.borrow_mut().unsupported += 1);
    count_kind("unsupported");
    let mut fields = context_fields(&current());
    fields.extend(unsupported_fields(feature, detail));
    logging::warn("js.unsupported", "page feature is not implemented", &fields);
}

/// Report an unsupported feature from the UI thread, where there is no script
/// context. Used by script discovery (module scripts).
pub fn report_unsupported(page_url: &str, feature: &str, detail: &str) {
    let mut fields = vec![("page_url", page_url.to_string())];
    fields.extend(unsupported_fields(feature, detail));
    logging::warn("js.unsupported", "page feature is not implemented", &fields);
}

fn unsupported_fields(feature: &str, detail: &str) -> Vec<(&'static str, String)> {
    vec![
        ("error_kind", "unsupported".to_string()),
        ("feature", feature.to_string()),
        ("detail", truncate_detail(detail)),
        ("hint", unsupported_hint(feature).to_string()),
    ]
}

/// Page-provided text (script URLs, inline markers) is capped so one record stays
/// one bounded line, even for `data:` URLs.
fn truncate_detail(detail: &str) -> String {
    const LIMIT: usize = 160;
    let mut out: String = detail.chars().take(LIMIT).collect();
    if detail.chars().count() > LIMIT {
        out.push('…');
    }
    out
}

/// Capabilities the page probed and did not find, as one debug record per page.
///
/// Page apps feature-detect these and silently take a fallback path when one is
/// missing, so the absence never appears as an exception.
pub fn record_capabilities(missing: &str) {
    if missing.is_empty() {
        return;
    }
    let mut fields = context_fields(&current());
    fields.push(("missing", missing.to_string()));
    fields.push((
        "hint",
        "Feature detection on these globals fails silently; implement the ones the page needs (F-032)."
            .to_string(),
    ));
    logging::debug("js.caps", "page capabilities", &fields);
}

fn unsupported_hint(feature: &str) -> &'static str {
    match feature {
        "dynamic-script" => {
            "Scripts inserted after parsing are mirrored into Blitz but not executed (F-036)."
        }
        "module-script" => "ES modules are parsed but never executed by the page runtime.",
        "dynamic-import" => "import() is not wired to the script loader.",
        _ => "A page feature reached the DOM bridge with no implementation; see F-032/F-036.",
    }
}

/// One record per load once scripts settle, so a blank page still has a trace.
///
/// Degraded pages log at INFO so they surface with the default filter; a clean
/// page logs at DEBUG, where it is diagnosis material rather than a warning.
pub fn log_page_summary(scripts_run: u64) {
    let counts = counts();
    let degraded = counts.uncaught > 0 || counts.console_errors > 0 || counts.unsupported > 0;
    if scripts_run == 0 && !degraded {
        return;
    }
    let mut fields = context_fields(&current());
    fields.push(("scripts", scripts_run.to_string()));
    fields.push(("uncaught", counts.uncaught.to_string()));
    fields.push(("console_errors", counts.console_errors.to_string()));
    fields.push(("unsupported", counts.unsupported.to_string()));
    if !counts.kinds.is_empty() {
        fields.push((
            "error_kinds",
            counts.kinds.iter().cloned().collect::<Vec<_>>().join(","),
        ));
    }
    fields.push((
        "status",
        if degraded { "degraded" } else { "clean" }.to_string(),
    ));
    fields.push((
        "hint",
        if degraded {
            "Use GLIDE_LOG=debug for each script; js.unsupported lists missing DOM features."
        } else {
            "No script errors. If the page is blank, its app may rely on a missing DOM feature or network API."
        }
        .to_string(),
    ));
    let level = if degraded { Level::Info } else { Level::Debug };
    logging::emit(level, "js.page", "page scripts settled", &fields);
}

fn context_fields(ctx: &ScriptContext) -> Vec<(&'static str, String)> {
    let mut fields = vec![(
        "page_url",
        ctx.page_url.clone().unwrap_or_else(|| "unknown".into()),
    )];
    if let Some(script) = &ctx.script_name {
        fields.push(("script", script.clone()));
    }
    if let Some(node) = ctx.script_node {
        fields.push(("script_node", node.to_string()));
    }
    if let Some(kind) = &ctx.script_kind {
        fields.push(("script_kind", kind.clone()));
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_counts_tally_uncaught_console_and_unsupported() {
        set_page_url("file:///page.html");
        record_uncaught("TypeError: boom", None, None, "");
        record_console(Level::Error, "Error: console boom");
        record_console(Level::Warn, "warn only");
        record_unsupported("dynamic-script", "<inline>");

        let counts = counts();
        assert_eq!(counts.uncaught, 1);
        assert_eq!(counts.console_errors, 1);
        assert_eq!(counts.unsupported, 1);
        assert!(counts.kinds.contains("unsupported"));
        assert!(counts.kinds.contains("other"));
    }

    #[test]
    fn classify_recognizes_known_root_causes() {
        assert_eq!(
            classify("ReferenceError: atob is not defined"),
            ErrorKind::MissingApi
        );
        assert_eq!(
            classify("TypeError: window.matchMedia is not a function"),
            ErrorKind::MissingApi
        );
        assert_eq!(
            classify("TypeError: Cannot redefine property: innerHTML"),
            ErrorKind::IncompatibleDescriptor
        );
        assert_eq!(classify("TypeError: Invalid URL"), ErrorKind::InvalidUrl);
        assert_eq!(
            classify("TypeError: Right-hand side of 'instanceof' is not an object"),
            ErrorKind::MissingApi
        );
        assert_eq!(classify("dom rpc timeout"), ErrorKind::DomRpc);
        assert_eq!(classify("unexpected failure"), ErrorKind::Other);
    }

    #[test]
    fn hint_names_the_missing_api_and_its_limit() {
        let missing_hint = hint(
            ErrorKind::MissingApi,
            "ReferenceError: MutationObserver is not defined",
        );
        assert!(missing_hint.contains("no-op"));
        assert!(missing_hint.contains("F-038"));

        let descriptor_hint = hint(
            ErrorKind::IncompatibleDescriptor,
            "TypeError: Cannot redefine property: innerHTML",
        );
        assert!(descriptor_hint.contains("non-configurable"));

        let perf_hint = hint(
            ErrorKind::MissingApi,
            "TypeError: Cannot read properties of undefined (reading 'navigationStart')",
        );
        assert!(perf_hint.contains("performance.now()"));
    }

    #[test]
    fn uncaught_fields_include_page_script_and_cause() {
        let ctx = ScriptContext {
            page_url: Some("file:///page.html".into()),
            script_name: Some("script-14".into()),
            script_node: Some(42),
            script_kind: Some("classic".into()),
        };
        let message = "Uncaught ReferenceError: atob is not defined";
        let fields = uncaught_fields(message, Some(31879), Some("script-14"), "frame", &ctx);
        let get = |key| {
            fields
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(get("page_url"), Some("file:///page.html"));
        assert_eq!(get("script"), Some("script-14"));
        assert_eq!(get("script_node"), Some("42"));
        assert_eq!(get("script_kind"), Some("classic"));
        assert_eq!(get("line"), Some("31879"));
        assert_eq!(get("error_kind"), Some("missing_api"));
        assert!(get("hint").is_some_and(|h| h.contains("Base64")));
    }

    #[test]
    fn set_and_clear_script_context_is_thread_local() {
        set_page_url("file:///other.html");
        set_script(Some("script-0"), Some(7), Some("defer"));
        let ctx = current();
        assert_eq!(ctx.page_url.as_deref(), Some("file:///other.html"));
        assert_eq!(ctx.script_name.as_deref(), Some("script-0"));
        assert_eq!(ctx.script_node, Some(7));
        clear_script();
        let ctx = current();
        assert_eq!(ctx.page_url.as_deref(), Some("file:///other.html"));
        assert!(ctx.script_name.is_none());
        assert!(ctx.script_node.is_none());
    }
}
