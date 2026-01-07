use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId {
    pub id: String,
    pub name: String,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Agent,
}

impl EntityId {
    pub fn new(id: impl Into<String>, name: impl Into<String>, role: Role) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            role,
        }
    }

    pub fn system() -> Self {
        Self {
            id: "system".to_string(),
            name: "System".to_string(),
            role: Role::System,
        }
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Entity {
    System,
    User(TelegramUser),
    Agent(AgentEntity),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramUser {
    pub id: i64,
    pub username: Option<String>,
    pub first_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEntity {
    pub project_name: String,
}

impl Entity {
    pub fn id(&self) -> EntityId {
        match self {
            Entity::System => EntityId::system(),
            Entity::User(u) => EntityId::new(
                format!("telegram:{}", u.id),
                u.username.clone().unwrap_or(u.first_name.clone()),
                Role::User,
            ),
            Entity::Agent(a) => EntityId::new(
                format!("agent:{}", a.project_name),
                format!("Agent ({})", a.project_name),
                Role::Agent,
            ),
        }
    }
}
