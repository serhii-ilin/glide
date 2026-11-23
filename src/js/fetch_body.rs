//! Decode page `fetch()` / XHR bodies from the JSON RPC shape into [`Request`] bodies.

use base64::Engine as _;
use blitz_traits::net::{Body, Bytes, Request};

use crate::http::{MultipartField, MultipartValue};

/// Apply the JSON `body` field from a fetch RPC onto `request`.
pub fn apply_fetch_body(request: &mut Request, body_json: &str) -> Result<(), String> {
    if body_json.trim().is_empty() {
        request.body = Body::Empty;
        return Ok(());
    }
    let value: serde_json::Value =
        serde_json::from_str(body_json).map_err(|_| "invalid fetch body json".to_string())?;
    let kind = value.get("kind").and_then(|k| k.as_str()).unwrap_or("text");
    match kind {
        "text" => {
            let data = value.get("data").and_then(|d| d.as_str()).unwrap_or("");
            request.body = if data.is_empty() {
                Body::Empty
            } else {
                Body::Bytes(Bytes::from(data.as_bytes().to_vec()))
            };
        }
        "blob" => {
            let bytes = decode_base64_field(&value, "data")?;
            if bytes.is_empty() {
                request.body = Body::Empty;
            } else {
                request.body = Body::Bytes(Bytes::from(bytes));
            }
            let content_type = value
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("application/octet-stream");
            if !content_type.is_empty() && !request.headers.contains_key(http::header::CONTENT_TYPE)
            {
                request.content_type = Some(content_type.to_string());
                let header_value = http::header::HeaderValue::from_str(content_type)
                    .map_err(|_| "invalid blob content-type".to_string())?;
                request
                    .headers
                    .insert(http::header::CONTENT_TYPE, header_value);
            }
        }
        "urlSearchParams" => {
            let pairs = parse_pairs(&value)?;
            let encoded = crate::http::encode_url_search_params(&pairs);
            request.body = Body::Bytes(Bytes::from(encoded));
            if !request.headers.contains_key(http::header::CONTENT_TYPE) {
                const CT: &str = "application/x-www-form-urlencoded;charset=UTF-8";
                request.content_type = Some(CT.into());
                let header_value = http::header::HeaderValue::from_str(CT)
                    .map_err(|_| "invalid content-type".to_string())?;
                request
                    .headers
                    .insert(http::header::CONTENT_TYPE, header_value);
            }
        }
        "formData" => {
            let fields = parse_form_data_entries(&value)?;
            let (boundary, body) = crate::http::encode_multipart_form_data(&fields)?;
            request.body = Body::Bytes(Bytes::from(body));
            if !request.headers.contains_key(http::header::CONTENT_TYPE) {
                let ct = format!("multipart/form-data; boundary={boundary}");
                request.content_type = Some(ct.clone());
                let header_value = http::header::HeaderValue::from_str(&ct)
                    .map_err(|_| "invalid multipart content-type".to_string())?;
                request
                    .headers
                    .insert(http::header::CONTENT_TYPE, header_value);
            }
        }
        other => return Err(format!("unsupported fetch body kind: {other}")),
    }
    Ok(())
}

fn decode_base64_field(value: &serde_json::Value, key: &str) -> Result<Vec<u8>, String> {
    let b64 = value.get(key).and_then(|d| d.as_str()).unwrap_or("");
    if b64.is_empty() {
        return Ok(Vec::new());
    }
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|_| "invalid base64 body".to_string())
}

fn parse_pairs(value: &serde_json::Value) -> Result<Vec<(String, String)>, String> {
    let Some(arr) = value.get("pairs").and_then(|p| p.as_array()) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for item in arr {
        let Some(pair) = item.as_array() else {
            continue;
        };
        if pair.len() < 2 {
            continue;
        }
        let name = pair[0].as_str().unwrap_or("").to_string();
        let val = pair[1].as_str().unwrap_or("").to_string();
        out.push((name, val));
    }
    Ok(out)
}

pub(crate) fn parse_form_data_entries(
    value: &serde_json::Value,
) -> Result<Vec<MultipartField>, String> {
    let Some(arr) = value.get("entries").and_then(|e| e.as_array()) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for item in arr {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let name = obj
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let entry_kind = obj
            .get("valueKind")
            .and_then(|k| k.as_str())
            .unwrap_or("text");
        let field = match entry_kind {
            "file" => MultipartField {
                name,
                value: MultipartValue::File {
                    bytes: decode_base64_field(
                        &serde_json::Value::Object(obj.clone()),
                        "dataBase64",
                    )?,
                    filename: obj
                        .get("filename")
                        .and_then(|f| f.as_str())
                        .unwrap_or("blob")
                        .to_string(),
                    content_type: obj
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("application/octet-stream")
                        .to_string(),
                },
            },
            _ => MultipartField {
                name,
                value: MultipartValue::Text(
                    obj.get("data")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string(),
                ),
            },
        };
        out.push(field);
    }
    Ok(out)
}

use blitz_traits::net::http;

#[cfg(test)]
mod tests {
    use super::*;
    use blitz_traits::net::Method;

    #[test]
    fn url_search_params_body_sets_form_content_type() {
        let mut req =
            blitz_traits::net::Request::get(url::Url::parse("http://127.0.0.1/").unwrap());
        req.method = Method::POST;
        apply_fetch_body(
            &mut req,
            r#"{"kind":"urlSearchParams","pairs":[["a","1"],["b","two"]]}"#,
        )
        .expect("apply");
        let Body::Bytes(bytes) = req.body else {
            panic!("expected bytes");
        };
        assert_eq!(&bytes[..], b"a=1&b=two");
        let ct = req
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.starts_with("application/x-www-form-urlencoded"));
    }

    #[test]
    fn form_data_keeps_existing_content_type_header() {
        let mut req =
            blitz_traits::net::Request::get(url::Url::parse("http://127.0.0.1/").unwrap());
        req.method = Method::POST;
        let header_value = http::header::HeaderValue::from_static("text/plain");
        req.headers.insert(http::header::CONTENT_TYPE, header_value);
        apply_fetch_body(
            &mut req,
            r#"{"kind":"formData","entries":[{"name":"a","valueKind":"text","data":"x"}]}"#,
        )
        .expect("apply");
        let ct = req
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .expect("ct");
        assert_eq!(ct, "text/plain");
        let Body::Bytes(body) = req.body else {
            panic!("bytes");
        };
        assert!(body.starts_with(b"--"));
    }

    #[test]
    fn url_search_params_content_type_is_exact_charset_suffix() {
        let mut req =
            blitz_traits::net::Request::get(url::Url::parse("http://127.0.0.1/").unwrap());
        req.method = Method::POST;
        apply_fetch_body(
            &mut req,
            r#"{"kind":"urlSearchParams","pairs":[["q","a b"]]}"#,
        )
        .expect("apply");
        let ct = req
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .expect("ct");
        assert_eq!(ct, "application/x-www-form-urlencoded;charset=UTF-8");
    }

    #[test]
    fn blob_body_uses_declared_type() {
        let mut req =
            blitz_traits::net::Request::get(url::Url::parse("http://127.0.0.1/").unwrap());
        req.method = Method::POST;
        apply_fetch_body(
            &mut req,
            r#"{"kind":"blob","type":"text/plain","data":"aGVsbG8="}"#,
        )
        .expect("apply");
        let Body::Bytes(bytes) = req.body else {
            panic!("expected bytes");
        };
        assert_eq!(&bytes[..], b"hello");
        let ct = req
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(ct, "text/plain");
    }
}
