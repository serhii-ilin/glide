//! HTTP navigation: follow redirects and keep non-success response bodies.
//!
//! blitz-net follows some redirects, then drops the body of any final status
//! that is not 2xx. Document loads need the opposite: walk 301/302/303/307/308,
//! a `Refresh` header, and an HTML `<meta http-equiv="refresh">`, then hand the
//! browser the last response, including a server error page.

use std::collections::HashSet;
use std::sync::OnceLock;

use blitz_traits::net::http::{self, HeaderMap, Method};
use blitz_traits::net::{Body, Bytes, EntryValue, FormData, Request};
use url::Url;

fn normalize_form_field(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(current) = chars.next() {
        match (current, chars.peek()) {
            ('\r', Some('\n')) => {
                result.push_str("\r\n");
                chars.next();
            }
            ('\r' | '\n', _) => result.push_str("\r\n"),
            _ => result.push(current),
        }
    }
    result
}

/// Encode a form entry list as `application/x-www-form-urlencoded` bytes.
pub fn encode_form_urlencoded(form: &FormData) -> Vec<u8> {
    let mut body = String::new();
    {
        let mut ser = url::form_urlencoded::Serializer::new(&mut body);
        for entry in form.iter() {
            let name = normalize_form_field(&entry.name);
            let value = match &entry.value {
                EntryValue::String(s) => normalize_form_field(s),
                EntryValue::File(p) => normalize_form_field(p.to_str().unwrap_or("")),
                EntryValue::EmptyFile => String::new(),
            };
            ser.append_pair(&name, &value);
        }
    }
    body.into_bytes()
}

/// `text/plain` form submission body (HTML encoding algorithm).
pub fn encode_form_text_plain(form: &FormData) -> Result<Vec<u8>, String> {
    let mut out = String::new();
    for entry in form.iter() {
        let name = normalize_form_field(&entry.name);
        let value = match &entry.value {
            EntryValue::String(s) => normalize_form_field(s),
            EntryValue::File(p) => normalize_form_field(p.to_str().unwrap_or("")),
            EntryValue::EmptyFile => String::new(),
        };
        out.push_str(&name);
        out.push('=');
        out.push_str(&value);
        out.push_str("\r\n");
    }
    Ok(out.into_bytes())
}

fn origin_header_value(url: &Url) -> Option<String> {
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?;
    let mut origin = format!("{}://{}", url.scheme(), host);
    if let Some(port) = url.port() {
        let default = match url.scheme() {
            "http" => 80,
            "https" => 443,
            _ => return Some(origin),
        };
        if port != default {
            origin.push(':');
            origin.push_str(&port.to_string());
        }
    }
    Some(origin)
}

const MAX_REDIRECTS: usize = 20;
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:60.0) Gecko/20100101 Firefox/81.0";

/// A completed HTTP exchange after redirects have been followed.
#[derive(Debug, Clone)]
pub struct Followed {
    pub url: String,
    pub status: u16,
    pub body: Bytes,
    pub redirected: bool,
    /// `Content-Type` header value, without parameters. Empty when the server omitted it.
    pub content_type: String,
    /// Response header names and values from the transport (including `Set-Cookie`;
    /// stripped before exposure to page `fetch` / XHR).
    pub headers: Vec<(String, String)>,
}

/// What a top-level navigation should do with an HTTP result.
#[derive(Debug)]
pub enum DocumentFetch {
    /// Render `body` as the document at `url` (the URL after redirects).
    Page {
        url: String,
        body: Bytes,
        status: u16,
        headers: Vec<(String, String)>,
    },
    /// Show Glide's error page. `url` is the URL the user asked for.
    Failed { url: String, reason: String },
}

struct Hop {
    url: Url,
    method: Method,
    headers: HeaderMap,
    body: Body,
    cookie_document: Option<Url>,
    cookie_kind: crate::cookies::CookieRequestKind,
    cookie_credentials: crate::cookies::FetchCredentials,
}

/// `http` and `https` top-level navigations this module can send.
///
/// POST submissions use [`Body::Form`] with `application/x-www-form-urlencoded`
/// encoding in [`send`]. Multipart navigations are not supported here.
pub fn is_navigable_http(request: &Request) -> bool {
    if !matches!(request.url.scheme(), "http" | "https") {
        return false;
    }
    match &request.body {
        Body::Form(_) => request.method == Method::POST,
        _ => true,
    }
}

/// Status codes a browser follows for a navigation or subresource.
pub fn is_redirect_status(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// Method to use on the next hop. 303 becomes GET (except HEAD). 301 and 302
/// turn POST into GET. 307 and 308 keep the method.
pub fn redirect_method(status: u16, method: &Method) -> Method {
    match status {
        303 if *method == Method::HEAD => Method::HEAD,
        303 => Method::GET,
        301 | 302 if *method == Method::POST => Method::GET,
        _ => method.clone(),
    }
}

/// Resolve a `Location` or `Refresh` target against the current URL.
pub fn resolve_redirect(base: &Url, location: &str) -> Option<Url> {
    let location = location.trim();
    if location.is_empty() {
        return None;
    }
    let mut url = match Url::parse(location) {
        Ok(url) => url,
        Err(_) => base.join(location).ok()?,
    };
    url.set_fragment(None);
    if url.scheme() == "http" || url.scheme() == "https" {
        Some(url)
    } else {
        None
    }
}

/// URL from a `Refresh` header (`0;url=https://example.com/next`).
pub fn refresh_target(header: &str) -> Option<&str> {
    let bytes = header.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if bytes[i..i + 4].eq_ignore_ascii_case(b"url=") {
            let rest = header[i + 4..].trim().trim_start_matches(['"', '\'']);
            let end = rest.find(['"', '\'', ';', ' ', '\t']).unwrap_or(rest.len());
            let url = rest[..end].trim();
            return if url.is_empty() { None } else { Some(url) };
        }
        i += 1;
    }
    None
}

/// Whether the response entity should replace the document.
///
/// 2xx always does (including an empty 204). A leftover 3xx or a 4xx/5xx does
/// when the server sent a body, so the site's own error page is shown.
pub fn render_http_body(status: u16, body_empty: bool) -> bool {
    if (200..300).contains(&status) {
        return true;
    }
    !body_empty
}

pub fn status_phrase(status: u16) -> &'static str {
    http::StatusCode::from_u16(status)
        .ok()
        .and_then(|code| code.canonical_reason())
        .unwrap_or("Error")
}

fn response_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_string(), v.to_string()))
        })
        .collect()
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("reqwest client builds with the process TLS backend")
    })
}

/// Follow redirects, then return the last status and body.
pub async fn fetch_followed(request: Request) -> Result<Followed, String> {
    let document = request.url.clone();
    fetch_followed_with_cookie_context(
        request,
        Some(document),
        crate::cookies::CookieRequestKind::Navigation,
        crate::cookies::FetchCredentials::Include,
    )
    .await
}

/// Same as [`fetch_followed`] with an explicit document URL and credentials mode.
pub async fn fetch_followed_with_cookie_context(
    request: Request,
    cookie_document: Option<Url>,
    cookie_kind: crate::cookies::CookieRequestKind,
    cookie_credentials: crate::cookies::FetchCredentials,
) -> Result<Followed, String> {
    let mut hop = Hop {
        url: request.url,
        method: request.method,
        headers: request.headers,
        body: request.body,
        cookie_document,
        cookie_kind,
        cookie_credentials,
    };
    hop.url.set_fragment(None);
    let mut redirected = false;
    let mut seen = HashSet::new();
    let mut redirects = 0u16;

    loop {
        let key = format!("{} {}", hop.method, hop.url);
        if !seen.insert(key) {
            return Err("Too many redirects".to_string());
        }
        let response = send(&hop).await?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let final_url = response.url().clone();
        crate::cookies::CookieJar::global().store_response_header_map(&final_url, &headers);
        if let Some(next) = next_hop(status, &headers, &hop) {
            redirects += 1;
            if redirects > MAX_REDIRECTS as u16 {
                return Err("Too many redirects".to_string());
            }
            redirected = true;
            let _ = response.bytes().await;
            hop = next;
            continue;
        }
        let content_type = headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let header_pairs = response_headers(&headers);
        let body = response
            .bytes()
            .await
            .map_err(|err| transport_reason(&err))?;
        return Ok(Followed {
            url: final_url.to_string(),
            status,
            body,
            redirected,
            content_type,
            headers: header_pairs,
        });
    }
}

/// Top-level navigation fetch: final document, or a reason for the error page.
///
/// After HTTP redirects, a zero-delay `<meta http-equiv="refresh">` is followed
/// too. A refresh that lives only inside `<noscript>` is followed only when
/// scripting is disabled: the HTML parser does not create `<noscript>` children
/// while scripting is enabled, so the fallback Google serves for script-less
/// clients must stay inert. See [`meta_refresh_url`].
pub async fn fetch_document(request: Request, scripting_enabled: bool) -> DocumentFetch {
    let original = request.url.to_string();
    let mut request = request;
    let mut seen = HashSet::new();
    seen.insert(request.url.to_string());
    let mut meta_hops = 0u16;
    loop {
        match fetch_followed(request).await {
            Ok(resp) if render_http_body(resp.status, resp.body.is_empty()) => {
                if meta_hops < MAX_REDIRECTS as u16
                    && is_html_document(&resp.content_type, &resp.body)
                    && let Some(target) =
                        meta_refresh_url(&String::from_utf8_lossy(&resp.body), scripting_enabled)
                    && let Ok(base) = Url::parse(&resp.url)
                    && let Some(next) = resolve_redirect(&base, &target)
                    && seen.insert(next.to_string())
                {
                    meta_hops += 1;
                    request = Request::get(next);
                    continue;
                }
                return DocumentFetch::Page {
                    url: resp.url,
                    body: resp.body,
                    status: resp.status,
                    headers: resp.headers,
                };
            }
            Ok(resp) => {
                return DocumentFetch::Failed {
                    url: original,
                    reason: format!(
                        "Server answered {} {}",
                        resp.status,
                        status_phrase(resp.status)
                    ),
                };
            }
            Err(reason) => {
                return DocumentFetch::Failed {
                    url: original,
                    reason,
                };
            }
        }
    }
}

fn is_html_document(content_type: &str, body: &[u8]) -> bool {
    if content_type.eq_ignore_ascii_case("text/html")
        || content_type.eq_ignore_ascii_case("application/xhtml+xml")
    {
        return true;
    }
    if !content_type.is_empty() {
        return false;
    }
    let head = &body[..body.len().min(256)];
    let sniffed = String::from_utf8_lossy(head);
    let sniffed = sniffed.trim_start().trim_start_matches('\u{feff}');
    let lower = sniffed.to_ascii_lowercase();
    lower.starts_with("<!doctype html") || lower.starts_with("<html") || lower.starts_with("<meta")
}

/// URL from a zero-delay meta refresh that a navigation should follow.
///
/// A refresh that lives only inside `<noscript>` is the fallback for a script
/// shell. Google's search response is that shell plus one `display:none` box,
/// so hidden subtrees do not count as page content.
///
/// Source / basis: HTML Standard, the `noscript` element in a document with a
/// browsing context — with scripting enabled its contents are not parsed, so a
/// refresh inside it never applies. With scripting disabled the contents are
/// parsed and the refresh does apply.
fn meta_refresh_url(html: &str, scripting_enabled: bool) -> Option<String> {
    let bytes = html.as_bytes();
    let mut i = 0;
    let mut in_script = false;
    let mut in_style = false;
    let mut in_noscript = false;
    let mut in_head = false;
    let mut body_has_content = false;
    let mut element_refresh = None;
    let mut noscript_refresh = None;
    // True when this open element, or an ancestor, is `display:none`.
    let mut hidden_stack: Vec<bool> = Vec::new();

    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &bytes[i + 1..];
        if rest.starts_with(b"!--") {
            if let Some(rel) = find_bytes(&bytes[i + 4..], b"-->") {
                i = i + 4 + rel + 3;
            } else {
                break;
            }
            continue;
        }
        if in_script {
            if tag_is(rest, b"/script") {
                in_script = false;
                hidden_stack.pop();
                i = end_of_tag(bytes, i);
            } else {
                i += 1;
            }
            continue;
        }
        if in_style {
            if tag_is(rest, b"/style") {
                in_style = false;
                hidden_stack.pop();
                i = end_of_tag(bytes, i);
            } else {
                i += 1;
            }
            continue;
        }
        let end = if rest
            .first()
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'/' || *c == b'!')
        {
            end_of_tag(bytes, i)
        } else {
            i + 1
        };
        let tag = &html[i..end];
        if tag_is(rest, b"/noscript") && in_noscript {
            in_noscript = false;
            hidden_stack.pop();
            i = end;
            continue;
        }
        if let Some(name) = end_tag_name(rest) {
            if !is_void(name) {
                hidden_stack.pop();
            }
            if name.eq_ignore_ascii_case("head") {
                in_head = false;
            }
            i = end;
            continue;
        }
        let Some(name) = start_tag_name(rest) else {
            i = end;
            continue;
        };
        let hidden = hidden_stack.last().copied().unwrap_or(false) || is_display_none(tag);
        if !is_void(name) {
            hidden_stack.push(hidden);
        }
        if name.eq_ignore_ascii_case("script") {
            in_script = true;
        } else if name.eq_ignore_ascii_case("style") {
            in_style = true;
        } else if name.eq_ignore_ascii_case("noscript") {
            in_noscript = true;
        } else if name.eq_ignore_ascii_case("head") {
            in_head = true;
        } else if name.eq_ignore_ascii_case("body") {
            in_head = false;
        } else if name.eq_ignore_ascii_case("meta")
            && let Some(url) = refresh_from_meta(tag)
        {
            if in_noscript {
                if noscript_refresh.is_none() {
                    noscript_refresh = Some(url);
                }
            } else if element_refresh.is_none() {
                element_refresh = Some(url);
            }
        } else if !hidden && !in_head && !in_noscript && is_content_name(name) {
            body_has_content = true;
        }
        i = end;
    }

    element_refresh.or(if scripting_enabled || body_has_content {
        None
    } else {
        noscript_refresh
    })
}

fn is_display_none(tag: &str) -> bool {
    let Some(style) = attr_value(tag, "style") else {
        return false;
    };
    let compact: String = style.chars().filter(|c| !c.is_whitespace()).collect();
    compact.to_ascii_lowercase().contains("display:none")
}

fn refresh_from_meta(tag: &str) -> Option<String> {
    let equiv = attr_value(tag, "http-equiv")?;
    if !equiv.eq_ignore_ascii_case("refresh") {
        return None;
    }
    refresh_destination(&attr_value(tag, "content")?)
}

/// Destination of a zero-delay refresh. A positive delay is left for the page.
fn refresh_destination(content: &str) -> Option<String> {
    let decoded = decode_basic_entities(content);
    let delay = decoded.split(';').next().unwrap_or("").trim();
    if let Ok(seconds) = delay.parse::<f64>() {
        if seconds > 0.0 {
            return None;
        }
    } else if delay.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    if let Some(url) = refresh_target(&decoded) {
        return Some(decode_basic_entities(url));
    }
    let rest = decoded.split_once(';')?.1.trim();
    let rest = rest.trim_matches(|c| c == '"' || c == '\'');
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

fn decode_basic_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(';') else {
            out.push('&');
            rest = after;
            continue;
        };
        let name = &after[..end];
        let ch = if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else if let Some(dec) = name.strip_prefix('#') {
            dec.parse::<u32>().ok().and_then(char::from_u32)
        } else {
            match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => None,
            }
        };
        if let Some(ch) = ch {
            out.push(ch);
            rest = &after[end + 1..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let needle = name.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i + needle.len() < bytes.len() {
        if bytes[i..].starts_with(needle.as_bytes()) {
            let before_ok = i == 0 || bytes[i - 1].is_ascii_whitespace() || bytes[i - 1] == b'<';
            let after = i + needle.len();
            if before_ok && bytes.get(after) == Some(&b'=') {
                return Some(read_attr(&tag[after + 1..]));
            }
        }
        i += 1;
    }
    None
}

fn read_attr(value: &str) -> String {
    let value = value.trim_start();
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return String::new();
    }
    if bytes[0] == b'"' || bytes[0] == b'\'' {
        let quote = bytes[0] as char;
        let rest = &value[1..];
        let end = rest.find(quote).unwrap_or(rest.len());
        rest[..end].to_string()
    } else {
        let end = bytes
            .iter()
            .position(|c| c.is_ascii_whitespace() || *c == b'>')
            .unwrap_or(bytes.len());
        value[..end].to_string()
    }
}

fn tag_is(rest: &[u8], name: &[u8]) -> bool {
    if rest.len() < name.len() || !rest[..name.len()].eq_ignore_ascii_case(name) {
        return false;
    }
    rest.get(name.len())
        .is_some_and(|c| c.is_ascii_whitespace() || *c == b'/' || *c == b'>')
}

fn start_tag_name(rest: &[u8]) -> Option<&str> {
    if !rest.first().is_some_and(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    element_name(rest)
}

fn end_tag_name(rest: &[u8]) -> Option<&str> {
    let rest = rest.strip_prefix(b"/")?;
    if !rest.first().is_some_and(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    element_name(rest)
}

fn element_name(rest: &[u8]) -> Option<&str> {
    let name_len = rest
        .iter()
        .position(|c| !c.is_ascii_alphanumeric() && *c != b'-')
        .unwrap_or(rest.len());
    std::str::from_utf8(&rest[..name_len]).ok()
}

fn is_void(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn is_content_name(name: &str) -> bool {
    !matches!(
        name.to_ascii_lowercase().as_str(),
        "html"
            | "body"
            | "head"
            | "meta"
            | "link"
            | "base"
            | "title"
            | "script"
            | "style"
            | "noscript"
            | "br"
            | "wbr"
            | "template"
    )
}

fn end_of_tag(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    let mut quote = 0u8;
    while i < bytes.len() {
        let c = bytes[i];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
            i += 1;
            continue;
        }
        if c == b'"' || c == b'\'' {
            quote = c;
            i += 1;
            continue;
        }
        if c == b'>' {
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn next_hop(status: u16, headers: &HeaderMap, hop: &Hop) -> Option<Hop> {
    let location = if is_redirect_status(status) {
        headers.get(http::header::LOCATION)?.to_str().ok()?
    } else if (200..300).contains(&status) {
        refresh_target(headers.get("refresh")?.to_str().ok()?)?
    } else {
        return None;
    };
    let url = resolve_redirect(&hop.url, location)?;
    let method = if is_redirect_status(status) {
        redirect_method(status, &hop.method)
    } else {
        Method::GET
    };
    let drop_body = method == Method::GET || method == Method::HEAD;
    let body = if drop_body {
        Body::Empty
    } else {
        hop.body.clone()
    };
    let mut headers = hop.headers.clone();
    if !same_origin(&hop.url, &url) {
        headers.remove(http::header::AUTHORIZATION);
        headers.remove(http::header::COOKIE);
        headers.remove(http::header::PROXY_AUTHORIZATION);
    }
    if drop_body {
        headers.remove(http::header::CONTENT_TYPE);
        headers.remove(http::header::CONTENT_LENGTH);
    }
    Some(Hop {
        url,
        method,
        headers,
        body,
        cookie_document: hop.cookie_document.clone(),
        cookie_kind: hop.cookie_kind,
        cookie_credentials: hop.cookie_credentials,
    })
}

fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

async fn send(hop: &Hop) -> Result<reqwest::Response, String> {
    let mut headers = hop.headers.clone();
    crate::cookies::CookieJar::global().append_request_cookies(
        &hop.url,
        hop.cookie_document.as_ref(),
        hop.cookie_kind,
        hop.cookie_credentials,
        hop.method.clone(),
        &mut headers,
    );
    if hop.method == Method::POST
        && let Some(origin) = origin_header_value(&hop.url)
        && let Ok(value) = http::HeaderValue::from_str(&origin)
    {
        headers.insert(http::header::ORIGIN, value);
    }
    let mut builder = client().request(hop.method.clone(), hop.url.clone());
    builder = builder.headers(headers);
    if !hop.headers.contains_key(http::header::USER_AGENT) {
        builder = builder.header(http::header::USER_AGENT, USER_AGENT);
    }
    builder = match &hop.body {
        Body::Empty => builder,
        Body::Bytes(bytes) => builder.body(bytes.clone()),
        Body::Form(form) => {
            let ct = hop
                .headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if ct.starts_with("multipart/") {
                return Err("multipart/form-data document navigation is not supported".to_string());
            }
            let (encoded, default_ct) = if ct.starts_with("text/plain") {
                (encode_form_text_plain(form)?, "text/plain".to_string())
            } else if ct.is_empty() || ct.starts_with("application/x-www-form-urlencoded") {
                (
                    encode_form_urlencoded(form),
                    "application/x-www-form-urlencoded".into(),
                )
            } else {
                return Err(format!("unsupported document form enctype: {ct}"));
            };
            if !hop.headers.contains_key(http::header::CONTENT_TYPE) {
                builder = builder.header(http::header::CONTENT_TYPE, default_ct);
            }
            builder.body(encoded)
        }
    };
    builder.send().await.map_err(|err| transport_reason(&err))
}

fn transport_reason(err: &reqwest::Error) -> String {
    if err.is_connect() || err.is_timeout() {
        "Couldn't reach host".to_string()
    } else {
        err.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::time::Duration;

    use super::*;

    #[test]
    fn encode_form_urlencoded_pairs() {
        use blitz_traits::net::{Entry, EntryValue, FormData};
        let form = FormData(vec![
            Entry {
                name: "q".into(),
                value: EntryValue::String("a+b".into()),
            },
            Entry {
                name: "empty".into(),
                value: EntryValue::String(String::new()),
            },
        ]);
        assert_eq!(
            String::from_utf8(encode_form_urlencoded(&form)).expect("utf8"),
            "q=a%2Bb&empty="
        );
    }

    #[test]
    fn encode_form_urlencoded_normalizes_newlines() {
        use blitz_traits::net::{Entry, EntryValue, FormData};
        let form = FormData(vec![Entry {
            name: "n".into(),
            value: EntryValue::String("a\nb".into()),
        }]);
        assert_eq!(
            String::from_utf8(encode_form_urlencoded(&form)).expect("utf8"),
            "n=a%0D%0Ab"
        );
    }

    #[test]
    fn encode_form_text_plain_uses_plain_algorithm() {
        use blitz_traits::net::{Entry, EntryValue, FormData};
        let form = FormData(vec![
            Entry {
                name: "a".into(),
                value: EntryValue::String("1".into()),
            },
            Entry {
                name: "b".into(),
                value: EntryValue::String("2".into()),
            },
        ]);
        let body = encode_form_text_plain(&form).expect("plain");
        assert_eq!(body, b"a=1\r\nb=2\r\n");
    }

    #[test]
    fn navigable_http_allows_post_form_body() {
        let mut url = Url::parse("https://example.test/post").unwrap();
        let mut req = Request::get(url.clone());
        assert!(is_navigable_http(&req));
        req.method = Method::POST;
        req.body = Body::Form(FormData::default());
        assert!(is_navigable_http(&req));
        req.method = Method::GET;
        assert!(!is_navigable_http(&req));
        url.set_scheme("file").ok();
        req.url = url;
        assert!(!is_navigable_http(&req));
    }

    #[test]
    fn redirect_method_turns_post_into_get() {
        assert_eq!(redirect_method(301, &Method::POST), Method::GET);
        assert_eq!(redirect_method(302, &Method::POST), Method::GET);
        assert_eq!(redirect_method(303, &Method::POST), Method::GET);
        assert_eq!(redirect_method(303, &Method::HEAD), Method::HEAD);
        assert_eq!(redirect_method(307, &Method::POST), Method::POST);
        assert_eq!(redirect_method(308, &Method::PUT), Method::PUT);
        assert_eq!(redirect_method(302, &Method::GET), Method::GET);
    }

    #[test]
    fn resolve_redirect_joins_relative_location() {
        let base = Url::parse("https://example.com/a/b").unwrap();
        let next = resolve_redirect(&base, "/land").unwrap();
        assert_eq!(next.as_str(), "https://example.com/land");
        assert!(resolve_redirect(&base, "javascript:alert(1)").is_none());
    }

    #[test]
    fn meta_refresh_reads_head_and_noscript_fallback() {
        let head_refresh = r#"<head><meta content="0;url=/search?q=a&amp;b=1" http-equiv="refresh"></head><body>wait</body>"#;
        for scripting in [true, false] {
            assert_eq!(
                meta_refresh_url(head_refresh, scripting).as_deref(),
                Some("/search?q=a&b=1"),
                "a real refresh applies in both scripting modes"
            );
        }

        let noscript_only = r#"<body><noscript><style>div{display:none}</style><meta content="0;url=/httpservice/retry/enablejs?sei=1" http-equiv="refresh"><div>Please click</div></noscript><script>var x=1</script></body>"#;
        assert_eq!(
            meta_refresh_url(noscript_only, false).as_deref(),
            Some("/httpservice/retry/enablejs?sei=1"),
            "with scripting disabled the noscript fallback is real markup"
        );
        assert_eq!(
            meta_refresh_url(noscript_only, true),
            None,
            "with scripting enabled the noscript contents are inert"
        );

        let with_article = r#"<body><p>results</p><noscript><meta http-equiv="refresh" content="0;url=/nojs"></noscript></body>"#;
        assert_eq!(meta_refresh_url(with_article, false), None);
        assert_eq!(meta_refresh_url(with_article, true), None);

        let google_shell = r#"<head><style>body{background:#fff}</style></head><body><noscript><style>div{display:none}</style><meta content="0;url=/httpservice/retry/enablejs?sei=1" http-equiv="refresh"><div style="display:block">Please click <a href="/httpservice/retry/enablejs?sei=1">here</a></div></noscript><script>var x=1</script><div id="yvlrue" style="display:none">trouble <a href="/search">click</a></div></body>"#;
        assert_eq!(
            meta_refresh_url(google_shell, false).as_deref(),
            Some("/httpservice/retry/enablejs?sei=1")
        );
        assert_eq!(
            meta_refresh_url(google_shell, true),
            None,
            "Google's enablejs fallback must not run while scripts are on"
        );

        let script_text = r#"<script>var s='<meta http-equiv="refresh" content="0;url=/evil">'</script><p>ok</p>"#;
        assert_eq!(meta_refresh_url(script_text, true), None);
        assert_eq!(meta_refresh_url(script_text, false), None);
        let delayed = r#"<head><meta http-equiv="refresh" content="5;url=/later"></head>"#;
        assert_eq!(meta_refresh_url(delayed, true), None);
        assert_eq!(meta_refresh_url(delayed, false), None);
    }

    #[test]
    fn refresh_target_reads_url_parameter() {
        assert_eq!(
            refresh_target("0;url=https://example.com/next"),
            Some("https://example.com/next")
        );
        assert_eq!(refresh_target("5; URL=\"/other\""), Some("/other"));
        assert!(refresh_target("10").is_none());
    }

    #[test]
    fn render_http_body_keeps_error_documents() {
        assert!(render_http_body(200, false));
        assert!(render_http_body(204, true));
        assert!(render_http_body(404, false));
        assert!(!render_http_body(404, true));
        assert!(!render_http_body(302, true));
        assert!(render_http_body(302, false));
        assert_eq!(status_phrase(404), "Not Found");
        assert_eq!(status_phrase(302), "Found");
    }

    fn http_response(status_line: &str, extra_headers: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n{body}",
            body.len()
        )
    }

    fn spawn_server(handler: impl Fn(&str) -> String + Send + Sync + 'static) -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            for _ in 0..24 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut buf = Vec::new();
                let mut tmp = [0u8; 2048];
                loop {
                    match stream.read(&mut tmp) {
                        Ok(0) => break,
                        Ok(n) => {
                            buf.extend_from_slice(&tmp[..n]);
                            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                let req = String::from_utf8_lossy(&buf).into_owned();
                let resp = handler(&req);
                let _ = stream.write_all(resp.as_bytes());
            }
        });
        port
    }

    fn local_router(req: &str) -> String {
        let path = req.split_whitespace().nth(1).unwrap_or("");
        match path {
            "/start" => http_response("302 Found", "Location: /mid\r\n", ""),
            "/mid" => http_response("301 Moved Permanently", "Location: /land\r\n", ""),
            "/land" => http_response("200 OK", "Content-Type: text/html\r\n", "<p>landed</p>"),
            "/refresh" => http_response("200 OK", "Refresh: 0;url=/land\r\n", ""),
            "/meta" => http_response(
                "200 OK",
                "Content-Type: text/html\r\n",
                "<head><meta http-equiv=\"refresh\" content=\"0;url=/land\"></head>",
            ),
            "/noscript" => http_response(
                "200 OK",
                "Content-Type: text/html\r\n",
                "<body><noscript><meta content=\"0;url=/land\" http-equiv=\"refresh\"></noscript><script>1</script></body>",
            ),
            "/keep" => http_response(
                "200 OK",
                "Content-Type: text/html\r\n",
                "<body><p>stay</p><noscript><meta http-equiv=\"refresh\" content=\"0;url=/land\"></noscript></body>",
            ),
            "/missing" => http_response(
                "404 Not Found",
                "Content-Type: text/html\r\n",
                "<p>custom-404</p>",
            ),
            "/empty" => http_response("503 Service Unavailable", "", ""),
            _ => http_response("400 Bad Request", "", ""),
        }
    }

    #[test]
    fn post_navigation_includes_origin_and_content_type() {
        use blitz_traits::net::{Entry, EntryValue, FormData};
        use std::sync::{Arc, Mutex};
        let captured = Arc::new(Mutex::new(String::new()));
        let captured2 = captured.clone();
        let port = spawn_server(move |req| {
            if req.starts_with("POST /submit") {
                *captured2.lock().expect("lock") = req.to_string();
                return http_response("200 OK", "Content-Type: text/html\r\n", "<p>ok</p>");
            }
            http_response("404 Not Found", "", "")
        });
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();
        let url = Url::parse(&format!("http://127.0.0.1:{port}/submit")).unwrap();
        let mut request = Request::get(url);
        request.method = Method::POST;
        request.content_type = Some("application/x-www-form-urlencoded".into());
        request.body = Body::Form(FormData(vec![Entry {
            name: "q".into(),
            value: EntryValue::String("x".into()),
        }]));
        match rt.block_on(fetch_document(request, false)) {
            DocumentFetch::Page { .. } => {}
            DocumentFetch::Failed { reason, .. } => panic!("post failed: {reason}"),
        }
        let req = captured.lock().expect("lock").clone();
        assert!(req.lines().any(|l| l.starts_with("POST ")));
        let expect_origin = format!("origin: http://127.0.0.1:{port}");
        assert!(
            req.lines().any(|l| l.eq_ignore_ascii_case(&expect_origin)),
            "missing Origin: {req}"
        );
        assert!(
            req.lines().any(|l| {
                l.to_ascii_lowercase()
                    .starts_with("content-type: application/x-www-form-urlencoded")
            }),
            "missing content-type: {req}"
        );
    }

    #[test]
    fn post_form_document_navigation_is_urlencoded() {
        use blitz_traits::net::{Entry, EntryValue, FormData};
        use std::sync::{Arc, Mutex};
        let posted = Arc::new(Mutex::new(Vec::new()));
        let posted2 = posted.clone();
        let port = spawn_server(move |req| {
            if req.starts_with("POST /submit") {
                let body = req.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
                posted2.lock().expect("lock").push(body);
                return http_response("200 OK", "Content-Type: text/html\r\n", "<p>posted</p>");
            }
            http_response("404 Not Found", "", "")
        });
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();
        let url = Url::parse(&format!("http://127.0.0.1:{port}/submit")).unwrap();
        let mut request = Request::get(url);
        request.method = Method::POST;
        request.body = Body::Form(FormData(vec![Entry {
            name: "user".into(),
            value: EntryValue::String("demo".into()),
        }]));
        match rt.block_on(fetch_document(request, false)) {
            DocumentFetch::Page { body, .. } => {
                assert!(String::from_utf8_lossy(&body).contains("posted"));
            }
            DocumentFetch::Failed { reason, .. } => panic!("post navigation failed: {reason}"),
        }
        let bodies = posted.lock().expect("lock");
        assert_eq!(bodies.len(), 1);
        assert_eq!(bodies[0], "user=demo");
    }

    #[test]
    fn follows_redirect_chain_and_keeps_error_body() {
        let port = spawn_server(local_router);
        let rt = crate::runtime::tokio_runtime();
        let _guard = rt.enter();

        let start = Url::parse(&format!("http://127.0.0.1:{port}/start")).unwrap();
        match rt.block_on(fetch_document(Request::get(start), false)) {
            DocumentFetch::Page { url, body, .. } => {
                assert!(url.ends_with("/land"), "{url}");
                assert!(String::from_utf8_lossy(&body).contains("landed"));
            }
            DocumentFetch::Failed { reason, .. } => panic!("redirect should land: {reason}"),
        }

        let refresh = Url::parse(&format!("http://127.0.0.1:{port}/refresh")).unwrap();
        match rt.block_on(fetch_followed(Request::get(refresh))) {
            Ok(resp) => {
                assert!(resp.redirected);
                assert!(resp.url.ends_with("/land"), "{}", resp.url);
                assert_eq!(resp.status, 200);
            }
            Err(err) => panic!("refresh should follow: {err}"),
        }

        let missing = Url::parse(&format!("http://127.0.0.1:{port}/missing")).unwrap();
        match rt.block_on(fetch_document(Request::get(missing), false)) {
            DocumentFetch::Page {
                body,
                status,
                headers,
                ..
            } => {
                assert_eq!(status, 404);
                assert!(!headers.is_empty());
                assert!(String::from_utf8_lossy(&body).contains("custom-404"));
            }
            DocumentFetch::Failed { reason, .. } => panic!("404 body should render: {reason}"),
        }

        let meta = Url::parse(&format!("http://127.0.0.1:{port}/meta")).unwrap();
        match rt.block_on(fetch_document(Request::get(meta), false)) {
            DocumentFetch::Page { url, body, .. } => {
                assert!(url.ends_with("/land"), "{url}");
                assert!(String::from_utf8_lossy(&body).contains("landed"));
            }
            DocumentFetch::Failed { reason, .. } => panic!("meta refresh should land: {reason}"),
        }

        let noscript = Url::parse(&format!("http://127.0.0.1:{port}/noscript")).unwrap();
        match rt.block_on(fetch_document(Request::get(noscript.clone()), false)) {
            DocumentFetch::Page { url, .. } => assert!(url.ends_with("/land"), "{url}"),
            DocumentFetch::Failed { reason, .. } => {
                panic!("noscript-only refresh should land: {reason}")
            }
        }
        match rt.block_on(fetch_document(Request::get(noscript), true)) {
            DocumentFetch::Page { url, .. } => assert!(url.ends_with("/noscript"), "{url}"),
            DocumentFetch::Failed { reason, .. } => {
                panic!("noscript refresh must stay inert: {reason}")
            }
        }

        let keep = Url::parse(&format!("http://127.0.0.1:{port}/keep")).unwrap();
        match rt.block_on(fetch_document(Request::get(keep), false)) {
            DocumentFetch::Page { body, .. } => {
                assert!(String::from_utf8_lossy(&body).contains("stay"));
            }
            DocumentFetch::Failed { reason, .. } => panic!("content page should stay: {reason}"),
        }

        let empty = Url::parse(&format!("http://127.0.0.1:{port}/empty")).unwrap();
        match rt.block_on(fetch_document(Request::get(empty), false)) {
            DocumentFetch::Failed { reason, .. } => {
                assert!(reason.contains("503"), "{reason}");
            }
            DocumentFetch::Page { .. } => panic!("empty 503 should be an error page"),
        }
    }

    #[test]
    fn set_cookie_echoes_on_follow_up_request() {
        use std::sync::mpsc;
        use std::sync::{Arc, Mutex};

        let _jar_guard = crate::cookies::CookieJar::lock_global_for_test();
        crate::cookies::CookieJar::reset_global();
        crate::cookies::CookieJar::ensure_process_global(None);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let seen_cookie: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let seen2 = seen_cookie.clone();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).expect("read");
                let req = String::from_utf8_lossy(&buf[..n]);
                ready_tx.send(()).expect("ready");
                if req.contains("/set") {
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nSet-Cookie: sid=alpha; Path=/\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                    )
                    .expect("write");
                } else {
                    let cookie = req
                        .lines()
                        .find(|l| l.to_ascii_lowercase().starts_with("cookie:"))
                        .and_then(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()));
                    *seen2.lock().expect("lock") = cookie;
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                    )
                    .expect("write");
                }
            }
        });
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let set = Url::parse(&format!("http://127.0.0.1:{port}/set")).unwrap();
        rt.block_on(fetch_followed(Request::get(set))).expect("set");
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("first");
        let next = Url::parse(&format!("http://127.0.0.1:{port}/next")).unwrap();
        rt.block_on(fetch_followed(Request::get(next)))
            .expect("next");
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("second");
        server.join().expect("join");
        assert_eq!(
            seen_cookie.lock().expect("lock").as_deref(),
            Some("sid=alpha")
        );
    }
}
