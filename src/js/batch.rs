//! Apply batched domino → Blitz mutations from the JS thread.
//!
//! One batch is one task (script, timer, rAF, event, or a layout read). Client ids
//! (`cN`) name nodes created earlier in the same batch. Nodes that are removed and
//! not moved again in the batch are dropped from the slab.

use std::collections::{HashMap, HashSet};

use blitz_dom::node::NodeData;
use blitz_dom::{BaseDocument, LocalName, QualName, ns};
use blitz_traits::node_id::NodeId;
use serde::Deserialize;
use serde_json::json;

use super::bridge::{DomResponse, DomValue, node_to_handle, resolve_handle};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum NodeRef {
    Id(u64),
    Cid(String),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AttrSpec {
    Pair(String, String),
    Namespaced(String, String, String),
}

impl AttrSpec {
    fn parts(&self) -> (&str, &str, Option<&str>) {
        match self {
            Self::Pair(name, value) => (name, value, None),
            Self::Namespaced(name, value, ns) => {
                let ns = if ns.is_empty() {
                    None
                } else {
                    Some(ns.as_str())
                };
                (name, value, ns)
            }
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "t")]
enum BatchOp {
    #[serde(rename = "insert")]
    Insert {
        parent: NodeRef,
        #[serde(default)]
        before: Option<NodeRef>,
        cid: String,
        kind: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        ns: Option<String>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        attrs: Vec<AttrSpec>,
    },
    #[serde(rename = "remove")]
    Remove { id: NodeRef },
    #[serde(rename = "attr")]
    Attr {
        id: NodeRef,
        name: String,
        value: Option<String>,
        #[serde(default)]
        ns: Option<String>,
    },
    #[serde(rename = "text")]
    Text { id: NodeRef, value: String },
    #[serde(rename = "move")]
    Move {
        id: NodeRef,
        parent: NodeRef,
        #[serde(default)]
        before: Option<NodeRef>,
    },
}

pub fn apply_batch(doc: &mut BaseDocument, payload: &str, reply: u64) -> DomResponse {
    let ops: Vec<BatchOp> = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("[js] batch json: {err}");
            return DomResponse::ok(reply, DomValue::Json(json!({ "ids": {} })));
        }
    };
    let (keep_ids, keep_cids) = moved_refs(&ops);
    let mut cid_map: HashMap<String, NodeId> = HashMap::new();
    let mut new_ids = serde_json::Map::new();
    let mut logged = false;
    for op in ops {
        if let Err(err) = apply_op(doc, op, &mut cid_map, &keep_ids, &keep_cids)
            && !logged
        {
            logged = true;
            eprintln!("[js] batch op skipped: {err}");
        }
    }
    for (cid, id) in cid_map {
        if doc.get_node(id).is_some() {
            new_ids.insert(cid, json!(node_to_handle(id)));
        }
    }
    DomResponse::ok(reply, DomValue::Json(json!({ "ids": new_ids })))
}

fn moved_refs(ops: &[BatchOp]) -> (HashSet<u64>, HashSet<String>) {
    let mut ids = HashSet::new();
    let mut cids = HashSet::new();
    for op in ops {
        if let BatchOp::Move { id, .. } = op {
            match id {
                NodeRef::Id(n) => {
                    ids.insert(*n);
                }
                NodeRef::Cid(c) => {
                    cids.insert(c.clone());
                }
            }
        }
    }
    (ids, cids)
}

fn apply_op(
    doc: &mut BaseDocument,
    op: BatchOp,
    cid_map: &mut HashMap<String, NodeId>,
    keep_ids: &HashSet<u64>,
    keep_cids: &HashSet<String>,
) -> Result<(), String> {
    match op {
        BatchOp::Insert {
            parent,
            before,
            cid,
            kind,
            name,
            ns,
            text,
            attrs,
        } => {
            let parent_id = resolve_ref(doc, cid_map, &parent)
                .ok_or_else(|| "insert: bad parent".to_string())?;
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
                    let id = m.create_element(element_qual(tag, ns.as_deref()), Vec::new());
                    for attr in &attrs {
                        let (n, v, ans) = attr.parts();
                        m.set_attribute(id, attr_qual(n, ans), v);
                    }
                    id
                }
            };
            let before_id = before.as_ref().and_then(|r| resolve_ref(doc, cid_map, r));
            insert_under(doc, parent_id, id, before_id);
            cid_map.insert(cid, id);
            Ok(())
        }
        BatchOp::Remove { id } => {
            let keep = match &id {
                NodeRef::Id(n) => keep_ids.contains(n),
                NodeRef::Cid(c) => keep_cids.contains(c),
            };
            let Some(node) = resolve_ref(doc, cid_map, &id) else {
                return Ok(());
            };
            let mut m = doc.mutate();
            if keep {
                m.remove_node(node);
            } else {
                m.remove_and_drop_node(node);
            }
            Ok(())
        }
        BatchOp::Attr {
            id,
            name,
            value,
            ns,
        } => {
            let Some(node) = resolve_ref(doc, cid_map, &id) else {
                return Ok(());
            };
            let mut m = doc.mutate();
            match value {
                Some(v) => m.set_attribute(node, attr_qual(&name, ns.as_deref()), &v),
                None => m.clear_attribute(node, attr_qual(&name, ns.as_deref())),
            }
            Ok(())
        }
        BatchOp::Text { id, value } => {
            let Some(node) = resolve_ref(doc, cid_map, &id) else {
                return Ok(());
            };
            match doc.get_node(node).map(|n| &n.data) {
                Some(NodeData::Text(_)) => {
                    let mut m = doc.mutate();
                    m.set_node_text(node, &value);
                }
                Some(NodeData::Comment { .. }) => {
                    if let Some(n) = doc.get_node_mut(node)
                        && let NodeData::Comment { contents } = &mut n.data
                    {
                        contents.clear();
                        contents.push_str(&value);
                    }
                }
                Some(_) => {
                    let children: Vec<NodeId> = doc
                        .get_node(node)
                        .map(|n| n.children.iter().copied().collect())
                        .unwrap_or_default();
                    let mut m = doc.mutate();
                    for child in children {
                        let raw = node_to_handle(child);
                        if keep_ids.contains(&raw) {
                            m.remove_node(child);
                        } else {
                            m.remove_and_drop_node(child);
                        }
                    }
                    if !value.is_empty() {
                        let text_id = m.create_text_node(&value);
                        m.append_children(node, &[text_id]);
                    }
                }
                None => {}
            }
            Ok(())
        }
        BatchOp::Move { id, parent, before } => {
            let Some(node) = resolve_ref(doc, cid_map, &id) else {
                return Ok(());
            };
            let Some(parent_id) = resolve_ref(doc, cid_map, &parent) else {
                return Ok(());
            };
            {
                let mut m = doc.mutate();
                m.remove_node(node);
            }
            let before_id = before.as_ref().and_then(|r| resolve_ref(doc, cid_map, r));
            insert_under(doc, parent_id, node, before_id);
            Ok(())
        }
    }
}

fn resolve_ref(
    doc: &BaseDocument,
    cid_map: &HashMap<String, NodeId>,
    node_ref: &NodeRef,
) -> Option<NodeId> {
    match node_ref {
        NodeRef::Id(raw) => resolve_handle(doc, *raw),
        NodeRef::Cid(cid) => cid_map
            .get(cid)
            .copied()
            .filter(|id| doc.get_node(*id).is_some()),
    }
}

fn insert_under(doc: &mut BaseDocument, parent: NodeId, child: NodeId, before: Option<NodeId>) {
    let anchor = before.filter(|id| doc.get_node(*id).and_then(|n| n.parent).is_some());
    let mut m = doc.mutate();
    match anchor {
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

fn element_qual(local: &str, ns_url: Option<&str>) -> QualName {
    let namespace = match ns_url.unwrap_or("") {
        "http://www.w3.org/2000/svg" => ns!(svg),
        "http://www.w3.org/1998/Math/MathML" => ns!(mathml),
        "http://www.w3.org/XML/1998/namespace" => ns!(xml),
        "http://www.w3.org/2000/xmlns/" => ns!(xmlns),
        "http://www.w3.org/1999/xlink" => ns!(xlink),
        _ => ns!(html),
    };
    QualName::new(None, namespace, LocalName::from(local))
}

fn attr_qual(local: &str, ns_url: Option<&str>) -> QualName {
    match ns_url.unwrap_or("") {
        "" | "http://www.w3.org/1999/xhtml" => {
            QualName::new(None, ns!(html), LocalName::from(local))
        }
        other => element_qual(local, Some(other)),
    }
}

/// Count live nodes whose slotmap version is still 1.
///
/// Fresh nodes and nodes that were detached with `remove_node` (never dropped)
/// keep version 1 and a new index. `remove_and_drop_node` frees the slot, so a
/// later occupant has a higher version and is not counted. The difference after
/// a text-replacement loop is the leak.
pub fn live_node_count(doc: &BaseDocument) -> usize {
    let mut count = 0usize;
    for index in 0..16_384u32 {
        let raw = (1u64 << 32) | u64::from(index);
        if doc.get_node(NodeId::from_u64(raw)).is_some() {
            count += 1;
        }
    }
    count
}

pub fn reply_bounding_rect(doc: &mut BaseDocument, raw: u64, reply: u64) -> DomResponse {
    let id = super::bridge::handle_to_node(raw);
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
        return DomResponse::ok(reply, DomValue::String(String::new()));
    };
    doc.resolve(0.0);
    let kebab = super::apply::camel_to_kebab(prop);
    let value = doc.resolved_style_value(id, &kebab);
    DomResponse::ok(reply, DomValue::String(value))
}

pub fn reply_layout_number(
    doc: &mut BaseDocument,
    raw: u64,
    field: &str,
    reply: u64,
) -> DomResponse {
    let id = super::bridge::handle_to_node(raw);
    doc.resolve(0.0);
    let offset = doc.offset_rect(id);
    let border = doc.get_client_bounding_rect(id);
    let n = match field {
        "offsetWidth" => offset.map(|r| r.width).or_else(|| border.map(|r| r.width)),
        "offsetHeight" => offset
            .map(|r| r.height)
            .or_else(|| border.map(|r| r.height)),
        "offsetLeft" => offset.map(|r| r.x),
        "offsetTop" => offset.map(|r| r.y),
        // Border box from layout. CSS `clientWidth` is the padding box; Blitz
        // does not expose that edge separately.
        "clientWidth" => border.map(|r| r.width),
        "clientHeight" => border.map(|r| r.height),
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
    use blitz_dom::{local_name, ns};

    fn sample(html: &str) -> BaseDocument {
        parse::document_from_html(html, DocumentConfig::default(), true)
    }

    fn ids_of(resp: &DomResponse) -> serde_json::Value {
        let DomValue::Json(v) = &resp.value else {
            panic!("expected json, got {:?}", resp.value);
        };
        v["ids"].clone()
    }

    #[test]
    fn batch_insert_remove_attr_text() {
        let mut doc = sample("<!DOCTYPE html><html><body><div id=\"p\"></div></body></html>");
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
        let child_h = ids_of(&resp)["c1"].as_u64().unwrap();
        let batch2 = json!([
            { "t": "attr", "id": child_h, "name": "data-x", "value": "1" },
            { "t": "text", "id": child_h, "value": "hi" }
        ])
        .to_string();
        apply_batch(&mut doc, &batch2, 2);
        let child = super::super::bridge::handle_to_node(child_h);
        assert_eq!(
            doc.get_node(child)
                .map(|n| n.text_content())
                .unwrap_or_default(),
            "hi"
        );
        let before = live_node_count(&doc);
        let batch3 = json!([{ "t": "remove", "id": child_h }]).to_string();
        apply_batch(&mut doc, &batch3, 3);
        assert!(
            doc.get_node(parent)
                .map(|n| n.children.is_empty())
                .unwrap_or(false)
        );
        assert!(
            doc.get_node(child).is_none(),
            "removed node stays in the slab"
        );
        assert!(live_node_count(&doc) <= before);
    }

    #[test]
    fn batch_nested_insert_uses_each_parent() {
        let mut doc = sample("<!DOCTYPE html><html><body><div id=\"p\"></div></body></html>");
        let parent_h = node_to_handle(doc.get_element_by_id("p").unwrap());
        let batch = json!([
            {
                "t": "insert",
                "parent": parent_h,
                "cid": "outer",
                "kind": "element",
                "name": "div",
                "attrs": [["id", "outer"]]
            },
            {
                "t": "insert",
                "parent": "outer",
                "cid": "inner",
                "kind": "element",
                "name": "div",
                "attrs": [["id", "in"]]
            }
        ])
        .to_string();
        let resp = apply_batch(&mut doc, &batch, 1);
        let outer = super::super::bridge::handle_to_node(ids_of(&resp)["outer"].as_u64().unwrap());
        let inner = super::super::bridge::handle_to_node(ids_of(&resp)["inner"].as_u64().unwrap());
        assert_eq!(doc.get_node(inner).and_then(|n| n.parent), Some(outer));
        assert_eq!(
            doc.get_node(outer).and_then(|n| n.parent),
            Some(doc.get_element_by_id("p").unwrap())
        );
    }

    #[test]
    fn batch_insert_before_keeps_order() {
        let mut doc = sample(
            "<!DOCTYPE html><html><body><div id=\"p\"><b id=\"out\">Z</b></div></body></html>",
        );
        let parent_h = node_to_handle(doc.get_element_by_id("p").unwrap());
        let out_h = node_to_handle(doc.get_element_by_id("out").unwrap());
        let batch = json!([
            {
                "t": "insert",
                "parent": parent_h,
                "before": out_h,
                "cid": "c1",
                "kind": "element",
                "name": "b",
                "attrs": [["id", "a"]]
            },
            {
                "t": "insert",
                "parent": parent_h,
                "before": "c1",
                "cid": "c2",
                "kind": "element",
                "name": "b",
                "attrs": [["id", "b"]]
            }
        ])
        .to_string();
        apply_batch(&mut doc, &batch, 1);
        let parent = doc.get_element_by_id("p").unwrap();
        let names: Vec<String> = doc
            .get_node(parent)
            .unwrap()
            .children
            .iter()
            .filter_map(|id| doc.get_node(*id).and_then(|n| n.element_data()))
            .map(|el| el.name.local.to_string())
            .collect();
        let ids: Vec<String> = doc
            .get_node(parent)
            .unwrap()
            .children
            .iter()
            .filter_map(|id| {
                doc.get_node(*id)
                    .and_then(|n| n.element_data())
                    .and_then(|el| el.attr(local_name!("id")))
                    .map(str::to_string)
            })
            .collect();
        assert_eq!(names.len(), 3, "children: {ids:?}");
        assert_eq!(ids, vec!["b", "a", "out"]);
    }

    #[test]
    fn batch_move_reparents_without_drop() {
        let mut doc = sample(
            "<!DOCTYPE html><html><body><div id=\"p\"></div><div id=\"q\"></div></body></html>",
        );
        let p = node_to_handle(doc.get_element_by_id("p").unwrap());
        let q = node_to_handle(doc.get_element_by_id("q").unwrap());
        let created = apply_batch(
            &mut doc,
            &json!([{
                "t": "insert",
                "parent": p,
                "cid": "c1",
                "kind": "element",
                "name": "span",
                "attrs": [["id", "k"]]
            }])
            .to_string(),
            1,
        );
        let child_h = ids_of(&created)["c1"].as_u64().unwrap();
        let before = live_node_count(&doc);
        apply_batch(
            &mut doc,
            &json!([{ "t": "move", "id": child_h, "parent": q }]).to_string(),
            2,
        );
        let child = super::super::bridge::handle_to_node(child_h);
        assert!(doc.get_node(child).is_some(), "move dropped the node");
        assert_eq!(
            doc.get_node(child).and_then(|n| n.parent),
            Some(doc.get_element_by_id("q").unwrap())
        );
        assert_eq!(live_node_count(&doc), before);
    }

    #[test]
    fn text_replacement_does_not_grow_the_slab() {
        let mut doc = sample("<!DOCTYPE html><html><body><div id=\"p\"></div></body></html>");
        let parent_h = node_to_handle(doc.get_element_by_id("p").unwrap());
        let start = live_node_count(&doc);
        assert!(start > 3, "node scan saw {start}");
        for i in 0..40 {
            let batch =
                json!([{ "t": "text", "id": parent_h, "value": format!("n{i}") }]).to_string();
            apply_batch(&mut doc, &batch, i + 1);
        }
        let end = live_node_count(&doc);
        assert!(
            end <= start + 2,
            "textContent leaked nodes: {start} -> {end}"
        );
        let parent = doc.get_element_by_id("p").unwrap();
        assert_eq!(
            doc.get_node(parent)
                .map(|n| n.text_content())
                .unwrap_or_default(),
            "n39"
        );
    }

    #[test]
    fn invalid_op_does_not_abort_the_batch() {
        let mut doc = sample("<!DOCTYPE html><html><body><div id=\"p\"></div></body></html>");
        let parent_h = node_to_handle(doc.get_element_by_id("p").unwrap());
        let batch = json!([
            {
                "t": "insert",
                "parent": 9_999_999_999u64,
                "cid": "bad",
                "kind": "element",
                "name": "div"
            },
            {
                "t": "insert",
                "parent": parent_h,
                "cid": "ok",
                "kind": "element",
                "name": "span",
                "attrs": [["id", "ok"]]
            }
        ])
        .to_string();
        let resp = apply_batch(&mut doc, &batch, 1);
        assert!(!matches!(resp.value, DomValue::Error(_)));
        assert!(doc.get_element_by_id("ok").is_some());
    }

    #[test]
    fn insert_keeps_svg_namespace() {
        let mut doc = sample("<!DOCTYPE html><html><body><div id=\"p\"></div></body></html>");
        let parent_h = node_to_handle(doc.get_element_by_id("p").unwrap());
        let batch = json!([{
            "t": "insert",
            "parent": parent_h,
            "cid": "c1",
            "kind": "element",
            "name": "svg",
            "ns": "http://www.w3.org/2000/svg"
        }])
        .to_string();
        let resp = apply_batch(&mut doc, &batch, 1);
        let id = super::super::bridge::handle_to_node(ids_of(&resp)["c1"].as_u64().unwrap());
        let ns = doc
            .get_node(id)
            .and_then(|n| n.element_data())
            .map(|el| el.name.ns.clone())
            .unwrap();
        assert_eq!(ns, ns!(svg));
    }
}
