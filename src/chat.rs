use crate::entity::EntityId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub chat_id: Option<String>,
    pub sender: EntityId,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub participant_a: EntityId,
    pub participant_b: EntityId,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}

impl ChatSession {
    pub fn new(id: impl Into<String>, participant_a: EntityId, participant_b: EntityId) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            participant_a,
            participant_b,
            created_at: now,
            last_active_at: now,
        }
    }
}
