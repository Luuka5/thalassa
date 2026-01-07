use crate::agent::client::AcpClient;
use crate::bus::{Event, EventBus, NotificationLevel};
use crate::chat::ChatMessage;
use crate::entity::{EntityId, Role};
use mothership::runtime::Runtime;
use std::sync::Arc;
use tokio::task;
use tracing::{debug, error, info};
use uuid::Uuid;

pub struct AgentSession {
    project_name: String,
    session_id: String,                                      // Internal Bridge ID
    acp_session_id: Arc<tokio::sync::Mutex<Option<String>>>, // ACP Session ID
    agent_id: EntityId,
    event_bus: Arc<EventBus>,
    runtime: Arc<Runtime>,
    acp_client: Arc<tokio::sync::Mutex<Option<Arc<AcpClient>>>>,
    // Store metadata for ongoing conversation to attach to streaming chunks
    current_metadata: Arc<tokio::sync::Mutex<Option<std::collections::HashMap<String, String>>>>,
    // Accumulator for chunks to send as complete messages
    chunk_accumulator: Arc<tokio::sync::Mutex<String>>,
}

impl AgentSession {
    pub fn new(
        project_name: String,
        agent_id: EntityId,
        event_bus: Arc<EventBus>,
        runtime: Arc<Runtime>,
    ) -> Self {
        let session_id = format!("ses_{}", Uuid::new_v4().simple());

        Self {
            project_name,
            session_id,
            acp_session_id: Arc::new(tokio::sync::Mutex::new(None)),
            agent_id,
            event_bus,
            runtime,
            acp_client: Arc::new(tokio::sync::Mutex::new(None)),
            current_metadata: Arc::new(tokio::sync::Mutex::new(None)),
            chunk_accumulator: Arc::new(tokio::sync::Mutex::new(String::new())),
        }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        let bus_rx = self.event_bus.subscribe();
        let runtime = self.runtime.clone();
        let project_name = self.project_name.clone();
        let session_id = self.session_id.clone(); // Bridge Session ID
        let event_bus = self.event_bus.clone();
        let agent_id = self.agent_id.clone();
        let acp_client_arc = self.acp_client.clone();
        let acp_session_id_arc = self.acp_session_id.clone();
        let current_metadata_arc = self.current_metadata.clone();
        let chunk_accumulator_arc = self.chunk_accumulator.clone();

        // Initialize ACP Connection
        info!("Starting ACP Session for {}", project_name);

        let child = runtime.spawn_exec(&project_name, "opencode acp")?;
        let client = Arc::new(AcpClient::new(child)?);

        {
            let mut guard = acp_client_arc.lock().await;
            *guard = Some(client.clone());
        }

        // Initialize Protocol
        match client.initialize().await {
            Ok(_) => info!("ACP Initialized successfully"),
            Err(e) => {
                error!("ACP Initialize failed: {}", e);
                // We should probably retry or fail hard
            }
        }

        // Create Agent Session
        let cwd = format!("/home/devuser/projects/{}", project_name);
        match client.new_session(&cwd).await {
            Ok(sid) => {
                info!("Agent Session Created: {}", sid);
                let mut session_id_guard = acp_session_id_arc.lock().await;
                *session_id_guard = Some(sid);
            }
            Err(e) => error!("Failed to create agent session: {}", e),
        }

        event_bus.publish(Event::SystemNotification {
            level: NotificationLevel::Success,
            message: format!("Agent session {} started for {}", session_id, project_name),
            target: None,
        });

        // Spawn Notification Listener - just accumulate chunks silently
        let client_clone = client.clone();
        let accumulator_for_updates = chunk_accumulator_arc.clone();

        task::spawn(async move {
            let mut rx = client_clone.notification_tx.subscribe();

            while let Ok(notification) = rx.recv().await {
                // Handle session/update
                if notification.method == "session/update" {
                    debug!("Received update: {:?}", notification.params);

                    // Extract text from session/update notifications
                    if let Some(params) = &notification.params {
                        if let Some(update) = params.get("update") {
                            // Check for agent_message_chunk updates
                            if let Some(session_update) = update.get("sessionUpdate") {
                                if session_update.as_str() == Some("agent_message_chunk") {
                                    // Extract content from the update
                                    if let Some(content) = update.get("content") {
                                        if let Some(text) =
                                            content.get("text").and_then(|t| t.as_str())
                                        {
                                            // Just accumulate, don't send yet
                                            let mut guard = accumulator_for_updates.lock().await;
                                            guard.push_str(text);
                                            debug!(
                                                "Accumulated {} chars (total: {})",
                                                text.len(),
                                                guard.len()
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        // Spawn Message Listener Task
        let acp_session_id_for_prompt = acp_session_id_arc.clone();
        let metadata_for_prompt = current_metadata_arc.clone();
        let accumulator_for_prompt = chunk_accumulator_arc.clone();
        task::spawn(async move {
            let mut rx = bus_rx;
            while let Ok(event) = rx.recv().await {
                if let Event::ChatMessage(msg) = event {
                    if msg.sender.role == Role::User {
                        info!("Bridge received message from User: {}", msg.content);

                        let client_ref = {
                            let guard = acp_client_arc.lock().await;
                            guard.clone()
                        };

                        if let Some(client) = client_ref {
                            let content = msg.content.clone();
                            let original_metadata = msg.metadata.clone();
                            let bus = event_bus.clone();
                            let a_id = agent_id.clone();
                            let session_id_clone = acp_session_id_for_prompt.clone();
                            let metadata_clone = metadata_for_prompt.clone();
                            let accumulator_clone = accumulator_for_prompt.clone();

                            // We spawn a separate task to handle the prompt exchange so we don't block the bus listener
                            task::spawn(async move {
                                // Clear the accumulator for this new turn
                                {
                                    let mut guard = accumulator_clone.lock().await;
                                    guard.clear();
                                }

                                // Store the metadata for this conversation turn
                                {
                                    let mut guard = metadata_clone.lock().await;
                                    *guard = Some(original_metadata.clone());
                                }

                                // Get the ACP session ID
                                let session_id = {
                                    let guard = session_id_clone.lock().await;
                                    guard.clone()
                                };

                                if let Some(sid) = session_id {
                                    // 1. Send Prompt and get response
                                    match client.prompt(&sid, &content).await {
                                        Ok(_response) => {
                                            // 2. Get the accumulated text
                                            let accumulated_text = {
                                                let guard = accumulator_clone.lock().await;
                                                guard.clone()
                                            };

                                            if !accumulated_text.is_empty() {
                                                // Get project name from metadata for prefix
                                                let project_name_for_prefix = original_metadata
                                                    .get("project_name")
                                                    .map(|s| s.as_str())
                                                    .unwrap_or("unknown");

                                                // Strip leading newline if present
                                                let trimmed_text =
                                                    accumulated_text.trim_start_matches('\n');

                                                // Add project name prefix to response with one newline
                                                let prefixed_content = format!(
                                                    "[{}]\n{}",
                                                    project_name_for_prefix, trimmed_text
                                                );

                                                info!(
                                                    "Sending accumulated response: {} chars",
                                                    accumulated_text.len()
                                                );
                                                let reply = ChatMessage {
                                                    id: Uuid::new_v4().to_string(),
                                                    chat_id: None,
                                                    sender: a_id.clone(),
                                                    content: prefixed_content,
                                                    timestamp: chrono::Utc::now(),
                                                    metadata: original_metadata.clone(),
                                                };
                                                bus.publish(Event::ChatMessage(reply));
                                            } else {
                                                info!("Agent returned no content");
                                            }
                                        }
                                        Err(e) => {
                                            error!("Agent prompt failed: {}", e);
                                            bus.publish(Event::SystemNotification {
                                                level: NotificationLevel::Error,
                                                message: format!("Agent failed to reply: {}", e),
                                                target: None,
                                            });
                                        }
                                    }
                                } else {
                                    error!("Cannot send prompt: ACP session not initialized");
                                    bus.publish(Event::SystemNotification {
                                        level: NotificationLevel::Error,
                                        message: "Agent session not ready".to_string(),
                                        target: None,
                                    });
                                }
                            });
                        } else {
                            error!("ACP Client not available");
                        }
                    }
                }
            }
        });

        Ok(())
    }
}

/// Extract text from ACP response
/// Tries multiple common JSON paths where the agent might put the response text
fn extract_text_from_response(response: &crate::agent::acp::JsonRpcResponse) -> String {
    use tracing::debug;

    if let Some(result) = &response.result {
        debug!("Extracting text from response result: {:?}", result);

        // Try path: result.content[0].text (common in ACP implementations)
        if let Some(content_array) = result.get("content").and_then(|v| v.as_array()) {
            if let Some(first_item) = content_array.first() {
                if let Some(text) = first_item.get("text").and_then(|v| v.as_str()) {
                    return text.to_string();
                }
            }
        }

        // Try path: result.message.content
        if let Some(message) = result.get("message") {
            if let Some(content) = message.get("content").and_then(|v| v.as_str()) {
                return content.to_string();
            }
        }

        // Try path: result.text (simple case)
        if let Some(text) = result.get("text").and_then(|v| v.as_str()) {
            return text.to_string();
        }

        // Try: result itself might be a string
        if let Some(text) = result.as_str() {
            return text.to_string();
        }

        // Fallback: serialize as JSON for debugging
        debug!(
            "Could not extract text from standard paths, result structure: {:?}",
            result
        );
    }

    String::new()
}
