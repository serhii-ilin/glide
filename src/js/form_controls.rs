//! Blitz form control state for the domino mirror (F-039).

use blitz_dom::traversal::TreeTraverser;
use blitz_dom::{BaseDocument, local_name};
use blitz_traits::node_id::NodeId;

use super::bridge::{DomResponse, DomValue};

/// Read a control property from Blitz (`value` or `checked`).
pub fn reply_control_field(doc: &BaseDocument, raw: u64, field: &str, reply: u64) -> DomResponse {
    let Some(id) = super::bridge::resolve_handle(doc, raw) else {
        return DomResponse::ok(reply, DomValue::Null);
    };
    let Some(node) = doc.get_node(id) else {
        return DomResponse::ok(reply, DomValue::Null);
    };
    let Some(element) = node.element_data() else {
        return DomResponse::ok(reply, DomValue::Null);
    };
    match field {
        "value" => {
            if let Some(text) = element.text_input_data() {
                return DomResponse::ok(reply, DomValue::String(text.editor.text().to_string()));
            }
            let value = element.attr(local_name!("value")).unwrap_or("").to_string();
            DomResponse::ok(reply, DomValue::String(value))
        }
        "checked" => {
            let checked = element.checkbox_input_checked().unwrap_or(false);
            DomResponse::ok(
                reply,
                DomValue::String(if checked {
                    "true".into()
                } else {
                    "false".into()
                }),
            )
        }
        _ => DomResponse::ok(reply, DomValue::Null),
    }
}

fn control_in_form_tree(doc: &BaseDocument, control: NodeId, form: NodeId) -> bool {
    let mut current = Some(control);
    while let Some(id) = current {
        if id == form {
            return true;
        }
        current = doc.get_node(id).and_then(|n| n.parent);
    }
    false
}

fn is_form_control(tag: &str) -> bool {
    matches!(
        tag,
        "input" | "textarea" | "select" | "button" | "output" | "object" | "fieldset"
    )
}

/// Apply domino-reported control values onto Blitz before building the entry list.
pub fn push_form_values(doc: &mut BaseDocument, pairs: &[(u64, String)]) {
    for &(handle, ref value) in pairs {
        let Some(id) = super::bridge::resolve_handle(doc, handle) else {
            continue;
        };
        let mut m = doc.mutate();
        m.set_attribute(id, blitz_dom::qual_name!("value"), value);
    }
}

/// Live Blitz state for every control associated with `form`.
pub fn sync_form_controls(doc: &BaseDocument, form: NodeId) -> Vec<(u64, String, Option<bool>)> {
    if doc.get_node(form).is_none() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for control_id in TreeTraverser::new(doc) {
        let Some(node) = doc.get_node(control_id) else {
            continue;
        };
        let Some(element) = node.element_data() else {
            continue;
        };
        if !is_form_control(element.name.local.as_ref()) {
            continue;
        }
        if !control_in_form_tree(doc, control_id, form) {
            continue;
        }
        let handle = super::bridge::node_to_handle(control_id);
        let value = element
            .text_input_data()
            .map(|t| t.editor.text().to_string())
            .or_else(|| element.attr(local_name!("value")).map(|s| s.to_string()))
            .unwrap_or_default();
        let checked = if element.checkbox_input_checked().is_some() {
            Some(element.checkbox_input_checked().unwrap_or(false))
        } else {
            None
        };
        out.push((handle, value, checked));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use blitz_dom::DocumentConfig;

    #[test]
    fn control_field_reads_value_attribute() {
        let html = "<!DOCTYPE html><html><body><input id=i value=from-attr></body></html>";
        let doc = crate::parse::document_from_html(html, DocumentConfig::default(), false);
        let input = doc.get_element_by_id("i").expect("input");
        let resp = reply_control_field(
            &doc,
            super::super::bridge::node_to_handle(input),
            "value",
            1,
        );
        match resp.value {
            DomValue::String(s) => assert_eq!(s, "from-attr"),
            other => panic!("unexpected {other:?}"),
        }
    }
}
