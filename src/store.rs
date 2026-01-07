use crate::{
    chat::ChatMessage,
    entity::{EntityId, Role},
};
use anyhow::{Context, Result};
use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions, Row, SqlitePool};
use std::{collections::HashMap, path::Path, str::FromStr};

#[derive(Clone, Debug)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Create a new Store instance.
    /// This will automatically create the database file if it doesn't exist.
    pub async fn new(db_path: impl AsRef<Path>) -> Result<Self> {
        let db_path = db_path.as_ref();

        // Ensure the parent directory exists
        if let Some(parent) = db_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).context("Failed to create database directory")?;
            }
        }

        let db_url = format!("sqlite://{}", db_path.to_string_lossy());

        let options = SqliteConnectOptions::from_str(&db_url)?
            .create_if_missing(true)
            .log_statements(tracing::log::LevelFilter::Trace);

        let pool = SqlitePool::connect_with(options)
            .await
            .context("Failed to connect to SQLite database")?;

        Ok(Self { pool })
    }

    /// Initialize the database schema.
    pub async fn init(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                chat_id TEXT,
                sender TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp DATETIME NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_messages_chat_timestamp ON messages(chat_id, timestamp DESC);
            
            CREATE TABLE IF NOT EXISTS telegram_users (
                id INTEGER PRIMARY KEY,
                username TEXT,
                first_name TEXT NOT NULL,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            "#
        )
        .execute(&self.pool)
        .await
        .context("Failed to initialize database schema")?;

        Ok(())
    }

    /// Save a chat message to the store.
    pub async fn save_message(&self, msg: &ChatMessage) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO messages (id, chat_id, sender, content, timestamp)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&msg.id)
        .bind(&msg.chat_id)
        .bind(msg.sender.to_string())
        .bind(&msg.content)
        .bind(msg.timestamp)
        .execute(&self.pool)
        .await
        .context("Failed to save message")?;

        Ok(())
    }

    /// Retrieve chat history for a specific chat session.
    /// Returns messages ordered by timestamp ascending (oldest to newest).
    pub async fn get_chat_history(&self, chat_id: &str, limit: i64) -> Result<Vec<ChatMessage>> {
        let rows = sqlx::query(
            r#"
            SELECT id, chat_id, sender, content, timestamp
            FROM messages
            WHERE chat_id = ?
            ORDER BY timestamp DESC
            LIMIT ?
            "#,
        )
        .bind(chat_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch chat history")?;

        let mut messages = Vec::with_capacity(rows.len());

        for row in rows {
            let sender_str: String = row.try_get("sender")?;
            // We need to deserialize the sender string back into an EntityId
            // But wait, EntityId::new takes (id, name, role).
            // We only stored a string representation.
            // Ideally we should store JSON or normalized fields.
            // For now, let's assume the string format is "Name (ID)" and parse it, or just use a default role.
            // Actually, `sender.to_string()` output format is `Name (ID)`.
            // Let's just create a generic "Historical" entity if we can't parse perfectly,
            // or better yet, fix `save_message` to store structured data if we want structured read.
            // For this iteration, let's treat it as a generic User/Agent based on content or just Unknown role.

            let sender = if sender_str.starts_with("Agent") {
                EntityId::new(sender_str.clone(), sender_str, Role::Agent)
            } else if sender_str == "System (system)" {
                EntityId::system()
            } else {
                EntityId::new(sender_str.clone(), sender_str, Role::User)
            };

            messages.push(ChatMessage {
                id: row.try_get("id")?,
                chat_id: row.try_get("chat_id")?,
                sender,
                content: row.try_get("content")?,
                timestamp: row.try_get("timestamp")?,
                metadata: HashMap::new(), // Metadata not stored in DB yet
            });
        }

        // Return in chronological order (oldest -> newest)
        messages.reverse();

        Ok(messages)
    }

    /// Save or update a Telegram user.
    pub async fn save_telegram_user(&self, user: &crate::entity::TelegramUser) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO telegram_users (id, username, first_name)
            VALUES (?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                username = excluded.username,
                first_name = excluded.first_name
            "#,
        )
        .bind(user.id)
        .bind(&user.username)
        .bind(&user.first_name)
        .execute(&self.pool)
        .await
        .context("Failed to save telegram user")?;

        Ok(())
    }
}
