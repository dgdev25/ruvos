// crates/ruvos-mcp/src/protocol.rs

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String, // must be "2.0"
    pub method: String,  // e.g., "initialize", "tools/list", "tools/call"
    #[serde(default)]
    pub params: Value, // optional; notifications/some methods omit it
    // id is absent for notifications (e.g. notifications/initialized)
    #[serde(default, deserialize_with = "deserialize_id")]
    pub id: Option<String>, // request ID (converted to string), None = notification
}

// Accept both string and numeric IDs per JSON-RPC 2.0 spec
fn deserialize_id<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(Value::Number(n)) => Ok(Some(n.to_string())),
        Some(_) => Err(Error::custom("id must be a string or number")),
    }
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String, // always "2.0"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    // null id is valid JSON-RPC (e.g. parse errors before id is known)
    pub id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Option<String>, result: Value) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn error(id: Option<String>, code: i32, message: String) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
            id,
        }
    }
}
