//! Discover `<script>` elements and order them for classic/defer/async semantics (post-parse).

use blitz_dom::{BaseDocument, local_name};
use blitz_net::Provider;
use blitz_traits::net::Request;
use blitz_traits::node_id::NodeId;
use std::sync::Arc;
use url::Url;

#[derive(Debug, Clone)]
pub struct ScriptSpec {
    pub inline: Option<String>,
    pub src: Option<String>,
    pub defer: bool,
    pub r#async: bool,
    pub node: NodeId,
}

#[derive(Debug, Clone)]
pub enum ScriptSource {
    Inline(String),
    External { url: String, body: String },
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
        if let Some(el) = doc.get_node(child).and_then(|n| n.element_data()) {
            if el.name.local == local_name!("script") {
                let defer = el.attr(local_name!("defer")).is_some();
                let async_attr = el.attr(local_name!("async")).is_some();
                let src = el
                    .attr(local_name!("src"))
                    .map(str::to_string)
                    .filter(|s| !s.is_empty());
                let inline = if src.is_none() {
                    Some(doc.get_node(child).map(|n| n.text_content()).unwrap_or_default())
                } else {
                    None
                };
                out.push(ScriptSpec {
                    inline,
                    src,
                    defer,
                    r#async: async_attr,
                    node: child,
                });
            }
        }
        walk(doc, child, out);
    }
}

/// Execution batches after the document is parsed.
pub fn execution_plan(scripts: &[ScriptSpec]) -> Vec<Vec<usize>> {
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
    let mut plan = vec![classic, defer, async_scripts];
    plan.retain(|b| !b.is_empty());
    plan
}

pub fn resolve_sources(
    scripts: &[ScriptSpec],
    net: &Arc<Provider>,
    base_url: &str,
    rt: &tokio::runtime::Handle,
) -> Vec<ScriptSource> {
    scripts
        .iter()
        .map(|s| {
            if let Some(body) = &s.inline {
                return ScriptSource::Inline(body.clone());
            }
            let src = s.src.as_deref().unwrap_or("");
            let parsed = Url::parse(src)
                .or_else(|_| Url::parse(base_url).and_then(|b| b.join(src)))
                .expect("script url");
            let (_final_url, bytes) = rt
                .block_on(net.fetch_async(Request::get(parsed)))
                .expect("script fetch");
            ScriptSource::External {
                url: src.to_string(),
                body: String::from_utf8_lossy(&bytes).into_owned(),
            }
        })
        .collect()
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
        let plan = execution_plan(&scripts);
        assert_eq!(plan[0], vec![0, 3]); // classic inline scripts
        assert_eq!(plan[1], vec![1]); // defer
        assert_eq!(plan[2], vec![2]); // async
    }

    #[test]
    fn finds_inline_scripts_in_fixture() {
        let html = std::fs::read_to_string("tests/fixtures/counter.html").unwrap();
        let doc = crate::parse::document_from_html(
            &html,
            DocumentConfig::default(),
            true,
        );
        let scripts = collect_scripts(&doc);
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].inline.as_ref().is_some_and(|s| s.contains("addEventListener")));
    }
}