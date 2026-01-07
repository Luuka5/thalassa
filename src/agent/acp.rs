use serde::{Deserialize, Serialize};
use serde_json::Value;

// JSON-RPC 2.0 Types

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
    pub id: Option<Value>, // None for notifications
}

impl JsonRpcRequest {
    pub fn new(method: &str, params: Option<Value>, id: Option<u64>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id: id.map(|i| i.into()),
        }
    }

    pub fn notification(method: &str, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

// ACP Specific Payload Types

#[derive(Debug, Serialize)]
pub struct InitializeParams {
    pub protocolVersion: u32,
    pub clientCapabilities: ClientCapabilities,
    pub clientInfo: ClientInfo,
}

#[derive(Debug, Serialize)]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fs: Option<FsCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct FsCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readTextFile: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub writeTextFile: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ClientInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct SessionNewParams {
    pub cwd: String,
    pub mcpServers: Vec<McpServer>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum McpServer {
    Stdio {
        name: String,
        command: String,
        args: Vec<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        env: Vec<EnvVariable>,
    },
    Http {
        #[serde(rename = "type")]
        transport_type: String, // "http"
        name: String,
        url: String,
        headers: Vec<HttpHeader>,
    },
    Sse {
        #[serde(rename = "type")]
        transport_type: String, // "sse"
        name: String,
        url: String,
        headers: Vec<HttpHeader>,
    },
}

#[derive(Debug, Serialize)]
pub struct EnvVariable {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct SessionPromptParams {
    pub sessionId: String,
    pub prompt: Vec<ContentBlock>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    // Add image/resource types later
}
