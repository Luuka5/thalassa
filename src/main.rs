use std::sync::Arc;
use tracing::{error, info};

mod agent; // Added agent module
mod bus;
mod chat;
mod entity;
mod interface;
mod manager;
mod mcp;
mod store; // Added interface module

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file
    if let Err(e) = dotenvy::dotenv() {
        // It's not fatal if .env doesn't exist, but good to know
        info!("No .env file found or failed to load: {}", e);
    }

    // Initialize logging with default filter if RUST_LOG is not set
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    info!("Thalassa daemon starting...");

    // Initialize the EventBus
    let bus = Arc::new(bus::EventBus::new());

    // Initialize the Store
    // We use ~/.mothership/thalassa.db
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let db_path = std::path::Path::new(&home_dir)
        .join(".mothership")
        .join("thalassa.db");

    info!("Initializing store at {}", db_path.display());
    let store = store::Store::new(&db_path).await?;
    store.init().await?;

    // Initialize the Manager
    let manager = Arc::new(manager::Manager::new(bus.clone())?);

    // Spawn the scheduler in the background
    let manager_clone = manager.clone();
    let scheduler_handle = tokio::spawn(async move {
        info!("Starting scheduler...");
        manager_clone.start_scheduler().await;
    });

    // Initialize MCP Server
    let mcp_server = mcp::server::McpServer::new(manager.clone());
    let app = mcp_server.router();

    let port = 3000;
    info!("Starting MCP server on port {}", port);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;

    // Initialize Telegram Interface if token is present
    let telegram_interface = {
        if std::env::var("TELOXIDE_TOKEN").is_ok() || std::env::var("TELEGRAM_BOT_TOKEN").is_ok() {
            Some(interface::telegram::TelegramInterface::new(
                bus.clone(),
                manager.clone(),
                Arc::new(store.clone()),
            ))
        } else {
            info!("No Telegram token found, skipping Telegram bot startup.");
            None
        }
    };

    // We need to manage the lifetimes and async tasks properly.
    // We'll use a JoinSet or just separate spawns.

    let telegram_handle = tokio::spawn(async move {
        if let Some(telegram) = telegram_interface {
            if let Err(e) = telegram.run().await {
                error!("Telegram bot stopped with error: {}", e);
            }
        } else {
            // Keep the task alive but doing nothing if disabled, or just exit.
            // Exiting is fine.
            std::future::pending::<()>().await;
        }
    });

    // Run both the scheduler and the web server
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
        }
        _ = scheduler_handle => {
            info!("Scheduler stopped unexpectedly");
        }
        res = axum::serve(listener, app) => {
            if let Err(e) = res {
                info!("Server stopped with error: {}", e);
            }
        }
        _ = telegram_handle => {
             error!("Telegram handle finished unexpectedly");
        }
    }

    Ok(())
}
