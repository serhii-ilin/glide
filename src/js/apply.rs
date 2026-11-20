//! Apply [`DomRequest`] values on the UI thread.

use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use blitz_dom::{BaseDocument, LocalName, QualName, local_name, qual_name};
use blitz_net::Provider;
use blitz_traits::net::Request;
use url::Url;

use super::bridge::{DomRequest, DomResponse, DomValue, node_to_handle, resolve_handle};
use crate::parse;

pub fn apply_request(
    doc: &mut BaseDocument,
    net: Option<&Arc<Provider>>,
    base_url: &str,
    req: DomRequest,
) -> DomResponse {
    let reply = reply_id(&req);
    let result = panic::catch_unwind(AssertUnwindSafe(|| apply_inner(doc, net, base_url, req)));
    match result {
        Ok(resp) => resp,
        Err(_) => DomResponse::err(reply, "dom operation panicked"),
    }
}

fn reply_id(req: &DomRequest) -> u64 {
    match req {
        DomRequest::GetElementById { reply, .. }
        | DomRequest::QuerySelector { reply, .. }
        | DomRequest::QuerySelectorAll { reply, .. }
        | DomRequest::CreateElement { reply, .. }
        | DomRequest::CreateTextNode { reply, .. }
        | DomRequest::AppendChild { reply, .. }
        | DomRequest::RemoveChild { reply, .. }
        | DomRequest::InsertBefore { reply, .. }
        | DomRequest::SetAttribute { reply, .. }
        | DomRequest::GetAttribute { reply, .. }
        | DomRequest::RemoveAttribute { reply, .. }
        | DomRequest::SetInnerHtml { reply, .. }
        | DomRequest::GetTextContent { reply, .. }
        | DomRequest::SetTextContent { reply, .. }
        | DomRequest::ClassListAdd { reply, .. }
        | DomRequest::ClassListRemove { reply, .. }
        | DomRequest::ClassListContains { reply, .. }
        | DomRequest::StyleSet { reply, .. }
        | DomRequest::ParentId { reply, .. }
        | DomRequest::Listen { reply, .. }
        | DomRequest::Fetch { reply, .. } => *reply,
    }
}

fn apply_inner(
    doc: &mut BaseDocument,
    net: Option<&Arc<Provider>>,
    base_url: &str,
    req: DomRequest,
) -> DomResponse {
    match req {
        DomRequest::GetElementById { id, reply } => {
            let value = doc
                .get_element_by_id(&id)
                .map(node_to_handle)
                .map(DomValue::NodeId)
                .unwrap_or(DomValue::Null);
            DomResponse::ok(reply, value)
        }
        DomRequest::QuerySelector { selector, reply } => {
            let value = match doc.query_selector(&selector) {
                Ok(Some(id)) => DomValue::NodeId(node_to_handle(id)),
                Ok(None) => DomValue::Null,
                Err(err) => DomValue::Error(format!("{err:?}")),
            };
            DomResponse::ok(reply, value)
        }
        DomRequest::QuerySelectorAll { selector, reply } => {
            let value = match doc.query_selector_all(&selector) {
                Ok(ids) => DomValue::NodeList(ids.into_iter().map(node_to_handle).collect()),
                Err(err) => DomValue::Error(format!("{err:?}")),
            };
            DomResponse::ok(reply, value)
        }
        DomRequest::CreateElement { tag, reply } => {
            let mut m = doc.mutate();
            let id = m.create_element(html_qual(&tag), Vec::new());
            DomResponse::ok(reply, DomValue::NodeId(node_to_handle(id)))
        }
        DomRequest::CreateTextNode { text, reply } => {
            let mut m = doc.mutate();
            let id = m.create_text_node(&text);
            DomResponse::ok(reply, DomValue::NodeId(node_to_handle(id)))
        }
        DomRequest::AppendChild {
            parent,
            child,
            reply,
        } => {
            let Some(parent) = resolve_handle(doc, parent) else {
                return DomResponse::err(reply, "invalid parent node");
            };
            let Some(child) = resolve_handle(doc, child) else {
                return DomResponse::err(reply, "invalid child node");
            };
            let mut m = doc.mutate();
            m.append_children(parent, &[child]);
            DomResponse::ok(reply, DomValue::NodeId(node_to_handle(child)))
        }
        DomRequest::RemoveChild {
            parent,
            child,
            reply,
        } => {
            let Some(parent) = resolve_handle(doc, parent) else {
                return DomResponse::err(reply, "invalid parent node");
            };
            let Some(child) = resolve_handle(doc, child) else {
                return DomResponse::err(reply, "invalid child node");
            };
            let mut m = doc.mutate();
            if m.parent_id(child) == Some(parent) {
                m.remove_node(child);
            }
            DomResponse::ok(reply, DomValue::Null)
        }
        DomRequest::InsertBefore {
            parent,
            child,
            before,
            reply,
        } => {
            let Some(parent) = resolve_handle(doc, parent) else {
                return DomResponse::err(reply, "invalid parent node");
            };
            let Some(child) = resolve_handle(doc, child) else {
                return DomResponse::err(reply, "invalid child node");
            };
            let before_id = if before == 0 {
                None
            } else {
                resolve_handle(doc, before)
            };
            let mut m = doc.mutate();
            match before_id {
                None => m.append_children(parent, &[child]),
                Some(anchor) => {
                    m.insert_nodes_before(anchor, &[child]);
                    if m.parent_id(child) != Some(parent) {
                        m.append_children(parent, &[child]);
                    }
                }
            }
            DomResponse::ok(reply, DomValue::NodeId(node_to_handle(child)))
        }
        DomRequest::SetAttribute {
            node,
            name,
            value,
            reply,
        } => {
            let Some(node) = resolve_handle(doc, node) else {
                return DomResponse::err(reply, "invalid node");
            };
            let mut m = doc.mutate();
            m.set_attribute(node, html_qual(&name), &value);
            DomResponse::ok(reply, DomValue::Null)
        }
        DomRequest::GetAttribute { node, name, reply } => {
            let Some(node) = resolve_handle(doc, node) else {
                return DomResponse::err(reply, "invalid node");
            };
            let value = doc
                .get_node(node)
                .and_then(|n| n.element_data())
                .and_then(|el| el.attr(LocalName::from(&*name)))
                .map(|v| DomValue::String(v.to_string()))
                .unwrap_or(DomValue::Null);
            DomResponse::ok(reply, value)
        }
        DomRequest::RemoveAttribute { node, name, reply } => {
            let Some(node) = resolve_handle(doc, node) else {
                return DomResponse::err(reply, "invalid node");
            };
            let mut m = doc.mutate();
            m.clear_attribute(node, html_qual(&name));
            DomResponse::ok(reply, DomValue::Null)
        }
        DomRequest::SetInnerHtml { node, html, reply } => {
            let Some(node) = resolve_handle(doc, node) else {
                return DomResponse::err(reply, "invalid node");
            };
            let mut m = doc.mutate();
            let children: Vec<_> = m.child_ids(node).into_iter().collect();
            for child in children {
                m.remove_and_drop_node(child);
            }
            parse::parse_inner_html_into_mutator(&mut m, node, &html, true);
            DomResponse::ok(reply, DomValue::Null)
        }
        DomRequest::GetTextContent { node, reply } => {
            let Some(node) = resolve_handle(doc, node) else {
                return DomResponse::err(reply, "invalid node");
            };
            let text = doc
                .get_node(node)
                .map(|n| n.text_content())
                .unwrap_or_default();
            DomResponse::ok(reply, DomValue::String(text))
        }
        DomRequest::SetTextContent { node, text, reply } => {
            let Some(node) = resolve_handle(doc, node) else {
                return DomResponse::err(reply, "invalid node");
            };
            let is_text = doc
                .get_node(node)
                .is_some_and(|n| matches!(n.data, blitz_dom::node::NodeData::Text(..)));
            let mut m = doc.mutate();
            if is_text {
                m.set_node_text(node, &text);
            } else {
                let children: Vec<_> = m.child_ids(node).into_iter().collect();
                for child in children {
                    m.remove_and_drop_node(child);
                }
                let text_id = m.create_text_node(&text);
                m.append_children(node, &[text_id]);
            }
            DomResponse::ok(reply, DomValue::Null)
        }
        DomRequest::ClassListAdd { node, token, reply } => {
            respond_class(op_add, doc, node, &token, reply)
        }
        DomRequest::ClassListRemove { node, token, reply } => {
            respond_class(op_remove, doc, node, &token, reply)
        }
        DomRequest::ClassListContains { node, token, reply } => {
            let Some(node) = resolve_handle(doc, node) else {
                return DomResponse::err(reply, "invalid node");
            };
            let contains = doc
                .get_node(node)
                .and_then(|n| n.element_data())
                .and_then(|el| el.attr(local_name!("class")))
                .is_some_and(|c| class_tokens(c).any(|t| t == token));
            DomResponse::ok(reply, DomValue::Bool(contains))
        }
        DomRequest::StyleSet {
            node,
            name,
            value,
            reply,
        } => {
            let Some(node) = resolve_handle(doc, node) else {
                return DomResponse::err(reply, "invalid node");
            };
            let css_name = camel_to_kebab(&name);
            let mut m = doc.mutate();
            m.set_style_property(node, &css_name, &value);
            DomResponse::ok(reply, DomValue::Null)
        }
        DomRequest::ParentId { node, reply } => {
            let Some(node) = resolve_handle(doc, node) else {
                return DomResponse::err(reply, "invalid node");
            };
            let value = doc
                .get_node(node)
                .and_then(|n| n.parent)
                .map(node_to_handle)
                .map(DomValue::NodeId)
                .unwrap_or(DomValue::Null);
            DomResponse::ok(reply, value)
        }
        DomRequest::Listen { reply } => DomResponse::ok(reply, DomValue::Null),
        DomRequest::Fetch { url, reply } => {
            if let Some(body) = data_url_body(&url) {
                return DomResponse::ok(reply, DomValue::String(body));
            }
            let Some(net) = net else {
                return DomResponse::err(reply, "no network provider");
            };
            let resolved = Url::parse(&url)
                .or_else(|_| Url::parse(base_url).and_then(|b| b.join(&url)))
                .map_err(|e| e.to_string());
            let Ok(parsed) = resolved else {
                return DomResponse::err(reply, "invalid url");
            };
            let Ok(handle) = tokio::runtime::Handle::try_current() else {
                return DomResponse::err(reply, "no async runtime");
            };
            let net = Arc::clone(net);
            let (tx, rx) = mpsc::sync_channel(1);
            std::thread::Builder::new()
                .name("glide-fetch".into())
                .spawn(move || {
                    let result = handle.block_on(async {
                        tokio::time::timeout(
                            Duration::from_secs(30),
                            net.fetch_async(Request::get(parsed)),
                        )
                        .await
                    });
                    let _ = tx.send(result);
                })
                .ok();
            let Ok(result) = rx.recv_timeout(Duration::from_secs(31)) else {
                return DomResponse::err(reply, "fetch timeout");
            };
            match result {
                Ok(Ok((_url, bytes))) => {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    DomResponse::ok(reply, DomValue::String(text))
                }
                Ok(Err(err)) => DomResponse::err(reply, format!("fetch failed: {err}")),
                Err(_) => DomResponse::err(reply, "fetch timed out"),
            }
        }
    }
}

fn html_qual(name: &str) -> QualName {
    QualName::new(None, html5ever::ns!(html), LocalName::from(name))
}

fn class_tokens(class_attr: &str) -> impl Iterator<Item = &str> {
    class_attr.split_whitespace()
}

fn read_class(doc: &BaseDocument, raw: u64) -> Option<String> {
    let node = resolve_handle(doc, raw)?;
    doc.get_node(node)
        .and_then(|n| n.element_data())
        .and_then(|el| el.attr(local_name!("class")))
        .map(str::to_string)
}

fn write_class(doc: &mut BaseDocument, raw: u64, value: &str) {
    let Some(node) = resolve_handle(doc, raw) else {
        return;
    };
    let mut m = doc.mutate();
    m.set_attribute(node, qual_name!("class"), value);
}

fn op_add(mut class: String, token: &str) -> String {
    if class_tokens(&class).any(|t| t == token) {
        return class;
    }
    if class.is_empty() {
        class = token.to_string();
    } else {
        class.push(' ');
        class.push_str(token);
    }
    class
}

fn op_remove(class: String, token: &str) -> String {
    let parts: Vec<_> = class_tokens(&class).filter(|t| *t != token).collect();
    parts.join(" ")
}

fn respond_class(
    op: fn(String, &str) -> String,
    doc: &mut BaseDocument,
    raw: u64,
    token: &str,
    reply: u64,
) -> DomResponse {
    let Some(_node) = resolve_handle(doc, raw) else {
        return DomResponse::err(reply, "invalid node");
    };
    let class = read_class(doc, raw).unwrap_or_default();
    write_class(doc, raw, &op(class, token));
    DomResponse::ok(reply, DomValue::Bool(true))
}

fn data_url_body(url: &str) -> Option<String> {
    let rest = url.strip_prefix("data:")?;
    let (meta, payload) = rest.split_once(',')?;
    if meta.contains(";base64") {
        return None;
    }
    Some(payload.to_string())
}

fn camel_to_kebab(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for ch in name.chars() {
        if ch.is_ascii_uppercase() {
            if !out.is_empty() {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use blitz_dom::DocumentConfig;

    fn sample_doc(html: &str) -> blitz_dom::BaseDocument {
        parse::document_from_html(html, DocumentConfig::default(), true)
    }

    #[test]
    fn dom_mutation_from_apply_commands() {
        let mut doc =
            sample_doc("<!DOCTYPE html><html><body><div id=\"root\"></div></body></html>");
        let base = doc.url().to_string();
        let root = apply_request(
            &mut doc,
            None,
            &base,
            DomRequest::GetElementById {
                id: "root".into(),
                reply: 1,
            },
        );
        let DomValue::NodeId(root) = root.value else {
            panic!("expected node");
        };
        let created = apply_request(
            &mut doc,
            None,
            &base,
            DomRequest::CreateElement {
                tag: "span".into(),
                reply: 2,
            },
        );
        let DomValue::NodeId(child) = created.value else {
            panic!("expected child");
        };
        apply_request(
            &mut doc,
            None,
            &base,
            DomRequest::AppendChild {
                parent: root,
                child,
                reply: 3,
            },
        );
        let text_node = apply_request(
            &mut doc,
            None,
            &base,
            DomRequest::CreateTextNode {
                text: "hello".into(),
                reply: 4,
            },
        );
        let DomValue::NodeId(text_id) = text_node.value else {
            panic!("expected text node");
        };
        apply_request(
            &mut doc,
            None,
            &base,
            DomRequest::AppendChild {
                parent: child,
                child: text_id,
                reply: 5,
            },
        );
        let text = apply_request(
            &mut doc,
            None,
            &base,
            DomRequest::GetTextContent {
                node: child,
                reply: 6,
            },
        );
        assert!(matches!(text.value, DomValue::String(ref s) if s == "hello"));
    }

    #[test]
    fn counter_fixture_ids() {
        let html = std::fs::read_to_string("tests/fixtures/counter.html").unwrap();
        let doc = sample_doc(&html);
        assert!(doc.get_element_by_id("count").is_some());
        assert!(doc.get_element_by_id("inc").is_some());
    }

    #[test]
    fn set_text_content_via_rpc() {
        let mut doc =
            sample_doc(r#"<!DOCTYPE html><html><body><p id="t">before</p></body></html>"#);
        let base = doc.url().to_string();
        let id = doc.get_element_by_id("t").unwrap();
        let handle = node_to_handle(id);
        apply_request(
            &mut doc,
            None,
            &base,
            DomRequest::SetTextContent {
                node: handle,
                text: "after".into(),
                reply: 1,
            },
        );
        let text = doc
            .get_node(id)
            .map(|n| n.text_content())
            .unwrap_or_default();
        assert_eq!(text, "after");
    }

    #[test]
    fn create_after_text_clear_preserves_generation() {
        let mut doc = sample_doc(r#"<!DOCTYPE html><html><body><p id="t">x</p></body></html>"#);
        let base = doc.url().to_string();
        let handle = node_to_handle(doc.get_element_by_id("t").unwrap());
        apply_request(
            &mut doc,
            None,
            &base,
            DomRequest::SetTextContent {
                node: handle,
                text: String::new(),
                reply: 1,
            },
        );
        let created = apply_request(
            &mut doc,
            None,
            &base,
            DomRequest::CreateElement {
                tag: "span".into(),
                reply: 2,
            },
        );
        let DomValue::NodeId(child) = created.value else {
            panic!("expected span");
        };
        let appended = apply_request(
            &mut doc,
            None,
            &base,
            DomRequest::AppendChild {
                parent: handle,
                child,
                reply: 3,
            },
        );
        assert!(!matches!(appended.value, DomValue::Error(_)));
    }

    #[test]
    fn camel_case_style_maps_to_css() {
        assert_eq!(camel_to_kebab("backgroundColor"), "background-color");
    }
}
