use crate::bus::{Event, EventBus, NotificationLevel};
use crate::chat::ChatMessage;
use crate::entity::{EntityId, Role};
use mothership::runtime::Runtime;
use std::sync::Arc;
use tokio::task;
use uuid::Uuid;

pub struct AgentSession {
    project_name: String,
    session_id: String,
    agent_id: EntityId,
    event_bus: Arc<EventBus>,
    runtime: Arc<Runtime>,
}

impl AgentSession {
    pub fn new(
        project_name: String,
        agent_id: EntityId,
        event_bus: Arc<EventBus>,
        runtime: Arc<Runtime>,
    ) -> Self {
        // Generate a stable session ID for this bridge instance
        // In the future, we might want to persist this or allow resuming previous sessions
        let session_id = format!("ses_{}", Uuid::new_v4().simple());
        
        Self {
            project_name,
            session_id,
            agent_id,
            event_bus,
            runtime,
        }
    }

    /// Starts the bridge listener. 
    /// Since we use stateless CLI commands, we don't need to "spawn" a process here.
    /// We just start listening to the EventBus to trigger commands.
    pub async fn start(&self) -> anyhow::Result<()> {
        let bus_rx = self.event_bus.subscribe();
        let runtime = self.runtime.clone();
        let project_name = self.project_name.clone();
        let session_id = self.session_id.clone();
        let event_bus = self.event_bus.clone();
        let agent_id = self.agent_id.clone();

        // Notify that the session is ready
        event_bus.publish(Event::SystemNotification {
            level: NotificationLevel::Success,
            message: format!("Agent session {} started for {}", session_id, project_name),
            target: None,
        });

        // Spawn the listener task
        task::spawn(async move {
            let mut rx = bus_rx;
            while let Ok(event) = rx.recv().await {
                if let Event::ChatMessage(msg) = event {
                    // Filter: Only process messages FROM User, meant for this context
                    // (For now, we assume all User messages in the system go to the active agent)
                    if msg.sender.role == Role::User {
                        println!("Bridge received message from User: {}", msg.content);
                        // 1. Construct the command
                        // We need to carefully escape the message content for the shell
                        // Using a simple replacement for now, but ideally we'd pass it via stdin or env var
                        // to avoid shell injection, but exec_capture uses bash -c.
                        // Safe approach: echo "content" | opencode run -s id (if opencode supports stdin)
                        // Or strictly escaping single quotes.
                        let safe_content = msg.content.replace("'", "'\\''");
                        let cmd = format!("opencode run '{}'", safe_content);
                        // We are ignoring session_id for now because it fails if it doesn't exist?
                        // Actually, let's try to create a session first if we want persistence.
                        // But opencode creates one automatically.
                        // The issue is reusing it.
                        // If we pass -s with a non-existent ID, opencode errors out.
                        // We should probably list sessions or handle the error.
                        // For MVP, let's just run without -s, which creates a NEW session every time (stateless).
                        // This matches "stateless CLI commands" comment.
                        // Wait, user wants a conversation.
                        // So we NEED the session ID.
                        // The error was "Resource not found: .../ses_12345.json".
                        // This means we cannot pass an arbitrary ID. We must use an EXISTING ID or let it generate one.
                        // We can't easily capture the generated ID from the output unless we parse it?
                        // Opencode output doesn't seem to print the session ID in the response.
                        
                        // Strategy:
                        // 1. On first message, run WITHOUT -s.
                        // 2. Parse the session ID? No, we can't reliably.
                        // 3. Explicitly create a session using `opencode session new`? No command for that?
                        //    Wait, `opencode session` has subcommands? The help said "opencode session manage sessions".
                        //    Let's check `opencode session --help` again.
                        
                        // If `opencode` CLI requires an existing session ID to reuse context, we must create one.
                        // If `opencode` doesn't support "create with this ID", we are stuck unless we find a way.
                        
                        // Workaround:
                        // Just run it. The "stateless" nature means no context.
                        // If the user said "how's it going", the agent said "hello".
                        // If we want context, we need to solve the session ID issue.
                        
                        // Let's assume for this specific debug step we drop -s to at least make it respond.
                        // Then we can figure out session management.


                        // 2. Execute sync (blocking the listener loop is fine for now, or spawn inner task)
                        // We spawn inner generic blocking task to avoid blocking the event loop
                        let rt_clone = runtime.clone();
                        let p_name = project_name.clone();
                        let a_id = agent_id.clone();
                        let bus_clone = event_bus.clone();
                        let cmd_clone = cmd.clone();
                        let original_metadata = msg.metadata.clone();

                        task::spawn_blocking(move || {
                            match rt_clone.exec_capture(&p_name, &cmd_clone) {
                                Ok(output) => {
                                    if !output.trim().is_empty() {
                                        let reply = ChatMessage {
                                            id: Uuid::new_v4().to_string(),
                                            chat_id: None, // Can link to original msg chat_id if available
                                            sender: a_id,
                                            content: output,
                                            timestamp: chrono::Utc::now(),
                                            metadata: original_metadata,
                                        };
                                        bus_clone.publish(Event::ChatMessage(reply));
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Agent execution error: {}", e);
                                    bus_clone.publish(Event::SystemNotification {
                                        level: NotificationLevel::Error,
                                        message: format!("Agent failed to reply: {}", e),
                                        target: None,
                                    });
                                }
                            }
                        });
                    }
                }
            }
        });

        Ok(())
    }
}
