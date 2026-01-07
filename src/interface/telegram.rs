use std::sync::Arc;
use teloxide::{prelude::*, utils::command::BotCommands};
use tracing::{info, error};
use crate::{bus::{Event, EventBus}, manager::Manager, store::Store, entity::{TelegramUser, EntityId, Role}, chat::ChatMessage};
use uuid::Uuid;

#[derive(Clone)]
pub struct TelegramInterface {
    #[allow(dead_code)]
    bus: Arc<EventBus>,
    manager: Arc<Manager>,
    store: Arc<Store>,
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "These commands are supported:")]
enum Command {
    #[command(description = "Start the conversation and register.")]
    Start,
    #[command(description = "Display this text.")]
    Help,
    #[command(description = "List available projects.")]
    Projects,
    #[command(description = "Chat with Nereus Manager.")]
    Nereus,
}

impl TelegramInterface {
    pub fn new(bus: Arc<EventBus>, manager: Arc<Manager>, store: Arc<Store>) -> Self {
        Self { bus, manager, store }
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
                                 if let Err(e) = bot_clone.send_message(teloxide::types::ChatId(chat_id), &msg.content).await {
                                     error!("Failed to send reply to Telegram: {}", e);
                                 }
                             }
                         } else {
                             // Fallback: log it
                             info!("Agent reply received but no telegram_chat_id in metadata: {}", msg.content);
                         }
                    }
                }
            }
        });

        let whitelist_clone = whitelist.clone();

        let handler = Update::filter_message()
            .branch(
                dptree::entry()
                    .filter_command::<Command>()
                    .endpoint(move |bot, msg, cmd, interface| {
                         answer_command(bot, msg, cmd, interface, whitelist.clone())
                    })
            )
            .branch(
                dptree::entry()
                    .endpoint(move |bot, msg, interface| {
                         answer_message(bot, msg, interface, whitelist_clone.clone())
                    })
            );

        let mut builder = Dispatcher::builder(bot, handler)
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
             bot.send_message(msg.chat.id, "You are not authorized to use this bot.").await?;
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
            bot.send_message(msg.chat.id, Command::descriptions().to_string()).await?;
        }
        Command::Projects => {
            match interface.manager.list_projects().await {
                Ok(projects) => {
                    if projects.is_empty() {
                         bot.send_message(msg.chat.id, "No projects found.").await?;
                    } else {
                        let list = projects.join("\n- ");
                        bot.send_message(msg.chat.id, format!("Projects:\n- {}", list)).await?;
                    }
                }
                Err(e) => {
                    error!("Failed to list projects: {}", e);
                    bot.send_message(msg.chat.id, "Failed to retrieve project list.").await?;
                }
            }
        }
        Command::Nereus => {
             // 1. Ensure 'mothership-config' project exists/runs
             // 2. Start agent session if not started
             // 3. Set user context to talk to Nereus
             
             bot.send_message(msg.chat.id, "Connecting to Nereus Manager...").await?;
             
             let project_name = "mothership-config";
             
             // Check if project exists, if not, we can't create it easily without config files.
             // But the user said: "Make it to create a project... if it doesn't already exist."
             // AND they provided the config. I have written the config files.
             
             // Try to launch (this handles create/build/run logic in mothership runtime ideally)
             match interface.manager.launch_project(project_name.to_string()).await {
                 Ok(_) => {
                     bot.send_message(msg.chat.id, "Nereus Agent is active. You can now chat.").await?;
                 }
                 Err(e) => {
                     error!("Failed to launch Nereus: {}", e);
                     bot.send_message(msg.chat.id, format!("Failed to launch Nereus: {}", e)).await?;
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
                bot.send_message(msg.chat.id, "You are not authorized to use this bot.").await?;
                return Ok(());
            }

            if let Err(e) = interface.register_user(user).await {
                error!("Failed to register user: {}", e);
            }
            user.id.0 as i64
        } else {
            return Ok(());
        };
        
        // If the user is just chatting, forward to Nereus (agent-mothership-config)
        // ideally we should check if they are in a "session" with Nereus.
        // For now, let's hardcode routing to 'mothership-config' agent.
        
        let agent_id = EntityId::new("agent-mothership-config", "Nereus", Role::Agent);
        let user_entity_id = EntityId::new(user_id.to_string(), "TelegramUser", Role::User);
        
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("telegram_chat_id".to_string(), msg.chat.id.to_string());

        let chat_msg = ChatMessage {
            id: Uuid::new_v4().to_string(),
            chat_id: Some("telegram-direct".to_string()),
            sender: user_entity_id,
            content: text.to_string(),
            timestamp: chrono::Utc::now(),
            metadata,
        };
        
        interface.bus.publish(Event::ChatMessage(chat_msg));
        
        // bot.send_message(msg.chat.id, "Sent to Nereus.").await?;
    }
    Ok(())
}
