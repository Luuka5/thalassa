use anyhow::Result;
use mothership::runtime::Runtime;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::task;

use crate::agent::bridge::AgentSession;
use crate::bus::EventBus;
use crate::entity::{EntityId, Role};

pub struct Manager {
    runtime: Arc<Runtime>,
    event_bus: Arc<EventBus>,
    scheduler: Scheduler,
    sessions: Arc<Mutex<HashMap<String, Arc<AgentSession>>>>, // Changed from Mutex<AgentSession> to AgentSession since AgentSession is mostly read-only/uses internal locking or async
    // Wait, AgentSession has async methods. But it doesn't seem to have mutable state that needs external locking after initialization.
    // The `start()` method takes &self.
}

impl Manager {
    pub fn new(event_bus: Arc<EventBus>) -> Result<Self> {
        let runtime = Runtime::new()?;
        Ok(Self {
            runtime: Arc::new(runtime),
            scheduler: Scheduler::new(),
            event_bus,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn list_projects(&self) -> Result<Vec<String>> {
        let runtime = self.runtime.clone();
        task::spawn_blocking(move || {
            runtime.list_projects()
        })
        .await?
    }

    pub async fn launch_project(&self, name: String) -> Result<()> {
        let runtime = self.runtime.clone();
        let name_clone = name.clone();
        task::spawn_blocking(move || {
            runtime.launch(&name_clone)
        })
        .await??;

        self.start_agent_session(name).await?;

        Ok(())
    }
    
    pub async fn start_agent_session(&self, project_name: String) -> Result<()> {
        // Scope the lock so it is dropped before awaiting
        {
            let sessions = self.sessions.lock().unwrap();
            if sessions.contains_key(&project_name) {
                return Ok(());
            }
        }
        
        let agent_id = EntityId::new(
            format!("agent-{}", project_name),
            "Mothership Agent",
            Role::Agent,
        );

        let session = AgentSession::new(
            project_name.clone(),
            agent_id,
            self.event_bus.clone(),
            self.runtime.clone(),
        );

        session.start().await?;
        
        // Re-acquire lock to insert
        let mut sessions = self.sessions.lock().unwrap();
        sessions.insert(project_name, Arc::new(session));
        
        Ok(())
    }

    pub async fn exec_command(&self, name: String, cmd: String) -> Result<String> {
        let runtime = self.runtime.clone();
        task::spawn_blocking(move || {
            runtime.exec_capture(&name, &cmd)
        })
        .await?
    }

    pub async fn start_scheduler(&self) {
        self.scheduler.start().await;
    }
}

pub struct Scheduler {
    // Placeholder for scheduling logic
}

impl Scheduler {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn start(&self) {
        // Placeholder for scheduler loop
        std::future::pending::<()>().await;
    }
}
