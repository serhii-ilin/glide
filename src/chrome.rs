//! The browser chrome is itself an HTML document. The page lives in an
//! `<iframe>`, so link clicks, scrolling, and subresource loads stay inside
//! Blitz. Buttons and the address form report back through [`command_from_url`].

use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavCommand {
    Back,
    Forward,
    Reload,
    ToggleJs,
    /// Raw address-bar text, not yet classified as a URL or a search.
    Go(String),
}

pub fn command_from_url(url: &Url) -> Option<NavCommand> {
    if url.scheme() == "glide" {
        return match url.host_str() {
            Some("back") => Some(NavCommand::Back),
            Some("forward") => Some(NavCommand::Forward),
            Some("reload") => Some(NavCommand::Reload),
            Some("toggle-js") => Some(NavCommand::ToggleJs),
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
  nav {{
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    background: #f7f4ee;
    border-bottom: 1px solid #e4ddd0;
  }}
  a.btn {{
    min-width: 32px;
    height: 32px;
    padding: 0 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    background: #fff;
    border: 1px solid #e4ddd0;
    color: #1c1915;
    text-decoration: none;
    font-size: 16px;
  }}
  form {{
    flex: 1;
    display: flex;
    margin: 0;
  }}
  input {{
    width: 100%;
    height: 32px;
    border: 1px solid #e4ddd0;
    border-radius: 999px;
    padding: 0 14px;
    background: #fff;
    color: #1c1915;
    font-size: 14px;
  }}
  iframe {{
    flex: 1;
    width: 100%;
    border: 0;
    background: #fff;
    min-height: 0;
  }}
</style>
</head>
<body>
  <nav>
    <a class="btn" href="glide://back" title="Back">&#8592;</a>
    <a class="btn" href="glide://forward" title="Forward">&#8594;</a>
    <a class="btn" href="glide://reload" title="Reload">&#8635;</a>
    <a id="js-toggle" class="btn" href="glide://toggle-js" title="Toggle JavaScript">{js_label}</a>
    <form action="https://glide.invalid/go" method="get">
      <input id="url" name="q" type="text" value="{url}" spellcheck="false" autocomplete="off">
    </form>
  </nav>
  <iframe id="page" src="{url}"></iframe>
</body>
</html>
"#
    )
}

fn escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

#[cfg(test)]
mod tests {
    use super::{NavCommand, command_from_url};
    use url::Url;

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
            command_from_url(&Url::parse("glide://toggle-js").unwrap()),
            Some(NavCommand::ToggleJs)
        );
    }

    #[test]
    fn reads_the_address_form() {
        let url = Url::parse("https://glide.invalid/go?q=example.com").unwrap();
        assert_eq!(
            command_from_url(&url),
            Some(NavCommand::Go("example.com".into()))
        );
    }
}
