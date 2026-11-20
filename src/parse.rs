//! HTML parsing with configurable `scripting_enabled` for html5ever.

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
    let mut doc = BaseDocument::new(config);
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
