//! HTML parsing with configurable `scripting_enabled` for html5ever.

use std::sync::{LazyLock, Mutex};

use blitz_dom::{BaseDocument, DocumentConfig, DocumentMutator, UserAgentStylesheet};
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
/// Stylo does not evaluate Blitz's `@media (scripting)` UA rule here, so make the
/// standard "noscript is not rendered when scripting is enabled" rule explicit.
static NOSCRIPT_HIDDEN: LazyLock<UserAgentStylesheet> =
    LazyLock::new(|| UserAgentStylesheet::parse("noscript { display: none !important; }"));

pub fn document_from_html_with(
    html: &str,
    mut config: DocumentConfig,
    scripting_enabled: bool,
    before_parse: impl FnOnce(&BaseDocument),
) -> BaseDocument {
    if scripting_enabled {
        config.ua_stylesheets.push(NOSCRIPT_HIDDEN.clone());
    }
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
    let rest = skip_to_doctype(html)?;
    let after = rest[9..].trim_start();
    let end = after.find('>')?;
    let inside = &after[..end];
    let name_end = inside.find(char::is_whitespace).unwrap_or(inside.len());
    let name = inside[..name_end].to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    let tail = inside[name_end..].trim_start();
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

/// Leading BOM, whitespace and comments, then a `<!DOCTYPE` at the cursor.
fn skip_to_doctype(html: &str) -> Option<&str> {
    let mut rest = html.trim_start_matches('\u{feff}');
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("<!--") {
            let end = after.find("-->")?;
            rest = &after[end + 3..];
            continue;
        }
        break;
    }
    let bytes = rest.as_bytes();
    if bytes.len() < 9 || !bytes[..9].eq_ignore_ascii_case(b"<!doctype") {
        return None;
    }
    Some(rest)
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

#[cfg(test)]
mod tests {
    use super::{document_from_html, sniff_doctype};
    use blitz_dom::DocumentConfig;

    const NOSCRIPT_HTML: &str = concat!(
        "<!DOCTYPE html><html><body>",
        "<noscript id=\"n\"><meta http-equiv=\"refresh\" content=\"0; URL=/?_fb_noscript=1\"></noscript>",
        "<p id=\"p\">page</p>",
        "</body></html>"
    );

    #[test]
    fn noscript_is_hidden_when_scripting_is_enabled() {
        let mut doc = document_from_html(NOSCRIPT_HTML, DocumentConfig::default(), true);
        doc.resolve(0.0);
        let noscript = doc.get_element_by_id("n").expect("noscript");
        assert_eq!(doc.resolved_style_value(noscript, "display"), "none");
        let paragraph = doc.get_element_by_id("p").expect("paragraph");
        assert_ne!(doc.resolved_style_value(paragraph, "display"), "none");
    }

    #[test]
    fn noscript_is_visible_when_scripting_is_disabled() {
        let mut doc = document_from_html(NOSCRIPT_HTML, DocumentConfig::default(), false);
        doc.resolve(0.0);
        let noscript = doc.get_element_by_id("n").expect("noscript");
        assert_ne!(doc.resolved_style_value(noscript, "display"), "none");
    }

    #[test]
    fn html401_public_doctype_stops_at_the_closing_bracket() {
        let html =
            r#"<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01 Transitional//EN"><html lang="en">"#;
        let dt = sniff_doctype(html).expect("doctype");
        assert_eq!(dt.name, "html");
        assert_eq!(dt.public_id, "-//W3C//DTD HTML 4.01 Transitional//EN");
        assert_eq!(dt.system_id, "");
    }

    #[test]
    fn html401_doctype_keeps_system_id() {
        let html = r#"<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01 Transitional//EN" "http://www.w3.org/TR/html4/loose.dtd">"#;
        let dt = sniff_doctype(html).expect("doctype");
        assert_eq!(dt.name, "html");
        assert_eq!(dt.public_id, "-//W3C//DTD HTML 4.01 Transitional//EN");
        assert_eq!(dt.system_id, "http://www.w3.org/TR/html4/loose.dtd");
    }

    #[test]
    fn doctype_after_comment_and_bom() {
        let html = "\u{feff}<!-- c --><!doctype html>";
        let dt = sniff_doctype(html).expect("doctype");
        assert_eq!(dt.name, "html");
        assert_eq!(dt.public_id, "");
        assert_eq!(dt.system_id, "");
    }

    #[test]
    fn missing_doctype_is_none() {
        assert!(sniff_doctype("<html><body></body></html>").is_none());
    }
}
