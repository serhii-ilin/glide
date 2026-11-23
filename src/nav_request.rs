//! Map Blitz [`NavigationOptions`] into HTTP [`Request`] values for page loads.

use blitz_traits::navigation::NavigationOptions;
use blitz_traits::net::http::{self, HeaderValue, Method};
use blitz_traits::net::{Body, EntryValue, Request};

/// Bus command payload for non-GET top-level navigations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageNavigationRequest {
    pub url: String,
    pub method: String,
    pub content_type: Option<String>,
    pub form_entries: Vec<(String, String)>,
}

impl PageNavigationRequest {
    pub fn from_navigation_options(options: NavigationOptions) -> Self {
        let content_type = options.content_type.clone();
        let method = options.method.to_string();
        let url = options.url.to_string();
        let form_entries = match &options.document_resource {
            Body::Form(form) => form
                .iter()
                .map(|entry| {
                    let value = match &entry.value {
                        EntryValue::String(s) => s.clone(),
                        EntryValue::File(p) => p.to_string_lossy().into_owned(),
                        EntryValue::EmptyFile => String::new(),
                    };
                    (entry.name.clone(), value)
                })
                .collect(),
            _ => Vec::new(),
        };
        Self {
            url,
            method,
            content_type,
            form_entries,
        }
    }

    pub fn to_request(&self) -> Result<Request, String> {
        let url = url::Url::parse(&self.url).map_err(|e| e.to_string())?;
        let method: Method = self
            .method
            .parse()
            .map_err(|_| format!("unsupported method {}", self.method))?;
        let mut headers = http::HeaderMap::new();
        if let Some(ct) = self.content_type.as_deref() {
            let value = HeaderValue::from_str(ct).map_err(|e| e.to_string())?;
            headers.insert(http::header::CONTENT_TYPE, value);
        }
        let body = if self.form_entries.is_empty() {
            Body::Empty
        } else {
            Body::Form(blitz_traits::net::FormData(
                self.form_entries
                    .iter()
                    .map(|(name, value)| blitz_traits::net::Entry {
                        name: name.clone(),
                        value: EntryValue::String(value.clone()),
                    })
                    .collect(),
            ))
        };
        let mut request = Request::get(url);
        request.method = method;
        request.content_type = self.content_type.clone();
        request.headers = headers;
        request.body = body;
        Ok(request)
    }
}

/// Build a document navigation [`Request`] from Blitz navigation options.
pub fn navigation_options_to_document_request(options: NavigationOptions) -> Request {
    let content_type = options.content_type.clone();
    let mut request = options.into_request();
    if let Some(ct) = content_type.as_deref()
        && let Ok(value) = HeaderValue::from_str(ct)
    {
        request.headers.insert(http::header::CONTENT_TYPE, value);
    }
    request
}

#[cfg(test)]
mod tests {
    use super::*;
    use blitz_traits::net::{Entry, FormData};

    #[test]
    fn navigation_options_to_document_request_post_urlencoded() {
        let url = url::Url::parse("https://example.test/login").unwrap();
        let form = FormData(vec![
            Entry {
                name: "user".into(),
                value: EntryValue::String("demo".into()),
            },
            Entry {
                name: "go".into(),
                value: EntryValue::String("1".into()),
            },
        ]);
        let options =
            NavigationOptions::new(url, Some("application/x-www-form-urlencoded".into()), 1)
                .set_method(Method::POST)
                .set_document_resource(Body::Form(form));
        let req = navigation_options_to_document_request(options);
        assert_eq!(req.method, Method::POST);
        assert_eq!(
            req.headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/x-www-form-urlencoded")
        );
        let Body::Form(body) = req.body else {
            panic!("expected form body");
        };
        assert_eq!(body.iter().count(), 2);
    }
}
