//! Apply batched domino → Blitz mutations from the JS thread.

use blitz_dom::node::NodeData;
use blitz_dom::{BaseDocument, local_name};
use blitz_traits::node_id::NodeId;
use serde::Deserialize;
use serde_json::json;

use super::bridge::{DomResponse, DomValue, handle_to_node, node_to_handle, resolve_handle};
#[derive(Debug, Deserialize)]
#[serde(tag = "t")]
enum BatchOp {
    #[serde(rename = "insert")]
    Insert {
        parent: u64,
        #[serde(default)]
        before: Option<u64>,
        cid: String,
        kind: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        attrs: Vec<(String, String)>,
    },
    #[serde(rename = "remove")]
    Remove { id: u64 },
    #[serde(rename = "attr")]
    Attr {
        id: u64,
        name: String,
        value: Option<String>,
    },
    #[serde(rename = "text")]
    Text { id: u64, value: String },
    #[serde(rename = "move")]
    Move {
        id: u64,
        parent: u64,
        #[serde(default)]
        before: Option<u64>,
    },
}

pub fn apply_batch(doc: &mut BaseDocument, payload: &str, reply: u64) -> DomResponse {
    let ops: Vec<BatchOp> = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(err) => return DomResponse::err(reply, format!("batch json: {err}")),
    };
    let mut new_ids = serde_json::Map::new();
    let mut created: Vec<(String, NodeId)> = Vec::new();
    for op in ops {
        match op {
            BatchOp::Insert {
                parent,
                before,
                cid,
                kind,
                name,
                text,
                attrs,
            } => {
                let Some(parent_id) = resolve_handle(doc, parent) else {
                    return DomResponse::err(reply, "batch insert: bad parent");
                };
                let id = match kind.as_str() {
                    "text" => {
                        let mut m = doc.mutate();
                        m.create_text_node(text.as_deref().unwrap_or(""))
                    }
                    "comment" => {
                        let mut m = doc.mutate();
                        m.create_comment_node(text.as_deref().unwrap_or(""))
                    }
                    _ => {
                        let tag = name.as_deref().unwrap_or("div");
                        let mut m = doc.mutate();
                        let id = m.create_element(super::apply::html_qual(tag), Vec::new());
                        for (n, v) in attrs {
                            m.set_attribute(id, super::apply::html_qual(&n), &v);
                        }
                        id
                    }
                };
                insert_under(doc, parent_id, id, before);
                created.push((cid, id));
            }
            BatchOp::Remove { id } => {
                let Some(node) = resolve_handle(doc, id) else {
                    continue;
                };
                let mut m = doc.mutate();
                m.remove_node(node);
            }
            BatchOp::Attr { id, name, value } => {
                let Some(node) = resolve_handle(doc, id) else {
                    continue;
                };
                let mut m = doc.mutate();
                match value {
                    Some(v) => {
                        m.set_attribute(node, super::apply::html_qual(&name), &v);
                    }
                    None => {
                        m.clear_attribute(node, super::apply::html_qual(&name));
                    }
                }
            }
            BatchOp::Text { id, value } => {
                let Some(node) = resolve_handle(doc, id) else {
                    continue;
                };
                let is_text = doc
                    .get_node(node)
                    .is_some_and(|n| matches!(n.data, NodeData::Text(..)));
                let mut m = doc.mutate();
                if is_text {
                    m.set_node_text(node, &value);
                } else {
                    let text_id = m.create_text_node(&value);
                    for child in m.child_ids(node) {
                        m.remove_node(child);
                    }
                    m.append_children(node, &[text_id]);
                }
            }
            BatchOp::Move { id, parent, before } => {
                let Some(node) = resolve_handle(doc, id) else {
                    continue;
                };
                let Some(parent_id) = resolve_handle(doc, parent) else {
                    continue;
                };
                let mut m = doc.mutate();
                m.remove_node(node);
                drop(m);
                insert_under(doc, parent_id, node, before);
            }
        }
    }
    for (cid, id) in created {
        new_ids.insert(cid, json!(node_to_handle(id)));
    }
    DomResponse::ok(reply, DomValue::Json(json!({ "ids": new_ids })))
}

fn insert_under(doc: &mut BaseDocument, parent: NodeId, child: NodeId, before: Option<u64>) {
    let before_id = before.and_then(|raw| resolve_handle(doc, raw));
    let mut m = doc.mutate();
    match before_id {
        None => m.append_children(parent, &[child]),
        Some(anchor) => {
            m.insert_nodes_before(anchor, &[child]);
            if m.parent_id(child) != Some(parent) {
                m.remove_node(child);
                m.append_children(parent, &[child]);
            }
        }
    }
}

pub fn reply_bounding_rect(doc: &mut BaseDocument, raw: u64, reply: u64) -> DomResponse {
    let id = handle_to_node(raw);
    doc.resolve(0.0);
    let rect = doc.get_client_bounding_rect(id);
    let obj = match rect {
        Some(r) => json!({
            "x": r.x,
            "y": r.y,
            "width": r.width,
            "height": r.height,
            "top": r.y,
            "left": r.x,
            "right": r.x + r.width,
            "bottom": r.y + r.height,
        }),
        None => json!({
            "x": 0, "y": 0, "width": 0, "height": 0,
            "top": 0, "left": 0, "right": 0, "bottom": 0,
        }),
    };
    DomResponse::ok(reply, DomValue::Json(obj))
}

pub fn reply_computed_style(
    doc: &mut BaseDocument,
    raw: u64,
    prop: &str,
    reply: u64,
) -> DomResponse {
    let Some(id) = resolve_handle(doc, raw) else {
        return DomResponse::err(reply, "invalid node");
    };
    doc.resolve(0.0);
    let kebab = super::apply::camel_to_kebab(prop);
    let value = doc
        .get_node(id)
        .and_then(|n| n.element_data())
        .and_then(|el| el.attr(local_name!("style")))
        .and_then(|style| parse_inline_style_prop(style, &kebab))
        .unwrap_or_default();
    DomResponse::ok(reply, DomValue::String(value))
}

fn parse_inline_style_prop(style: &str, prop: &str) -> Option<String> {
    for part in style.split(';') {
        let mut kv = part.splitn(2, ':');
        let name = kv.next()?.trim();
        let val = kv.next()?.trim();
        if name.eq_ignore_ascii_case(prop) {
            return Some(val.to_string());
        }
    }
    None
}

pub fn reply_layout_number(
    doc: &mut BaseDocument,
    raw: u64,
    field: &str,
    reply: u64,
) -> DomResponse {
    let id = handle_to_node(raw);
    doc.resolve(0.0);
    let rect = doc.get_client_bounding_rect(id);
    let n = match field {
        "offsetWidth" | "offsetHeight" => rect.map(|r| {
            if field == "offsetWidth" {
                r.width
            } else {
                r.height
            }
        }),
        "clientWidth" | "clientHeight" => rect.map(|r| {
            if field == "clientWidth" {
                r.width
            } else {
                r.height
            }
        }),
        "offsetTop" | "offsetLeft" => Some(0.0),
        _ => None,
    }
    .unwrap_or(0.0);
    DomResponse::ok(reply, DomValue::Number(n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use blitz_dom::DocumentConfig;

    fn sample() -> BaseDocument {
        parse::document_from_html(
            "<!DOCTYPE html><html><body><div id=\"p\"></div></body></html>",
            DocumentConfig::default(),
            true,
        )
    }

    #[test]
    fn batch_insert_remove_attr_text() {
        let mut doc = sample();
        let parent = doc.get_element_by_id("p").unwrap();
        let parent_h = node_to_handle(parent);
        let batch = json!([
            {
                "t": "insert",
                "parent": parent_h,
                "cid": "c1",
                "kind": "element",
                "name": "span",
                "attrs": [["id", "s"]]
            }
        ])
        .to_string();
        let resp = apply_batch(&mut doc, &batch, 1);
        let DomValue::Json(v) = resp.value else {
            panic!("json");
        };
        let child_h = v["ids"]["c1"].as_u64().unwrap();
        let batch2 = json!([
            { "t": "attr", "id": child_h, "name": "data-x", "value": "1" },
            { "t": "text", "id": child_h, "value": "hi" }
        ])
        .to_string();
        apply_batch(&mut doc, &batch2, 2);
        let child = handle_to_node(child_h);
        assert_eq!(
            doc.get_node(child)
                .map(|n| n.text_content())
                .unwrap_or_default(),
            "hi"
        );
        let batch3 = json!([{ "t": "remove", "id": child_h }]).to_string();
        apply_batch(&mut doc, &batch3, 3);
        assert!(
            doc.get_node(parent)
                .map(|n| n.children.is_empty())
                .unwrap_or(false)
        );
    }
}
