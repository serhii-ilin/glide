//! Serialize a Blitz page document for the domino snapshot bootstrap.

use blitz_dom::BaseDocument;
use blitz_dom::node::NodeData;
use blitz_traits::node_id::NodeId;
use serde_json::{Value, json};

use super::bridge::node_to_handle;

/// Nested array: `[blitzId, nodeType, nameOrText, attrs|null, children]`.
pub fn snapshot_tree(doc: &BaseDocument) -> Value {
    let root = doc.root_node().id;
    snapshot_node(doc, root)
}

fn snapshot_node(doc: &BaseDocument, id: NodeId) -> Value {
    let Some(node) = doc.get_node(id) else {
        return json!([node_to_handle(id), 0, "", null, []]);
    };
    let handle = node_to_handle(id);
    match &node.data {
        NodeData::Document(_) => {
            let children: Vec<Value> = node
                .children
                .iter()
                .map(|c| snapshot_node(doc, *c))
                .collect();
            json!([handle, 9, "#document", null, children, null])
        }
        NodeData::Element(el) | NodeData::AnonymousBlock(el) => {
            let tag = el.name.local.to_string();
            let attrs: Vec<Value> = el
                .attrs
                .iter()
                .map(|a| {
                    json!([
                        a.name.local.to_string(),
                        a.value.to_string(),
                        a.name.ns.to_string()
                    ])
                })
                .collect();
            let children: Vec<Value> = node
                .children
                .iter()
                .map(|c| snapshot_node(doc, *c))
                .collect();
            json!([handle, 1, tag, attrs, children, el.name.ns.to_string()])
        }
        NodeData::Text(data) => json!([handle, 3, data.content, null, [], null]),
        NodeData::Comment { contents } => json!([handle, 8, contents, null, [], null]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use blitz_dom::DocumentConfig;

    #[test]
    fn snapshot_shape_matches_contract() {
        let doc = parse::document_from_html(
            "<!DOCTYPE html><html><head><title>t</title></head>\
             <body><p id=\"x\">hi</p></body></html>",
            DocumentConfig::default(),
            true,
        );
        let tree = snapshot_tree(&doc);
        let arr = tree.as_array().expect("root array");
        assert_eq!(arr[1], 9);
        let children = arr[4].as_array().expect("children");
        assert!(!children.is_empty());
        let html = &children[0];
        assert_eq!(html.as_array().unwrap()[1], 1);
        assert_eq!(html.as_array().unwrap()[2], "html");
    }

    #[test]
    fn snapshot_includes_text_and_attrs() {
        let doc = parse::document_from_html(
            r#"<!DOCTYPE html><html><body><span class="a" data-v="1">z</span></body></html>"#,
            DocumentConfig::default(),
            true,
        );
        let tree = snapshot_tree(&doc);
        let body = find_tag(&tree, "body").expect("body");
        let span = body[4].as_array().unwrap()[0].as_array().unwrap();
        assert_eq!(span[2], "span");
        let attrs = span[3].as_array().unwrap();
        assert!(attrs.iter().any(|a| a[0] == "class" && a[1] == "a"));
        let text = span[4].as_array().unwrap()[0].as_array().unwrap();
        assert_eq!(text[1], 3);
        assert_eq!(text[2], "z");
    }

    #[test]
    fn snapshot_keeps_svg_namespace() {
        let doc = parse::document_from_html(
            "<!DOCTYPE html><html><body><svg><circle></circle></svg></body></html>",
            DocumentConfig::default(),
            true,
        );
        let tree = snapshot_tree(&doc);
        let svg = find_tag(&tree, "svg").expect("svg");
        let arr = svg.as_array().unwrap();
        assert_eq!(arr[5], "http://www.w3.org/2000/svg");
        let circle = find_tag(svg, "circle").expect("circle");
        assert_eq!(circle.as_array().unwrap()[5], "http://www.w3.org/2000/svg");
    }

    fn find_tag<'a>(v: &'a Value, tag: &str) -> Option<&'a Value> {
        let arr = v.as_array()?;
        if arr.len() >= 5 && arr[1] == 1 && arr[2] == tag {
            return Some(v);
        }
        for child in arr.get(4)?.as_array()? {
            if let Some(found) = find_tag(child, tag) {
                return Some(found);
            }
        }
        None
    }
}
