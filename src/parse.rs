//! HTML parsing with configurable `scripting_enabled` for html5ever.

use std::sync::Mutex;

use blitz_dom::{BaseDocument, DocumentConfig, DocumentMutator};
use html5ever::ParseOpts;
use html5ever::tendril::TendrilSink;
use html5ever::tokenizer::TokenizerOpts;
use html5ever::tree_builder::{QuirksMode, TreeBuilderOpts};

pub fn document_from_html(
    html: &str,
    config: DocumentConfig,
    scripting_enabled: bool,
) -> BaseDocument {
    document_from_html_with(html, config, scripting_enabled, |_| {})
}

/// Build a document and run `before_parse` after [`BaseDocument::new`] but before HTML is parsed,
/// so embedder hooks (e.g. chrome doc id for [`GlideNet`]) run before iframe loads fire.
pub fn document_from_html_with(
    html: &str,
    config: DocumentConfig,
    scripting_enabled: bool,
    before_parse: impl FnOnce(&BaseDocument),
) -> BaseDocument {
    let mut doc = BaseDocument::new(config);
    note_doctype(doc.id(), html);
    before_parse(&doc);
    let mut mutr = doc.mutate();
    parse_into_mutator(&mut mutr, html, scripting_enabled);
    drop(mutr);
    doc
}

pub fn parse_inner_html_into_mutator(
    mutr: &mut DocumentMutator<'_>,
    element_id: blitz_traits::node_id::NodeId,
    html: &str,
    scripting_enabled: bool,
) {
    let sink = blitz_html::DocumentHtmlParser::new(mutr);
    let opts = ParseOpts {
        tokenizer: TokenizerOpts::default(),
        tree_builder: TreeBuilderOpts {
            exact_errors: false,
            scripting_enabled,
            iframe_srcdoc: false,
            drop_doctype: true,
            quirks_mode: QuirksMode::NoQuirks,
        },
    };
    html5ever::driver::parse_fragment_for_element(sink, opts, element_id, false, None)
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .unwrap();
    let document_id = mutr.doc.root_node().id;
    let fragment_root_id = mutr.last_child_id(document_id).unwrap();
    let child_ids = mutr.child_ids(fragment_root_id);
    mutr.append_children(element_id, &child_ids);
    mutr.remove_and_drop_node(fragment_root_id);
}

pub fn parse_into_mutator(mutr: &mut DocumentMutator<'_>, html: &str, scripting_enabled: bool) {
    let is_xhtml_doc = html.starts_with("<?xml")
        || html.starts_with("<!DOCTYPE")
            && html
                .lines()
                .next()
                .is_some_and(|l| l.contains("XHTML") || l.contains("xhtml"))
        || root_element_has_xhtml_namespace(html);

    if is_xhtml_doc {
        blitz_html::DocumentHtmlParser::parse_xml_into_mutator(mutr, html);
        return;
    }

    let sink = blitz_html::DocumentHtmlParser::new(mutr);
    let opts = ParseOpts {
        tokenizer: TokenizerOpts::default(),
        tree_builder: TreeBuilderOpts {
            exact_errors: false,
            scripting_enabled,
            iframe_srcdoc: false,
            drop_doctype: true,
            quirks_mode: QuirksMode::NoQuirks,
        },
    };
    html5ever::parse_document(sink, opts)
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .unwrap();
}

/// Doctype sniffed from source. Blitz drops doctype tokens, so the snapshot
/// reads this instead of a document-type node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotedDoctype {
    pub name: String,
    pub public_id: String,
    pub system_id: String,
}

static DOCTYPES: Mutex<Vec<(usize, NotedDoctype)>> = Mutex::new(Vec::new());

pub fn noted_doctype(doc_id: usize) -> Option<NotedDoctype> {
    let guard = DOCTYPES.lock().unwrap_or_else(|err| err.into_inner());
    guard
        .iter()
        .find(|(id, _)| *id == doc_id)
        .map(|(_, dt)| dt.clone())
}

fn note_doctype(doc_id: usize, html: &str) {
    let Some(dt) = sniff_doctype(html) else {
        return;
    };
    let mut guard = DOCTYPES.lock().unwrap_or_else(|err| err.into_inner());
    guard.retain(|(id, _)| *id != doc_id);
    guard.push((doc_id, dt));
    if guard.len() > 64 {
        let drop_n = guard.len() - 64;
        guard.drain(0..drop_n);
    }
}

fn sniff_doctype(html: &str) -> Option<NotedDoctype> {
    let rest = html.trim_start_matches('\u{feff}').trim_start();
    let bytes = rest.as_bytes();
    if bytes.len() < 9 || !bytes[..9].eq_ignore_ascii_case(b"<!doctype") {
        return None;
    }
    let after = rest[9..].trim_start();
    let name_end = after
        .find(|c: char| c.is_whitespace() || c == '>')
        .unwrap_or(after.len());
    let name = after[..name_end].to_string();
    if name.is_empty() {
        return None;
    }
    let tail = after[name_end..].trim_start();
    let mut public_id = String::new();
    let mut system_id = String::new();
    let tail_l = tail.to_ascii_lowercase();
    if let Some(rest) = tail_l.strip_prefix("public") {
        let ids = quoted_ids(&tail[tail.len() - rest.len()..]);
        public_id = ids.0;
        system_id = ids.1;
    } else if let Some(rest) = tail_l.strip_prefix("system") {
        system_id = quoted_ids(&tail[tail.len() - rest.len()..]).0;
    }
    Some(NotedDoctype {
        name,
        public_id,
        system_id,
    })
}

fn quoted_ids(s: &str) -> (String, String) {
    let bytes = s.as_bytes();
    let mut out = [String::new(), String::new()];
    let mut n = 0;
    let mut i = 0;
    while i < bytes.len() && n < 2 {
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            out[n] = s[start..i].to_string();
            n += 1;
            if i < bytes.len() {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    (out[0].clone(), out[1].clone())
}

fn root_element_has_xhtml_namespace(html: &str) -> bool {
    let rest = html.trim_start_matches('\u{feff}').trim_start();
    let Some(rest) = rest.strip_prefix("<html") else {
        return false;
    };
    let Some(tag_end) = rest.find('>') else {
        return false;
    };
    rest[..tag_end].contains("xmlns=\"http://www.w3.org/1999/xhtml\"")
        || rest[..tag_end].contains("xmlns='http://www.w3.org/1999/xhtml'")
}
