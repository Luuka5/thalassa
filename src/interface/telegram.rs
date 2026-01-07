use crate::{
    bus::{Event, EventBus},
    chat::ChatMessage,
    entity::{EntityId, Role, TelegramUser},
    manager::Manager,
    store::Store,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use teloxide::{prelude::*, utils::command::BotCommands};
use tracing::{error, info};
use uuid::Uuid;

#[derive(Debug, Clone)]
struct ChatSession {
    chat_id: i64,
    active_project: String,
    agent_id: EntityId,
}

#[derive(Clone)]
pub struct TelegramInterface {
    #[allow(dead_code)]
    bus: Arc<EventBus>,
    manager: Arc<Manager>,
    store: Arc<Store>,
    chat_sessions: Arc<Mutex<HashMap<i64, ChatSession>>>,
}

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
enum Command {
    #[command(description = "Start the conversation and register.")]
    Start,
    #[command(description = "Display this text.")]
    Help,
    #[command(description = "List available projects.")]
    Projects,
    #[command(description = "Enter a project: /enter <project-name>")]
    Enter(String),
}

impl TelegramInterface {
    pub fn new(bus: Arc<EventBus>, manager: Arc<Manager>, store: Arc<Store>) -> Self {
        Self {
            bus,
            manager,
            store,
            chat_sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn get_active_project(&self, chat_id: i64) -> Option<ChatSession> {
        let sessions = self.chat_sessions.lock().unwrap();
        sessions.get(&chat_id).cloned()
    }

    fn set_active_project(&self, chat_id: i64, project_name: String) {
        let agent_id = EntityId::new(
            format!("agent-{}", project_name),
            format!("Agent ({})", project_name),
            Role::Agent,
        );

        let session = ChatSession {
            chat_id,
            active_project: project_name,
            agent_id,
        };

        let mut sessions = self.chat_sessions.lock().unwrap();
        sessions.insert(chat_id, session);
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let token = std::env::var("TELOXIDE_TOKEN")
            .or_else(|_| std::env::var("TELEGRAM_BOT_TOKEN"))
            .map_err(|_| anyhow::anyhow!("TELOXIDE_TOKEN or TELEGRAM_BOT_TOKEN not set"))?;

        // Parse whitelist
        let whitelist_str = std::env::var("TELEGRAM_WHITELIST").unwrap_or_default();
        let whitelist: Vec<String> = whitelist_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if whitelist.is_empty() {
            info!("Warning: No TELEGRAM_WHITELIST configured. All users will be denied access.");
        } else {
            info!("Telegram whitelist loaded: {:?}", whitelist);
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(130))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()?;

        let bot = Bot::with_client(token, client);
        let interface = self.clone();

        info!("Starting Telegram bot...");

        // Spawn listener for Agent replies
        let mut bus_rx = self.bus.subscribe();
        let bot_clone = bot.clone();

        // We need to map internal IDs to Telegram ChatIds.
        // For MVP, we'll store the last seen ChatId for a given UserID in memory or DB.
        // Or simpler: Assuming 1-on-1 with the whitelisted user for now.
        // Since we don't have a reliable mapping in this scope without `Arc<Mutex<State>>`,
        // We will assume that if we see a message on the bus directed at us (Role::Agent -> Role::User),
        // we try to send it to the user.
        // But wait, the `ChatMessage` doesn't have the telegram ChatID.
        // The `sender` is `TelegramUser:<id>`. So we can extract the ID.
        // The ID in `TelegramUser` struct (entity) was `user.id.0` (which is the Telegram User ID).

        tokio::spawn(async move {
            while let Ok(event) = bus_rx.recv().await {
                if let Event::ChatMessage(msg) = event {
                    if msg.sender.role == Role::Agent {
                        // This is a reply from an Agent
                        // We need to send it to the Telegram User.
                        // But WHO is the recipient?
                        // The Agent replies don't strictly specify a recipient in `ChatMessage` struct yet
                        // except implicitly by being in a "chat session".
                        // However, for this bridge, the AgentSession just broadcasts the reply.
                        // We need to look at who started the conversation or metadata.

                        // In `bridge.rs`, we publish the reply.
                        // The reply's `chat_id` is None or whatever we set.
                        // The original message had `chat_id: Some("telegram-direct")`.
                        // We could use metadata to carry the original Telegram ChatID.

                        // BUT, simpler MVP hack:
                        // Just send it to the whitelist user(s) if we can resolve them.
                        // OR, we assume the `msg.content` is what we want to send.

                        // Ideally, `bridge.rs` should copy the `chat_id` from the incoming message to the reply.
                        // Let's assume we fix `bridge.rs` to do that, or we rely on `metadata`.

                        // Let's parse the user ID from somewhere.
                        // Actually, in `bridge.rs`, the reply sender is Agent.
                        // The recipient is implied.

                        // Workaround: We will send this message to the ChatId found in the whitelisted user's session
                        // if we had one.
                        // Since we don't, and `teloxide::ChatId` is needed...
                        // We'll rely on the fact that `TelegramUser` entity ID *IS* the Telegram User ID.
                        // So if we knew who the message was for...

                        // Let's modify `bridge.rs` later to include `recipient` field in ChatMessage or metadata.
                        // For now, I will hardcode sending to the `chat_id` stored in a global map? No.

                        // Let's look at `bridge.rs` again.
                        // It replies to the bus.

                        // I will assume for now that I can just extract the target user from the context
                        // OR I will simply broadcast to the active user I last saw.
                        // This is brittle but works for single-user MVP.

                        // BETTER: Let's use `metadata` in `bridge.rs` to echo back the `telegram_chat_id`.
                        // For now, check if metadata has "telegram_chat_id".
                        // If not, we can try to guess from the content or just send to whitelisted user if we can find their chat ID.
                        // But we don't store chat ID in TelegramUser entity yet (only user ID).
                        // We need to store ChatID in the Store when registering user, or pass it in metadata.

                        // Update `answer_message` to put chat_id in metadata.

                        if let Some(chat_id_str) = msg.metadata.get("telegram_chat_id") {
                            if let Ok(chat_id) = chat_id_str.parse::<i64>() {
                                if let Err(e) = bot_clone
                                    .send_message(teloxide::types::ChatId(chat_id), &msg.content)
                                    .await
                                {
                                    error!("Failed to send reply to Telegram: {}", e);
                                }
                            }
                        } else {
                            // Fallback: log it
                            info!(
                                "Agent reply received but no telegram_chat_id in metadata: {}",
                                msg.content
                            );
                        }
                    }
                }
            }
        });

        let whitelist_clone = whitelist.clone();
        let whitelist_clone2 = whitelist.clone();

        let handler = Update::filter_message()
            .branch(dptree::entry().filter_command::<Command>().endpoint(
                move |bot, msg, cmd, interface| {
                    answer_command(bot, msg, cmd, interface, whitelist.clone())
                },
            ))
            .branch(dptree::entry().endpoint(move |bot, msg, interface| {
                answer_message(bot, msg, interface, whitelist_clone.clone())
            }));

        let callback_handler =
            Update::filter_callback_query().endpoint(move |bot, q, interface| {
                handle_callback_query(bot, q, interface, whitelist_clone2.clone())
            });

        let mut builder = Dispatcher::builder(
            bot,
            dptree::entry().branch(handler).branch(callback_handler),
        )
        .dependencies(dptree::deps![interface])
        .enable_ctrlc_handler();

        // In production/server environments, the default polling might have issues with
        // ipv6 or other networking quirks. Let's explicitly build the error handling.
        builder.build().dispatch().await;

        Ok(())
    }

    async fn register_user(&self, user: &teloxide::types::User) -> anyhow::Result<()> {
        let telegram_user = TelegramUser {
            id: user.id.0 as i64, // teloxide UserIds are u64, but we store i64 in DB for sqlite compat if needed, casting is safe-ish for now
            username: user.username.clone(),
            first_name: user.first_name.clone(),
        };
        self.store.save_telegram_user(&telegram_user).await?;
        Ok(())
    }
}

async fn answer_command(
    bot: Bot,
    msg: Message,
    cmd: Command,
    interface: TelegramInterface,
    whitelist: Vec<String>,
) -> ResponseResult<()> {
    // Attempt registration on every command interaction to ensure user exists
    if let Some(user) = msg.from() {
        if !whitelist.contains(&user.username.clone().unwrap_or_default()) {
            bot.send_message(msg.chat.id, "You are not authorized to use this bot.")
                .await?;
            return Ok(());
        }

        if let Err(e) = interface.register_user(user).await {
            error!("Failed to register user: {}", e);
            // We continue anyway
        }
    }

    match cmd {
        Command::Start => {
            bot.send_message(msg.chat.id, "Welcome to Mothership! 🚀\nI am Thalassa, your interface.\nUse /help to see what I can do.").await?;
        }
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?;
        }
        Command::Projects => {
            let current_project = interface.get_active_project(msg.chat.id.0);

            match interface.manager.list_projects().await {
                Ok(projects) => {
                    if projects.is_empty() {
                        bot.send_message(msg.chat.id, "No projects found.").await?;
                    } else {
                        let mut list = String::new();
                        for project in &projects {
                            if let Some(ref session) = current_project {
                                if &session.active_project == project {
                                    list.push_str(&format!("→ {}\n", project));
                                    continue;
                                }
                            }
                            list.push_str(&format!("  {}\n", project));
                        }

                        let header = if current_project.is_some() {
                            "Projects (→ = active):\n"
                        } else {
                            "Projects:\n"
                        };

                        bot.send_message(msg.chat.id, format!("{}{}", header, list))
                            .await?;
                    }
                }
                Err(e) => {
                    error!("Failed to list projects: {}", e);
                    bot.send_message(msg.chat.id, "Failed to retrieve project list.")
                        .await?;
                }
            }
        }
        Command::Enter(project_name) => {
            let project_name = project_name.trim().to_string();

            if project_name.is_empty() {
                bot.send_message(
                    msg.chat.id,
                    "Usage: /enter <project-name>\n\nUse /projects to see available projects.",
                )
                .await?;
                return Ok(());
            }

            // Check if project exists
            match interface.manager.list_projects().await {
                Ok(projects) => {
                    if !projects.contains(&project_name) {
                        bot.send_message(
                            msg.chat.id,
                            format!("Project '{}' not found.\n\nUse /projects to see available projects.", project_name)
                        ).await?;
                        return Ok(());
                    }
                }
                Err(e) => {
                    error!("Failed to list projects: {}", e);
                    bot.send_message(msg.chat.id, "Failed to retrieve project list.")
                        .await?;
                    return Ok(());
                }
            }

            // Launch the project
            bot.send_message(msg.chat.id, format!("Launching {}...", project_name))
                .await?;

            match interface.manager.launch_project(project_name.clone()).await {
                Ok(_) => {
                    // Set as active project for this chat
                    interface.set_active_project(msg.chat.id.0, project_name.clone());

                    bot.send_message(
                        msg.chat.id,
                        format!(
                            "✓ Entered [{}]\n\nYou can now chat with this project.",
                            project_name
                        ),
                    )
                    .await?;
                }
                Err(e) => {
                    error!("Failed to launch project: {}", e);
                    bot.send_message(
                        msg.chat.id,
                        format!("Failed to launch {}: {}", project_name, e),
                    )
                    .await?;
                }
            }
        }
    };
    Ok(())
}

async fn answer_message(
    bot: Bot,
    msg: Message,
    interface: TelegramInterface,
    whitelist: Vec<String>,
) -> ResponseResult<()> {
    // If it's a text message that wasn't a command
    if let Some(text) = msg.text() {
        // Attempt registration
        let user_id = if let Some(user) = msg.from() {
            if !whitelist.contains(&user.username.clone().unwrap_or_default()) {
                bot.send_message(msg.chat.id, "You are not authorized to use this bot.")
                    .await?;
                return Ok(());
            }

            if let Err(e) = interface.register_user(user).await {
                error!("Failed to register user: {}", e);
            }
            user.id.0 as i64
        } else {
            return Ok(());
        };

        // Check if chat has an active project
        let session = interface.get_active_project(msg.chat.id.0);

        if session.is_none() {
            // No active project - show project picker with clickable buttons
            match interface.manager.list_projects().await {
                Ok(projects) => {
                    if projects.is_empty() {
                        bot.send_message(
                            msg.chat.id,
                            "No projects available. Please configure projects first.",
                        )
                        .await?;
                    } else {
                        // Create inline keyboard with project buttons
                        use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

                        let buttons: Vec<Vec<InlineKeyboardButton>> = projects
                            .iter()
                            .map(|project| {
                                vec![InlineKeyboardButton::callback(
                                    project.clone(),
                                    format!("enter:{}", project),
                                )]
                            })
                            .collect();

                        let keyboard = InlineKeyboardMarkup::new(buttons);

                        bot.send_message(msg.chat.id, "Please select a project to enter:")
                            .reply_markup(keyboard)
                            .await?;
                    }
                }
                Err(e) => {
                    error!("Failed to list projects: {}", e);
                    bot.send_message(msg.chat.id, "Failed to retrieve project list. Use /enter <project-name> to enter manually.")
                        .await?;
                }
            }
            return Ok(());
        }

        // Has active project - route message to agent
        let session = session.unwrap();
        let user_entity_id = EntityId::new(user_id.to_string(), "TelegramUser", Role::User);

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("telegram_chat_id".to_string(), msg.chat.id.to_string());
        metadata.insert("project_name".to_string(), session.active_project.clone());

        let chat_msg = ChatMessage {
            id: Uuid::new_v4().to_string(),
            chat_id: Some("telegram-direct".to_string()),
            sender: user_entity_id,
            content: text.to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        };

        interface.bus.publish(Event::ChatMessage(chat_msg));
    }
    Ok(())
}

async fn handle_callback_query(
    bot: Bot,
    q: teloxide::types::CallbackQuery,
    interface: TelegramInterface,
    whitelist: Vec<String>,
) -> ResponseResult<()> {
    // Check authorization
    let user = &q.from;
    if !whitelist.contains(&user.username.clone().unwrap_or_default()) {
        bot.answer_callback_query(&q.id)
            .text("You are not authorized to use this bot.")
            .await?;
        return Ok(());
    }

    // Parse callback data
    if let Some(data) = &q.data {
        if let Some(project_name) = data.strip_prefix("enter:") {
            let project_name = project_name.to_string();

            // Get chat_id from the message
            let chat_id = if let Some(ref msg) = q.message {
                msg.chat.id
            } else {
                bot.answer_callback_query(&q.id)
                    .text("Error: Could not determine chat")
                    .await?;
                return Ok(());
            };

            // Verify project exists
            match interface.manager.list_projects().await {
                Ok(projects) => {
                    if !projects.contains(&project_name) {
                        bot.answer_callback_query(&q.id)
                            .text(format!("Project '{}' not found", project_name))
                            .show_alert(true)
                            .await?;
                        return Ok(());
                    }
                }
                Err(e) => {
                    error!("Failed to list projects: {}", e);
                    bot.answer_callback_query(&q.id)
                        .text("Failed to retrieve project list")
                        .show_alert(true)
                        .await?;
                    return Ok(());
                }
            }

            // Launch the project
            match interface.manager.launch_project(project_name.clone()).await {
                Ok(_) => {
                    // Set as active project for this chat
                    interface.set_active_project(chat_id.0, project_name.clone());

                    // Answer the callback query
                    bot.answer_callback_query(&q.id)
                        .text(format!("Entered {}", project_name))
                        .await?;

                    // Edit the original message to show success
                    if let Some(msg) = q.message {
                        bot.edit_message_text(
                            msg.chat.id,
                            msg.id,
                            format!(
                                "✓ Entered [{}]\n\nYou can now chat with this project.",
                                project_name
                            ),
                        )
                        .await?;
                    }
                }
                Err(e) => {
                    error!("Failed to launch project: {}", e);
                    bot.answer_callback_query(&q.id)
                        .text(format!("Failed to launch {}: {}", project_name, e))
                        .show_alert(true)
                        .await?;
                }
            }
        } else {
            bot.answer_callback_query(&q.id)
                .text("Unknown action")
                .await?;
        }
    }

    Ok(())
}
