use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use axum::{
    extract::{State, Json},
    response::{sse::{Event, Sse}, IntoResponse},
    routing::{get, post},
    Router,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing::{info, error};

use crate::manager::Manager;

// -----------------------------------------------------------------------------
// MCP Protocol Types (Simplified for basic SSE/JSON-RPC transport)
// -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(tag = "method")]
pub enum JsonRpcRequest {
    #[serde(rename = "initialize")]
    Initialize { params: InitializeParams, id: Value },
    #[serde(rename = "tools/list")]
    ListTools { params: Option<Value>, id: Value },
    #[serde(rename = "tools/call")]
    CallTool { params: CallToolParams, id: Value },
    // Catch-all for other methods we don't support yet, or notifications
    #[serde(untagged)]
    Unknown { method: String, params: Option<Value>, id: Option<Value> },
}

#[derive(Debug, Deserialize)]
pub struct InitializeParams {
    pub protocolVersion: String,
    pub capabilities: Value,
    pub clientInfo: Value,
}

#[derive(Debug, Deserialize)]
pub struct CallToolParams {
    pub name: String,
    pub arguments: Option<HashMap<String, Value>>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message }),
        }
    }
}

// -----------------------------------------------------------------------------
// Server State
// -----------------------------------------------------------------------------

pub struct McpState {
    pub manager: Arc<Manager>,
    pub tx: broadcast::Sender<String>, // Broadcast channel for SSE
}

// -----------------------------------------------------------------------------
// Implementation
// -----------------------------------------------------------------------------

pub struct McpServer {
    manager: Arc<Manager>,
}

impl McpServer {
    pub fn new(manager: Arc<Manager>) -> Self {
        Self { manager }
    }

    pub fn router(&self) -> Router {
        let (tx, _rx) = broadcast::channel(100);
        let state = Arc::new(McpState {
            manager: self.manager.clone(),
            tx,
        });

        Router::new()
            .route("/sse", get(sse_handler))
            .route("/messages", post(messages_handler))
            .with_state(state)
            .layer(CorsLayer::permissive())
    }
}

async fn sse_handler(
    State(state): State<Arc<McpState>>,
) -> Sse<impl Stream<Item = Result<Event, axum::BoxError>>> {
    info!("New SSE connection established");
    
    // Create a new receiver for this connection
    let mut rx = state.tx.subscribe();

    let stream = async_stream::stream! {
        // Send initial connection endpoint event as per MCP spec for SSE
        // The client needs to know where to send POST messages
        let endpoint_event = Event::default()
            .event("endpoint")
            .data("/messages");
        yield Ok(endpoint_event);

        loop {
            match rx.recv().await {
                Ok(msg) => {
                    yield Ok(Event::default().data(msg));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    // Handle lag if necessary
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

#[axum::debug_handler]
async fn messages_handler(
    State(state): State<Arc<McpState>>,
    Json(request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    info!("Received MCP message: {:?}", request);

    match request {
        JsonRpcRequest::Initialize { params, id } => {
            info!("Initializing MCP session: client={:?}", params.clientInfo);
            
            let result = serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "thalassa-mcp",
                    "version": "0.1.0"
                }
            });

            Json(JsonRpcResponse::success(id, result))
        }

        JsonRpcRequest::ListTools { id, .. } => {
            let tools = vec![
                serde_json::json!({
                    "name": "list_projects",
                    "description": "List all available projects",
                    "inputSchema": {
                        "type": "object",
                        "properties": {},
                    }
                }),
                serde_json::json!({
                    "name": "launch_project",
                    "description": "Launch a project by name",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Name of the project to launch" }
                        },
                        "required": ["name"]
                    }
                }),
                serde_json::json!({
                    "name": "exec_command",
                    "description": "Execute a command in a project's container",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "project": { "type": "string", "description": "Project name" },
                            "command": { "type": "string", "description": "Command to execute" }
                        },
                        "required": ["project", "command"]
                    }
                })
            ];

            let result = serde_json::json!({
                "tools": tools
            });
            
            Json(JsonRpcResponse::success(id, result))
        }

        JsonRpcRequest::CallTool { params, id } => {
            let result = match params.name.as_str() {
                "list_projects" => {
                    match state.manager.list_projects().await {
                        Ok(projects) => {
                            // Format as text for now, or could return raw list
                            let content = projects.join(", ");
                            Ok(serde_json::json!({
                                "content": [{
                                    "type": "text",
                                    "text": content
                                }]
                            }))
                        }
                        Err(e) => Err(e.to_string()),
                    }
                }
                "launch_project" => {
                    let name = params.arguments.as_ref()
                        .and_then(|args| args.get("name"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "Missing 'name' argument".to_string());

                    match name {
                        Ok(n) => {
                            match state.manager.launch_project(n.to_string()).await {
                                Ok(_) => Ok(serde_json::json!({
                                    "content": [{
                                        "type": "text",
                                        "text": format!("Launched project: {}", n)
                                    }]
                                })),
                                Err(e) => Err(e.to_string())
                            }
                        }
                        Err(e) => Err(e),
                    }
                }
                "exec_command" => {
                    let args = params.arguments.as_ref();
                    let project = args.and_then(|a| a.get("project")).and_then(|v| v.as_str());
                    let command = args.and_then(|a| a.get("command")).and_then(|v| v.as_str());

                    match (project, command) {
                        (Some(p), Some(c)) => {
                            match state.manager.exec_command(p.to_string(), c.to_string()).await {
                                Ok(output) => Ok(serde_json::json!({
                                    "content": [{
                                        "type": "text",
                                        "text": output
                                    }]
                                })),
                                Err(e) => Err(e.to_string())
                            }
                        }
                        _ => Err("Missing 'project' or 'command' argument".to_string())
                    }
                }
                unknown => Err(format!("Unknown tool: {}", unknown))
            };

            match result {
                Ok(val) => Json(JsonRpcResponse::success(id, val)),
                Err(e) => Json(JsonRpcResponse::error(id, -32000, e)),
            }
        }

        JsonRpcRequest::Unknown { method, id, .. } => {
            error!("Unknown method: {}", method);
            if let Some(req_id) = id {
                Json(JsonRpcResponse::error(req_id, -32601, format!("Method not found: {}", method)))
            } else {
                // Notification, no response needed (or we can't respond without ID)
                 Json(JsonRpcResponse::error(Value::Null, -32600, "Invalid Request".to_string()))
            }
        }
    }
}
