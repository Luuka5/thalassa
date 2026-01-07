# Thalassa: Implementation Plan & Specification

## 1. Program Specification

**Name:** Thalassa
**Purpose:** An orchestration daemon that manages Mothership development environments, providing intelligent agency and persistent interaction via MCP and Telegram.

### Core Architecture
*   **Daemon:** A long-running process managed by `tokio`.
*   **Event Bus:** A central broadcasting system for decoupling components.
*   **Entities:**
    *   `User`: A human operator (identified by Telegram ID or MCP session).
    *   `Agent`: An AI or automated actor.
    *   `System`: Internal system notifications/logs.

### Data Model
*   **Chat:** A linear history of interactions (backed by SQLite).
*   **Message:** Contains `sender` (Entity), `content` (Text/Block), `timestamp`, and `metadata`.
*   **Event:** System-wide signals (e.g., `ContainerStarted`, `MessageReceived`).

### Interfaces
*   **MCP (Model Context Protocol):** Exposes tools and resources via Server-Sent Events (SSE) using `axum`. Allows LLMs to control Mothership.
*   **Telegram:** Provides a human interface via a Bot API (using `teloxide`).

---

## 2. Detailed Implementation Plan

### Phase 1: Mothership Core Enhancements
Before Thalassa can fully function, the core `mothership` library needs specific updates.
*   **Goal:** Enable command execution within containers.
*   **Tasks:**
    1.  **Implement `exec`:** Add functionality to `mothership::Runtime` to execute non-interactive commands inside running containers.
    2.  **API Exposure:** Ensure the `exec` method returns `Result<Output, Error>` (capturing stdout/stderr).
    3.  **Verification:** Add unit tests in `mothership` to verify `exec` works on a dummy container.

### Phase 2: Thalassa Foundation
Setting up the internal plumbing of the daemon.
*   **Stack:** Rust, Tokio, SQLx (SQLite).
*   **Tasks:**
    1.  **Project Setup:** Initialize `thalassa` crate (if not exists) with `tokio` (full features).
    2.  **Event Bus:** Implement a typed `EventBus` using `tokio::sync::broadcast`.
        *   Enum: `ThalassaEvent` (`Message(Message)`, `System(SystemEvent)`).
    3.  **Entity System:** Define `Entity` trait and `EntityManager` to resolve IDs to names/types.
    4.  **Persistence:**
        *   Setup `sqlx` with SQLite.
        *   Schema: `chats`, `messages`, `entities`.
        *   Implement `ChatRepository` for history retrieval and storage.

### Phase 3: The Manager
The brain that wraps the Mothership library.
*   **Tasks:**
    1.  **Runtime Wrapper:** Create a `Manager` struct that holds the `mothership::Runtime` instance.
    2.  **State Management:** Track the state of environments (Up/Down) in memory, syncing with the actual Docker state via `mothership`.
    3.  **Scheduler:** Implement a simple loop or `tokio-cron-scheduler` for periodic tasks (e.g., checking container health, pruning logs).

### Phase 4: Interfaces
Connecting the daemon to the outside world.
*   **MCP Server (Machine Interface):**
    *   **Lib:** `axum` for HTTP/SSE.
    *   **Transport:** Implement SSE transport for MCP.
    *   **Endpoints:** `/sse` (connection), `/messages` (POST for JSON-RPC).
    *   **Tools:** Expose `mothership` capabilities as MCP Tools (`list_containers`, `up`, `down`, `exec`).
*   **Telegram Bot (Human Interface):**
    *   **Lib:** `teloxide`.
    *   **Handler:** Listen for text messages, wrap them in `ThalassaEvent`, and push to the Event Bus.
    *   **Response:** Listen to Event Bus for replies targeting the Telegram user.

### Phase 5: Integration & Loop
*   **Tasks:**
    1.  **Wiring:** Instantiate `EventBus`, `Manager`, `Database`, `HttpServer`, and `Bot` in `main.rs`.
    2.  **Flow:**
        *   User sends Telegram message -> Bot -> EventBus.
        *   Agent/System processes Event -> Calls `Manager` (e.g., `exec "ls"`) -> EventBus.
        *   Bot/MCP picks up result -> Sends back to User.
