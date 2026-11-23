//! Serialize Blitz form entry lists for the page `FormData` constructor.

use std::io::Read;
use std::path::Path;

use base64::Engine as _;
use blitz_dom::BaseDocument;
use blitz_traits::net::EntryValue;

use super::bridge::DomValue;

pub fn reply_form_data_entries(
    doc: &BaseDocument,
    form_id: blitz_traits::node_id::NodeId,
    submitter_id: blitz_traits::node_id::NodeId,
    reply: u64,
) -> super::bridge::DomResponse {
    let entries = blitz_dom::form::construct_entry_list(doc, form_id, submitter_id);
    let mut json_entries = Vec::new();
    for entry in entries.iter() {
        let mut obj = serde_json::Map::new();
        obj.insert("name".into(), serde_json::Value::String(entry.name.clone()));
        match &entry.value {
            EntryValue::String(s) => {
                obj.insert("valueKind".into(), serde_json::Value::String("text".into()));
                obj.insert("data".into(), serde_json::Value::String(s.clone()));
            }
            EntryValue::File(path) => {
                let (filename, content_type, data_b64) = match read_file_part(path) {
                    Ok(v) => v,
                    Err(reason) => {
                        return super::bridge::DomResponse::err(reply, reason);
                    }
                };
                obj.insert("valueKind".into(), serde_json::Value::String("file".into()));
                obj.insert("filename".into(), serde_json::Value::String(filename));
                obj.insert("type".into(), serde_json::Value::String(content_type));
                obj.insert("dataBase64".into(), serde_json::Value::String(data_b64));
            }
            EntryValue::EmptyFile => {
                obj.insert("valueKind".into(), serde_json::Value::String("file".into()));
                obj.insert("filename".into(), serde_json::Value::String(String::new()));
                obj.insert(
                    "type".into(),
                    serde_json::Value::String("application/octet-stream".into()),
                );
                obj.insert(
                    "dataBase64".into(),
                    serde_json::Value::String(String::new()),
                );
            }
        }
        json_entries.push(serde_json::Value::Object(obj));
    }
    super::bridge::DomResponse::ok(
        reply,
        DomValue::Json(serde_json::Value::Array(json_entries)),
    )
}

fn read_file_part(path: &Path) -> Result<(String, String, String), String> {
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("upload")
        .to_string();
    let mut file = std::fs::File::open(path).map_err(|e| format!("form file open failed: {e}"))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| format!("form file read failed: {e}"))?;
    let data_b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok((filename, "application/octet-stream".into(), data_b64))
}
