use crate::agent::acp::{
    ClientCapabilities, ClientInfo, ContentBlock, FsCapabilities, InitializeParams, JsonRpcRequest,
    JsonRpcResponse, SessionNewParams, SessionPromptParams,
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::Child;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task;
use tracing::{debug, error, info, warn};

pub struct AcpClient {
    tx_request: mpsc::Sender<JsonRpcRequest>,
    pending_requests: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>>,
    pub notification_tx: broadcast::Sender<JsonRpcRequest>,
    request_id_counter: Arc<Mutex<u64>>,
}

impl AcpClient {
    pub fn new(mut child: Child) -> Result<Self> {
        let stdin = child.stdin.take().context("Failed to take stdin")?;
        let stdout = child.stdout.take().context("Failed to take stdout")?;

        let (tx_request, mut rx_request) = mpsc::channel::<JsonRpcRequest>(100);
        let (notification_tx, _) = broadcast::channel(100);

        let pending_requests: Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let pending_requests_clone = pending_requests.clone();
        let notification_tx_clone = notification_tx.clone();

        // Stdin Writer Task (Blocking)
        task::spawn_blocking(move || {
            let mut stdin = stdin;
            while let Some(req) = rx_request.blocking_recv() {
                let json_str = match serde_json::to_string(&req) {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Failed to serialize request: {}", e);
                        continue;
                    }
                };

                debug!("-> Sending to Agent: {}", json_str);

                // Using Line-Delimited JSON
                if let Err(e) = writeln!(stdin, "{}", json_str) {
                    error!("Failed to write to agent stdin: {}", e);
                    break;
                }
            }
            debug!("Stdin writer task finished");
        });

        // Stdout Reader Task (Blocking)
        task::spawn_blocking(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if line.trim().is_empty() {
                            continue;
                        }
                        debug!("<- Received from Agent: {}", line);

                        // Try parsing as Response first
                        if let Ok(response) = serde_json::from_str::<JsonRpcResponse>(&line) {
                            // It's a response to one of our requests
                            let id_str = response.id.to_string(); // Simple normalization
                                                                  // Remove quotes if string id
                            let id_clean = id_str.trim_matches('"').to_string();

                            let sender = {
                                let mut pending = pending_requests_clone.lock().unwrap();
                                pending.remove(&id_clean)
                            };

                            if let Some(tx) = sender {
                                let _ = tx.send(response);
                            } else {
                                warn!("Received response for unknown ID: {}", id_clean);
                            }
                        } else if let Ok(request) = serde_json::from_str::<JsonRpcRequest>(&line) {
                            // It's a notification or method call from the agent
                            let _ = notification_tx_clone.send(request);
                        } else {
                            error!("Failed to parse agent message: {}", line);
                        }
                    }
                    Err(e) => {
                        error!("Error reading from agent stdout: {}", e);
                        break;
                    }
                }
            }
            debug!("Stdout reader task finished");
            // Optionally wait for child
            let _ = child.wait();
        });

        Ok(Self {
            tx_request,
            pending_requests,
            notification_tx,
            request_id_counter: Arc::new(Mutex::new(1)),
        })
    }

    pub async fn send_request(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<JsonRpcResponse> {
        let id = {
            let mut counter = self.request_id_counter.lock().unwrap();
            let id = *counter;
            *counter += 1;
            id
        };

        let req = JsonRpcRequest::new(method, params, Some(id));
        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self.pending_requests.lock().unwrap();
            pending.insert(id.to_string(), tx);
        }

        self.tx_request
            .send(req)
            .await
            .context("Failed to send request to writer loop")?;

        let response = rx.await.context("Response channel closed")?;
        Ok(response)
    }

    pub async fn send_notification(&self, method: &str, params: Option<Value>) -> Result<()> {
        let req = JsonRpcRequest::notification(method, params);
        self.tx_request
            .send(req)
            .await
            .context("Failed to send notification")?;
        Ok(())
    }

    // --- High Level Methods ---

    pub async fn initialize(&self) -> Result<()> {
        let params = InitializeParams {
            protocolVersion: 1,
            clientCapabilities: ClientCapabilities {
                fs: Some(FsCapabilities {
                    readTextFile: Some(true),
                    writeTextFile: Some(true),
                }),
                terminal: Some(true),
            },
            clientInfo: ClientInfo {
                name: "Thalassa".to_string(),
                title: Some("Thalassa Orchestrator".to_string()),
                version: "0.1.0".to_string(),
            },
        };

        let response = self
            .send_request("initialize", Some(serde_json::to_value(params)?))
            .await?;

        if let Some(err) = response.error {
            anyhow::bail!("Initialize failed: {} ({})", err.message, err.code);
        }

        info!("ACP Initialized: {:?}", response.result);
        Ok(())
    }

    pub async fn new_session(&self, cwd: &str) -> Result<String> {
        let params = SessionNewParams {
            cwd: cwd.to_string(),
            mcpServers: vec![], // Empty for now, can be extended later
        };

        let response = self
            .send_request("session/new", Some(serde_json::to_value(params)?))
            .await?;

        if let Some(err) = response.error {
            anyhow::bail!("session/new failed: {}", err.message);
        }

        // Parse result. Assuming result is { "sessionId": "..." } or just the ID string?
        // Let's assume the result object contains the ID.
        let result = response.result.context("No result in session/new")?;

        // Try to extract sessionId
        let session_id = result
            .get("sessionId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| result.as_str().map(|s| s.to_string())) // Maybe it returns just the string
            .context("Could not parse sessionId from result")?;

        Ok(session_id)
    }

    pub async fn prompt(&self, session_id: &str, content: &str) -> Result<JsonRpcResponse> {
        let params = SessionPromptParams {
            sessionId: session_id.to_string(),
            prompt: vec![ContentBlock::Text {
                text: content.to_string(),
            }],
        };

        // session/prompt returns when the turn is complete.
        // The response contains the final result/stopReason.
        // Real-time updates come via session/update notifications.

        let response = self
            .send_request("session/prompt", Some(serde_json::to_value(params)?))
            .await?;

        if let Some(err) = &response.error {
            anyhow::bail!("session/prompt failed: {}", err.message);
        }

        Ok(response)
    }
}
