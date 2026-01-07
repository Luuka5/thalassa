use crate::chat::ChatMessage;
use crate::entity::EntityId;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Event {
    /// A new chat message was sent
    ChatMessage(ChatMessage),

    /// A system notification (e.g., container started, build failed)
    SystemNotification {
        level: NotificationLevel,
        message: String,
        target: Option<EntityId>, // If None, broadcast to everyone
    },

    /// A scheduled job triggered
    ScheduledEvent { job_id: String, payload: String },

    /// Configuration changed
    ConfigChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
    Success,
}

pub struct EventBus {
    tx: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(100);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    pub fn publish(&self, event: Event) {
        // We ignore the error if there are no receivers
        let _ = self.tx.send(event);
    }
}
