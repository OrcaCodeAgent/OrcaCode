use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use crate::agent::cancel::CancelFlag;
use crate::events::{AgentEvent, PermissionPrompt};
use crate::ollama::client::OllamaClient;
use crate::permissions::manager::ApprovalCache;
use crate::storage::database::Database;
use crate::tools::process::ProcessManager;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    AllowSession,
    AllowOnce,
    Deny,
}

impl Decision {
    pub fn parse(value: &str) -> Self {
        match value {
            "allow" | "session" => Self::AllowSession,
            "once" => Self::AllowOnce,
            _ => Self::Deny,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::AllowSession => "allow",
            Self::AllowOnce => "once",
            Self::Deny => "deny",
        }
    }
}

struct Running {
    cancel: CancelFlag,
    task_id: String,
}

pub struct Supervisor {
    running: Mutex<Option<Running>>,
    pending: Mutex<HashMap<String, oneshot::Sender<Decision>>>,
    pub approvals: std::sync::Arc<ApprovalCache>,
}

impl Supervisor {
    pub fn new() -> Self {
        Self {
            running: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            approvals: std::sync::Arc::new(ApprovalCache::default()),
        }
    }

    pub fn try_start(&self, task_id: String) -> Option<CancelFlag> {
        let mut running = self.running.lock().unwrap_or_else(|error| error.into_inner());
        if running.is_some() {
            return None;
        }
        let cancel = CancelFlag::new();
        *running = Some(Running {
            cancel: cancel.clone(),
            task_id,
        });
        Some(cancel)
    }

    pub fn finish(&self, task_id: &str) {
        let mut running = self.running.lock().unwrap_or_else(|error| error.into_inner());
        if running.as_ref().map(|item| item.task_id.as_str()) == Some(task_id) {
            *running = None;
        }
    }

    pub fn current_task(&self) -> Option<String> {
        self.running
            .lock()
            .ok()
            .and_then(|running| running.as_ref().map(|item| item.task_id.clone()))
    }

    pub fn cancel(&self) {
        if let Ok(running) = self.running.lock() {
            if let Some(item) = running.as_ref() {
                item.cancel.cancel();
            }
        }
        self.close_pending();
    }

    pub async fn ask(&self, app: &AppHandle, prompt: PermissionPrompt) -> Decision {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(prompt.id.clone(), tx);
        }
        let _ = app.emit("agent-event", &AgentEvent::Permission { request: prompt });
        rx.await.unwrap_or(Decision::Deny)
    }

    pub fn respond(&self, id: &str, decision: Decision) -> bool {
        let sender = self.pending.lock().ok().and_then(|mut pending| pending.remove(id));
        if let Some(sender) = sender {
            sender.send(decision).is_ok()
        } else {
            false
        }
    }

    fn close_pending(&self) {
        if let Ok(mut pending) = self.pending.lock() {
            for (_, sender) in pending.drain() {
                let _ = sender.send(Decision::Deny);
            }
        }
    }
}

pub struct AppState {
    pub db: std::sync::Arc<Database>,
    pub supervisor: std::sync::Arc<Supervisor>,
    pub processes: std::sync::Arc<ProcessManager>,
    pub ollama: OllamaClient,
    pub host: crate::ollama::host::OllamaHost,
    pub keep_alive: std::sync::atomic::AtomicBool,
}

pub fn emit(app: &AppHandle, event: AgentEvent) {
    if let Err(error) = app.emit("agent-event", &event) {
        crate::logging::log_line("error", &format!("event emit failed: {error}"));
    }
}
