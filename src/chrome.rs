//! The browser chrome is itself an HTML document. The page lives in an
//! `<iframe>`, so link clicks, scrolling, and subresource loads stay inside
//! Blitz. Buttons and the address form report back through [`command_from_url`].

use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavCommand {
    Back,
    Forward,
    Reload,
    Stop,
    ToggleJs,
    FindClose,
    FindNext,
    FindPrev,
    /// Raw address-bar text, not yet classified as a URL or a search.
    Go(String),
}

pub fn command_from_url(url: &Url) -> Option<NavCommand> {
    if url.scheme() == "glide" {
        return match url.host_str() {
            Some("back") => Some(NavCommand::Back),
            Some("forward") => Some(NavCommand::Forward),
            Some("reload") => Some(NavCommand::Reload),
            Some("stop") => Some(NavCommand::Stop),
            Some("toggle-js") => Some(NavCommand::ToggleJs),
            Some("find-close") => Some(NavCommand::FindClose),
            Some("find-next") => Some(NavCommand::FindNext),
            Some("find-prev") => Some(NavCommand::FindPrev),
            _ => None,
        };
    }

    if url.host_str() == Some("glide.invalid") {
        let query = url
            .query_pairs()
            .find(|(key, _)| key == "q")
            .map(|(_, value)| value.into_owned())
            .unwrap_or_default();
        let query = query.trim().to_string();
        if query.is_empty() {
            return None;
        }
        return Some(NavCommand::Go(query));
    }

    Some(NavCommand::Go(url.to_string()))
}

pub fn chrome_document(start_url: &str, js_enabled: bool) -> String {
    let url = escape_attr(start_url);
    let js_label = if js_enabled { "JS on" } else { "JS off" };
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Glide</title>
<style>
  html {{
    --ui: 1;
  }}
  html, body {{
    margin: 0;
    height: 100%;
    background: #f4f1ea;
    color: #1c1915;
    font-family: "Segoe UI", Ubuntu, Cantarell, sans-serif;
  }}
  body {{
    display: flex;
    flex-direction: column;
  }}
  nav#toolbar {{
    display: flex;
    align-items: center;
    gap: calc(6px * var(--ui));
    padding: calc(8px * var(--ui)) calc(10px * var(--ui));
    background: #f7f4ee;
    border-bottom: calc(1px * var(--ui)) solid #e4ddd0;
    flex: none;
  }}
  a.btn {{
    min-width: calc(32px * var(--ui));
    height: calc(32px * var(--ui));
    padding: 0 calc(8px * var(--ui));
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: calc(8px * var(--ui));
    background: #fff;
    border: calc(1px * var(--ui)) solid #e4ddd0;
    color: #1c1915;
    text-decoration: none;
    font-size: calc(16px * var(--ui));
  }}
  #zoom-label {{
    min-width: calc(44px * var(--ui));
    text-align: center;
    font-size: calc(12px * var(--ui));
    color: #6b6358;
    display: none;
  }}
  #zoom-label.visible {{
    display: block;
  }}
  form {{
    flex: 1;
    display: flex;
    margin: 0;
  }}
  input {{
    width: 100%;
    height: calc(32px * var(--ui));
    border: calc(1px * var(--ui)) solid #e4ddd0;
    border-radius: 999px;
    padding: 0 calc(14px * var(--ui));
    background: #fff;
    color: #1c1915;
    font-size: calc(14px * var(--ui));
  }}
  #find-bar {{
    display: none;
    align-items: center;
    gap: calc(6px * var(--ui));
    padding: calc(6px * var(--ui)) calc(10px * var(--ui));
    background: #faf7f0;
    border-bottom: calc(1px * var(--ui)) solid #e4ddd0;
    flex: none;
  }}
  #find-bar.open {{
    display: flex;
  }}
  #find-input {{
    flex: 1;
    width: auto;
  }}
  #find-count {{
    font-size: calc(12px * var(--ui));
    color: #6b6358;
    min-width: calc(72px * var(--ui));
    text-align: center;
  }}
  iframe {{
    flex: 1;
    width: 100%;
    border: 0;
    background: #fff;
    min-height: 0;
  }}
  #loading {{
    height: calc(2px * var(--ui));
    background: #3b82f6;
    flex: none;
    display: none;
  }}
  #loading.active {{
    display: block;
  }}
</style>
</head>
<body>
  <nav id="toolbar">
    <a class="btn" href="glide://back" title="Back">&#8592;</a>
    <a class="btn" href="glide://forward" title="Forward">&#8594;</a>
    <a id="reload" class="btn" href="glide://reload" title="Reload">&#8635;</a>
    <a id="js-toggle" class="btn" href="glide://toggle-js" title="Toggle JavaScript">{js_label}</a>
    <span id="zoom-label"></span>
    <form action="https://glide.invalid/go" method="get">
      <input id="url" name="q" type="text" value="{url}" spellcheck="false" autocomplete="off">
    </form>
  </nav>
  <div id="find-bar">
    <input id="find-input" type="text" spellcheck="false" autocomplete="off" placeholder="Find in page">
    <span id="find-count"></span>
    <a class="btn" href="glide://find-prev" title="Previous match">&#8593;</a>
    <a class="btn" href="glide://find-next" title="Next match">&#8595;</a>
    <a class="btn" href="glide://find-close" title="Close find">&#10005;</a>
  </div>
  <div id="loading"></div>
  <iframe id="page" src="{url}"></iframe>
</body>
</html>
"#
    )
}

use blitz_dom::{BaseDocument, local_name, qual_name};

/// Whether the address bar currently has keyboard focus.
pub fn address_bar_focused(doc: &BaseDocument) -> bool {
    let Some(url_id) = doc.get_element_by_id("url") else {
        return false;
    };
    doc.get_focussed_node_id() == Some(url_id)
}

/// Whether the committed URL should be written into the address field.
pub fn should_write_committed_address(focused: bool, written: &str, shown: &str) -> bool {
    !focused && written != shown
}

/// Update the committed URL when the user is not editing the address bar.
pub fn sync_committed_address(doc: &mut BaseDocument, url: &str, address_written: &mut String) {
    let focused = address_bar_focused(doc);
    if should_write_committed_address(focused, address_written, url) {
        set_address(doc, url);
        *address_written = url.to_string();
    }
}

pub fn set_address(doc: &mut BaseDocument, url: &str) {
    let Some(id) = doc.get_element_by_id("url") else {
        return;
    };
    doc.mutate().set_attribute(id, qual_name!("value"), url);
}

pub fn set_loading_ui(doc: &mut BaseDocument, loading: bool) {
    if let Some(id) = doc.get_element_by_id("loading") {
        let class = if loading { "active" } else { "" };
        doc.mutate().set_attribute(id, qual_name!("class"), class);
    }
    if let Some(id) = doc.get_element_by_id("reload") {
        let mut m = doc.mutate();
        if loading {
            m.set_attribute(id, qual_name!("href"), "glide://stop");
            m.set_attribute(id, qual_name!("title"), "Stop");
            let children: Vec<_> = m.child_ids(id).into_iter().collect();
            for child in children {
                m.remove_and_drop_node(child);
            }
            let text_id = m.create_text_node("■");
            m.append_children(id, &[text_id]);
        } else {
            m.set_attribute(id, qual_name!("href"), "glide://reload");
            m.set_attribute(id, qual_name!("title"), "Reload");
            let children: Vec<_> = m.child_ids(id).into_iter().collect();
            for child in children {
                m.remove_and_drop_node(child);
            }
            let text_id = m.create_text_node("↻");
            m.append_children(id, &[text_id]);
        }
    }
}

pub fn find_bar_open(doc: &BaseDocument) -> bool {
    doc.get_element_by_id("find-bar").is_some_and(|id| {
        doc.get_node(id)
            .and_then(|n| n.attr(local_name!("class")))
            .is_some_and(|c| c.split_whitespace().any(|part| part == "open"))
    })
}

pub fn find_input_focused(doc: &BaseDocument) -> bool {
    let Some(id) = doc.get_element_by_id("find-input") else {
        return false;
    };
    doc.get_focussed_node_id() == Some(id)
}

pub fn find_input_value(doc: &BaseDocument) -> Option<String> {
    let id = doc.get_element_by_id("find-input")?;
    let input = doc.get_node(id)?.element_data()?.text_input_data()?;
    Some(input.editor.text().to_string())
}

pub fn set_find_bar_open(doc: &mut BaseDocument, open: bool) {
    let Some(id) = doc.get_element_by_id("find-bar") else {
        return;
    };
    let class = if open { "open" } else { "" };
    doc.mutate().set_attribute(id, qual_name!("class"), class);
}

pub fn focus_find_input(doc: &mut BaseDocument) {
    if let Some(id) = doc.get_element_by_id("find-input") {
        doc.set_focus_to(id);
    }
}

pub fn set_find_count(doc: &mut BaseDocument, current: usize, total: usize) {
    let Some(id) = doc.get_element_by_id("find-count") else {
        return;
    };
    let text = if total == 0 {
        String::new()
    } else {
        format!("{current} of {total}")
    };
    let mut m = doc.mutate();
    let children: Vec<_> = m.child_ids(id).into_iter().collect();
    for child in children {
        m.remove_and_drop_node(child);
    }
    let text_id = m.create_text_node(&text);
    m.append_children(id, &[text_id]);
}

pub fn set_zoom_label(doc: &mut BaseDocument, label: Option<&str>) {
    let Some(id) = doc.get_element_by_id("zoom-label") else {
        return;
    };
    let mut m = doc.mutate();
    let class = if label.is_some() { "visible" } else { "" };
    m.set_attribute(id, qual_name!("class"), class);
    let children: Vec<_> = m.child_ids(id).into_iter().collect();
    for child in children {
        m.remove_and_drop_node(child);
    }
    if let Some(label) = label {
        let text_id = m.create_text_node(label);
        m.append_children(id, &[text_id]);
    }
}

pub fn set_ui_scale(doc: &mut BaseDocument, ui: f32) {
    let root = doc.root_element().id;
    let style = format!("--ui: {ui}");
    doc.mutate()
        .set_attribute(root, qual_name!("style"), &style);
}

pub fn toolbar_layout_height(doc: &BaseDocument) -> Option<f32> {
    doc.get_element_by_id("toolbar")
        .and_then(|id| doc.get_node(id))
        .map(|n| n.final_layout().size.height)
}

fn escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        NavCommand, command_from_url, set_ui_scale, should_write_committed_address,
        toolbar_layout_height,
    };
    use blitz_dom::DocumentConfig;
    use blitz_traits::shell::{ColorScheme, Viewport};
    use url::Url;

    use crate::chrome::chrome_document;
    use crate::html::GlideHtmlProvider;
    use crate::parse;

    #[test]
    fn classifies_chrome_actions() {
        assert_eq!(
            command_from_url(&Url::parse("glide://back").unwrap()),
            Some(NavCommand::Back)
        );
        assert_eq!(
            command_from_url(&Url::parse("glide://forward").unwrap()),
            Some(NavCommand::Forward)
        );
        assert_eq!(
            command_from_url(&Url::parse("glide://reload").unwrap()),
            Some(NavCommand::Reload)
        );
        assert_eq!(
            command_from_url(&Url::parse("glide://stop").unwrap()),
            Some(NavCommand::Stop)
        );
        assert_eq!(
            command_from_url(&Url::parse("glide://toggle-js").unwrap()),
            Some(NavCommand::ToggleJs)
        );
    }

    #[test]
    fn address_sync_skips_when_focused_writes_when_stale() {
        assert!(!should_write_committed_address(
            true,
            "https://a/",
            "https://b/"
        ));
        assert!(should_write_committed_address(
            false,
            "https://a/",
            "https://b/"
        ));
        assert!(!should_write_committed_address(
            false,
            "https://b/",
            "https://b/"
        ));
    }

    #[test]
    fn reads_the_address_form() {
        let url = Url::parse("https://glide.invalid/go?q=example.com").unwrap();
        assert_eq!(
            command_from_url(&url),
            Some(NavCommand::Go("example.com".into()))
        );
    }

    #[test]
    fn toolbar_height_stable_across_page_zoom() {
        let html = chrome_document("about:blank", true);
        let config = DocumentConfig {
            viewport: Some(Viewport::new(1100, 760, 1.0, ColorScheme::Light)),
            html_parser_provider: Some(Arc::new(GlideHtmlProvider::default())),
            ..DocumentConfig::default()
        };
        let mut doc = parse::document_from_html(&html, config, false);
        doc.resolve(0.0);
        let zoom_at_100 = doc.viewport().zoom();
        let visual_h100 =
            toolbar_layout_height(&doc).expect("toolbar height at 100%") * zoom_at_100;
        {
            let mut vp = doc.viewport_mut();
            vp.set_zoom(2.0);
        }
        set_ui_scale(&mut doc, 0.5);
        doc.resolve(0.0);
        let zoom_at_200 = doc.viewport().zoom();
        let visual_h200 =
            toolbar_layout_height(&doc).expect("toolbar height at 200% zoom") * zoom_at_200;
        assert!(
            (visual_h100 - visual_h200).abs() < 2.0,
            "toolbar visual height should stay stable: {visual_h100} vs {visual_h200}"
        );
    }
}
