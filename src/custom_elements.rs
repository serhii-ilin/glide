//! Fallbacks for pages whose UI is built from custom elements.
//!
//! When scripts do not upgrade custom elements, two server-rendered
//! conventions depend on that upgrade and are applied here while parsing:
//!
//! - `show_unupgraded`: pages hide their custom-element shell with
//!   `:not(:defined)` until it upgrades. Built-in elements are marked `:defined`
//!   at parse (F-076); unupgraded autonomous custom elements are not. A per-document
//!   UA rule keeps server-rendered subtrees visible when definitions never run.
//! - `apply_streamed_suspense`: Reddit (faceplate) streams its feed after the
//!   shell. `<suspense-placeholder id="…">` marks a slot and a later
//!   `<template for="…">` carries the markup; the page's `suspense-replace`
//!   custom element performs the swap in a browser. Do the swap here.

use std::collections::{BTreeSet, HashMap};

use blitz_dom::{BaseDocument, NodeId, UserAgentStylesheet, local_name};
use stylo_dom::ElementState;

pub const HTML_NAMESPACE: &str = "http://www.w3.org/1999/xhtml";

/// Whether `local` in the HTML namespace is a custom element tag name (hyphenated).
pub fn is_custom_element_local_name(local: &str, namespace: &str) -> bool {
    namespace == HTML_NAMESPACE
        && local.contains('-')
        && local.starts_with(|c: char| c.is_ascii_lowercase())
}

/// Built-in HTML elements match `:defined`; autonomous custom elements do not until upgraded.
pub fn mark_built_in_elements_defined(doc: &mut BaseDocument) {
    let mut defined_ids = Vec::new();
    visit(doc, &mut |id, element| {
        let namespace: &str = element.name.ns.as_ref();
        let local: &str = element.name.local.as_ref();
        if namespace == HTML_NAMESPACE && !is_custom_element_local_name(local, namespace) {
            defined_ids.push(id);
        }
    });
    for id in defined_ids {
        doc.snapshot_node_and(id, ElementState::DEFINED, |node| {
            node.element_state_mut().insert(ElementState::DEFINED);
        });
    }
}

/// Keep unupgraded custom elements visible instead of letting a page's
/// `:not(:defined)` rule hide its server-rendered shell.
///
/// The rule covers each custom element's subtree and outranks every page
/// `visibility` declaration inside it. A widget that hides its own states with
/// `visibility` (an unupgraded carousel, for example) shows its pre-upgrade
/// state; that is preferable to hiding the page's whole server-rendered shell.
pub fn show_unupgraded(doc: &mut BaseDocument) {
    let names = custom_element_names(doc);
    if names.is_empty() {
        return;
    }
    // A UA `!important` declaration outranks the page's own `visibility` rules.
    // Standard descendants are covered too: pages usually hide every
    // `:not(:defined)` element, including the plain elements inside the shell.
    // Built-in `html` and `body` stay `:defined` after parse (F-076), so the UA
    // rule does not need to name them explicitly.
    let selectors = names
        .iter()
        .map(|name| format!("{name}:not(:defined),{name}:not(:defined) *"))
        .collect::<Vec<_>>()
        .join(",");
    let css = format!("{selectors}{{visibility:visible!important}}");
    doc.add_user_agent_stylesheet(UserAgentStylesheet::parse(&css));
}

/// Replace every `<suspense-placeholder>` whose `id` has a matching
/// `<template for="id">` with that template's content.
pub fn apply_streamed_suspense(doc: &mut BaseDocument) {
    let mut templates: HashMap<String, NodeId> = HashMap::new();
    let mut placeholders: Vec<(String, NodeId)> = Vec::new();
    visit(doc, &mut |id, element| {
        if element.name.local == local_name!("template")
            && let Some(target) = element.attr(local_name!("for"))
            && let Some(contents) = element.template_contents
            && !target.is_empty()
        {
            templates.insert(target.to_string(), contents);
        } else if element.name.local.to_string() == "suspense-placeholder"
            && let Some(target) = element.attr(local_name!("id"))
        {
            placeholders.push((target.to_string(), id));
        }
    });
    if placeholders.is_empty() {
        return;
    }

    let mut mutr = doc.mutate();
    for (target, placeholder) in placeholders {
        let Some(contents) = templates.get(&target) else {
            continue;
        };
        let children = mutr.child_ids(*contents);
        if children.is_empty() {
            continue;
        }
        mutr.replace_with_nodes(placeholder, &children);
    }
}

/// HTML custom element tag names used in the document (a hyphen is required by
/// the custom element name grammar).
fn custom_element_names(doc: &BaseDocument) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    visit(doc, &mut |_, element| {
        let namespace: &str = element.name.ns.as_ref();
        let local: &str = element.name.local.as_ref();
        if is_custom_element_local_name(local, namespace) {
            names.insert(local.to_string());
        }
    });
    names
}

fn visit(doc: &BaseDocument, f: &mut impl FnMut(NodeId, &blitz_dom::ElementData)) {
    let mut pending = vec![doc.root_node().id];
    while let Some(id) = pending.pop() {
        let Some(node) = doc.get_node(id) else {
            continue;
        };
        if let Some(element) = node.element_data() {
            f(id, element);
        }
        pending.extend(node.children.iter().copied());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blitz_dom::{DocumentConfig, NodeData};

    use crate::parse;

    fn parse_page(html: &str) -> BaseDocument {
        parse::document_from_html(html, DocumentConfig::default(), false)
    }

    fn body_text(doc: &BaseDocument) -> String {
        fn walk(doc: &BaseDocument, id: NodeId, out: &mut String) {
            let Some(node) = doc.get_node(id) else {
                return;
            };
            if let NodeData::Text(_) = &node.data {
                let text = node.text_content();
                if !text.trim().is_empty() {
                    out.push_str(&text);
                    out.push(' ');
                }
                return;
            }
            for child in &node.children {
                walk(doc, *child, out);
            }
        }
        let mut out = String::new();
        if let Some(body) = doc.find_body_node() {
            for child in &body.children {
                walk(doc, *child, &mut out);
            }
        }
        out
    }

    #[test]
    fn streamed_template_replaces_placeholder() {
        let doc = parse_page(
            "<!DOCTYPE html><html><body>\
             <suspense-placeholder id=\"s_1\">Loading</suspense-placeholder>\
             <template for=\"s_1\"><article>Post one</article></template>\
             </body></html>",
        );
        assert!(!body_text(&doc).contains("Loading"));
        assert!(body_text(&doc).contains("Post one"));
    }

    #[test]
    fn unmatched_placeholder_is_left_alone() {
        let doc = parse_page(
            "<!DOCTYPE html><html><body>\
             <suspense-placeholder id=\"s_2\">Loading</suspense-placeholder>\
             <template for=\"s_1\"><article>Post one</article></template>\
             </body></html>",
        );
        assert!(body_text(&doc).contains("Loading"));
        assert!(!body_text(&doc).contains("Post one"));
    }

    #[test]
    fn empty_template_keeps_placeholder() {
        let doc = parse_page(
            "<!DOCTYPE html><html><body>\
             <suspense-placeholder id=\"s_1\">Loading</suspense-placeholder>\
             <template for=\"s_1\"></template>\
             </body></html>",
        );
        assert!(body_text(&doc).contains("Loading"));
    }

    #[test]
    fn custom_element_names_skip_standard_elements() {
        let doc = parse_page(
            "<!DOCTYPE html><html><body><my-widget></my-widget><div></div></body></html>",
        );
        let names = custom_element_names(&doc);
        assert!(names.contains("my-widget"));
        assert!(!names.contains("div"));
    }

    #[test]
    fn built_in_elements_match_defined_before_upgrade_hammer() {
        let doc = parse_page(
            "<!DOCTYPE html><html><body><my-widget><span>shiny</span></my-widget></body></html>",
        );
        let html_id = doc.query_selector("html").expect("selector").expect("node");
        let span = doc.query_selector("span").expect("selector").expect("node");
        assert!(
            doc.get_node(html_id)
                .expect("node")
                .element_state()
                .contains(ElementState::DEFINED)
        );
        assert!(
            doc.get_node(span)
                .expect("node")
                .element_state()
                .contains(ElementState::DEFINED)
        );
    }

    #[test]
    fn unupgraded_custom_element_stays_visible() {
        // Build without the parse-time fallback so the page's own rule applies.
        let mut doc = BaseDocument::new(DocumentConfig::default());
        let mut mutr = doc.mutate();
        crate::parse::parse_into_mutator(
            &mut mutr,
            "<!DOCTYPE html><html><head><style>\
             :not(:defined){visibility:hidden}\
             </style></head><body><my-widget><span>shiny</span></my-widget></body></html>",
            false,
        );
        drop(mutr);
        doc.resolve(0.0);
        let widget = doc
            .query_selector("my-widget")
            .expect("selector")
            .expect("node");
        let span = doc.query_selector("span").expect("selector").expect("node");
        assert_eq!(doc.resolved_style_value(widget, "visibility"), "hidden");
        assert_eq!(doc.resolved_style_value(span, "visibility"), "hidden");

        show_unupgraded(&mut doc);
        doc.resolve(0.0);
        assert_eq!(doc.resolved_style_value(widget, "visibility"), "visible");
        assert_eq!(doc.resolved_style_value(span, "visibility"), "visible");
    }
}
