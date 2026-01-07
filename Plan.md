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

### Phase 1: Mothership Core Enhancements (Complete)
*   **Goal:** Enable command execution within containers.
*   **Status:** 
    *   [x] Implemented `exec_capture` in `mothership::Runtime`.
    *   [x] Implemented `spawn_exec` for persistent process spawning (ACP support).

### Phase 2: Thalassa Foundation (Complete)
*   **Stack:** Rust, Tokio, SQLx (SQLite).
*   **Status:**
    *   [x] Project Setup (`thalassa` crate).
    *   [x] Event Bus (`src/bus.rs`).
    *   [x] Entity System (`src/entity.rs`).
    *   [x] Persistence (`src/store.rs`).

### Phase 3: The Manager (Complete)
*   **Status:**
    *   [x] Runtime Wrapper (`src/manager.rs`).
    *   [x] State Management.
    *   [ ] Scheduler Logic (Placeholder exists in `src/manager.rs`, need implementation).

### Phase 4: Interfaces (Complete)
*   **Status:**
    *   [x] MCP Server (`src/mcp/server.rs`).
    *   [x] Telegram Bot (`src/interface/telegram.rs`).
    *   [x] Agent Bridge using ACP Protocol (`src/agent/bridge.rs`, `src/agent/client.rs`, `src/agent/acp.rs`).

### Phase 5: Integration & Loop (In Progress)
*   **Status:**
    *   [x] Wiring (`src/main.rs`).
    *   [ ] Full End-to-End Verification.

### Phase 6: Refinement & Robustness (In Progress)
Address architectural fragility identified in the initial implementation.
*   **Tasks:**
    1.  **Agent Bridge Persistence:** [x] COMPLETE
        *   Implemented ACP (Agent Client Protocol) client in `src/agent/client.rs`.
        *   Replaced stateless `opencode run` with persistent `opencode acp` connection.
        *   Session persistence now handled by ACP protocol.
    2.  **Telegram Reliability:** [x] COMPLETE (metadata-based routing implemented)
        *   Telegram chat ID is passed through metadata in `ChatMessage`.
        *   Agent replies preserve the metadata for proper routing back to Telegram users.
    3.  **Scheduler Implementation:** [ ] TODO
        *   Flesh out the `Scheduler` in `src/manager.rs` to handle periodic tasks (e.g., health checks).
    4.  **Logging & Error Handling:** [x] COMPLETE
        *   All critical paths use `tracing` for logging.
        *   Errors are logged via `tracing::error!`.
    5.  **Response Extraction:** [x] COMPLETE
        *   Implemented `extract_text_from_response()` helper in `bridge.rs`.
        *   Tries multiple JSON paths to extract agent responses.
        *   Falls back to debug logging if structure is unexpected.
