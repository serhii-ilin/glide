//! Discover `<script>` elements and order them for classic/defer/async semantics (post-parse).

use blitz_dom::{BaseDocument, local_name};
use blitz_net::Provider;
use blitz_traits::net::Request;
use blitz_traits::node_id::NodeId;
use crossbeam_channel::{Receiver, unbounded};
use std::sync::Arc;
use std::thread;
use tokio::runtime::Handle;
use url::Url;

use super::diagnostics;
use crate::logging;

#[derive(Debug, Clone)]
pub struct ScriptSpec {
    pub inline: Option<String>,
    pub src: Option<String>,
    pub defer: bool,
    pub r#async: bool,
    pub element: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum ScriptSource {
    Inline(String),
    External(String),
    Failed,
}

/// Depth-first document order.
pub fn collect_scripts(doc: &BaseDocument) -> Vec<ScriptSpec> {
    let mut out = Vec::new();
    let root = doc.root_node().id;
    walk(doc, root, &mut out);
    out
}

fn walk(doc: &BaseDocument, node: NodeId, out: &mut Vec<ScriptSpec>) {
    let children: Vec<_> = doc
        .get_node(node)
        .map(|n| n.children.iter().copied().collect())
        .unwrap_or_default();
    for child in children {
        if let Some(el) = doc.get_node(child).and_then(|n| n.element_data())
            && el.name.local == local_name!("script")
        {
            if let Some(kind) = el.attr(local_name!("type")).map(str::to_lowercase) {
                if kind == "application/ld+json" || kind == "text/json" {
                    continue;
                }
                if kind != "text/javascript"
                    && kind != "application/javascript"
                    && kind != "module"
                    && !kind.is_empty()
                {
                    continue;
                }
                if kind == "module" {
                    continue;
                }
            }
            let defer = el.attr(local_name!("defer")).is_some();
            let async_attr = el.attr(local_name!("async")).is_some();
            let src = el
                .attr(local_name!("src"))
                .map(str::to_string)
                .filter(|s| !s.is_empty());
            let defer = src.is_some() && defer;
            let async_attr = src.is_some() && async_attr;
            let inline = if src.is_none() {
                Some(
                    doc.get_node(child)
                        .map(|n| n.text_content())
                        .unwrap_or_default(),
                )
            } else {
                None
            };
            out.push(ScriptSpec {
                inline,
                src,
                defer,
                r#async: async_attr,
                element: Some(super::bridge::node_to_handle(child)),
            });
        }
        walk(doc, child, out);
    }
}

/// Execution batches: classic (sync), defer, then async indices.
pub fn execution_plan(scripts: &[ScriptSpec]) -> (Vec<usize>, Vec<usize>, Vec<usize>) {
    let mut classic = Vec::new();
    let mut defer = Vec::new();
    let mut async_scripts = Vec::new();
    for (i, s) in scripts.iter().enumerate() {
        if s.r#async {
            async_scripts.push(i);
        } else if s.defer {
            defer.push(i);
        } else {
            classic.push(i);
        }
    }
    (classic, defer, async_scripts)
}

pub fn spawn_resolve_sources(
    scripts: Vec<ScriptSpec>,
    net: Arc<Provider>,
    base_url: String,
    rt: Handle,
) -> Receiver<Vec<ScriptSource>> {
    let (tx, rx) = unbounded();
    thread::Builder::new()
        .name("glide-script-fetch".into())
        .spawn(move || {
            let out = resolve_sources(&scripts, &net, &base_url, &rt);
            let _ = tx.send(out);
        })
        .ok();
    rx
}

pub fn resolve_sources(
    scripts: &[ScriptSpec],
    net: &Arc<Provider>,
    base_url: &str,
    rt: &Handle,
) -> Vec<ScriptSource> {
    scripts
        .iter()
        .map(|s| {
            if let Some(body) = &s.inline {
                return ScriptSource::Inline(body.clone());
            }
            let src = s.src.as_deref().unwrap_or("");
            let parsed =
                match Url::parse(src).or_else(|_| Url::parse(base_url).and_then(|b| b.join(src))) {
                    Ok(u) => u,
                    Err(err) => {
                        log_fetch_failure(base_url, src, err.to_string(), None);
                        return ScriptSource::Failed;
                    }
                };
            if parsed.scheme() == "http" || parsed.scheme() == "https" {
                return match rt.block_on(crate::http::fetch_followed(Request::get(parsed))) {
                    Ok(resp) if (200..300).contains(&resp.status) => {
                        ScriptSource::External(String::from_utf8_lossy(&resp.body).into_owned())
                    }
                    Ok(resp) => {
                        log_fetch_failure(
                            base_url,
                            src,
                            "non-success status".into(),
                            Some(resp.status),
                        );
                        ScriptSource::Failed
                    }
                    Err(err) => {
                        log_fetch_failure(base_url, src, err.to_string(), None);
                        ScriptSource::Failed
                    }
                };
            }
            match rt.block_on(net.fetch_async(Request::get(parsed))) {
                Ok((_final_url, bytes)) => {
                    ScriptSource::External(String::from_utf8_lossy(&bytes).into_owned())
                }
                Err(err) => {
                    log_fetch_failure(base_url, src, err.to_string(), None);
                    ScriptSource::Failed
                }
            }
        })
        .collect()
}

fn log_fetch_failure(base_url: &str, src: &str, detail: String, status: Option<u16>) {
    let kind = match status {
        Some(_) => "http_status".to_string(),
        None => diagnostics::classify(&detail).as_str().to_string(),
    };
    let mut fields = vec![
        ("page_url", base_url.to_string()),
        ("script_src", src.to_string()),
        ("error", detail),
        ("error_kind", kind),
    ];
    if let Some(status) = status {
        fields.push(("status", status.to_string()));
    }
    fields.push((
        "hint",
        "The script was skipped; check the script URL and its response.".to_string(),
    ));
    logging::error("js.loader", "script fetch failed", &fields);
}

pub fn source_body(source: &ScriptSource) -> Option<String> {
    match source {
        ScriptSource::Inline(s) => Some(s.clone()),
        ScriptSource::External(body) => Some(body.clone()),
        ScriptSource::Failed => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blitz_dom::DocumentConfig;
    use blitz_html::HtmlDocument;

    #[test]
    fn script_ordering_defer_and_async() {
        let html = r#"<!DOCTYPE html><html><body>
            <script>window.a=1</script>
            <script defer src="b.js"></script>
            <script async src="c.js"></script>
            <script>window.d=4</script>
        </body></html>"#;
        let doc = HtmlDocument::from_html(html, DocumentConfig::default());
        let scripts = collect_scripts(&doc);
        assert_eq!(scripts.len(), 4);
        let (classic, defer, async_scripts) = execution_plan(&scripts);
        assert_eq!(classic, vec![0, 3]);
        assert_eq!(defer, vec![1]);
        assert_eq!(async_scripts, vec![2]);
    }

    #[test]
    fn finds_inline_scripts_in_fixture() {
        let html = std::fs::read_to_string("tests/fixtures/counter.html").unwrap();
        let doc = crate::parse::document_from_html(&html, DocumentConfig::default(), true);
        let scripts = collect_scripts(&doc);
        assert_eq!(scripts.len(), 1);
        assert!(
            scripts[0]
                .inline
                .as_ref()
                .is_some_and(|s| s.contains("addEventListener"))
        );
    }
}
