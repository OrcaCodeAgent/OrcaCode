use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::thread;

use serde::Serialize;

use crate::tools::terminal::kill_pid;
use crate::util::new_id;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub id: String,
    pub command: String,
    pub cwd: String,
    pub running: bool,
    pub started_at: i64,
}

struct ManagedProcess {
    command: String,
    cwd: String,
    child: std::process::Child,
    output: Arc<Mutex<String>>,
    started_at: i64,
    running: bool,
}

pub struct ProcessManager {
    inner: Mutex<HashMap<String, ManagedProcess>>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn start(&self, command: &str, cwd: PathBuf) -> Result<String, String> {
        if command.trim().is_empty() {
            return Err("An empty command cannot run.".into());
        }
        let mut process = crate::tools::terminal::shell_command(command);
        process
            .current_dir(&cwd)
            .env("ORCA_AGENT", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            process.process_group(0);
        }
        let mut child = process.spawn().map_err(|error| {
            crate::logging::log_line("error", &format!("process spawn: {error}"));
            "Could not start the process.".to_string()
        })?;
        let id = format!("p{}", &new_id().replace('-', "")[..8]);
        let output = Arc::new(Mutex::new(String::new()));
        if let Some(stdout) = child.stdout.take() {
            spawn_reader(stdout, Arc::clone(&output), "stdout");
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_reader(stderr, Arc::clone(&output), "stderr");
        }
        let info = ManagedProcess {
            command: command.to_string(),
            cwd: cwd.display().to_string(),
            child,
            output,
            started_at: crate::util::now_ms(),
            running: true,
        };
        self.inner
            .lock()
            .map_err(|_| "Could not lock the process list.".to_string())?
            .insert(id.clone(), info);
        Ok(id)
    }

    pub fn list(&self) -> Vec<ProcessInfo> {
        let Ok(mut inner) = self.inner.lock() else {
            return Vec::new();
        };
        let mut infos = Vec::new();
        for (id, process) in inner.iter_mut() {
            if process.running {
                if let Ok(Some(_)) = process.child.try_wait() {
                    process.running = false;
                }
            }
            infos.push(ProcessInfo {
                id: id.clone(),
                command: process.command.clone(),
                cwd: process.cwd.clone(),
                running: process.running,
                started_at: process.started_at,
            });
        }
        infos
    }
    pub fn output(&self, id: &str, max_chars: usize) -> Result<String, String> {
        let inner = self.inner.lock().map_err(|_| "Could not lock the process list.".to_string())?;
        let process = inner.get(id).ok_or_else(|| "Could not find that process.".to_string())?;
        let text = process
            .output
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default();
        let count = text.chars().count();
        if count <= max_chars {
            Ok(text)
        } else {
            Ok(text.chars().skip(count - max_chars).collect())
        }
    }

    pub fn stop(&self, id: &str) -> Result<String, String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Could not lock the process list.".to_string())?;
        let process = inner
            .get_mut(id)
            .ok_or_else(|| "Could not find that process.".to_string())?;
        let pid = process.child.id();
        kill_pid(pid);
        let _ = process.child.wait();
        process.running = false;
        Ok(format!("Stopped process {id}."))
    }
}

impl Drop for ProcessManager {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.inner.lock() {
            for (_, mut process) in inner.drain() {
                kill_pid(process.child.id());
                let _ = process.child.kill();
                let _ = process.child.wait();
            }
        }
    }
}

fn spawn_reader(mut reader: impl Read + Send + 'static, output: Arc<Mutex<String>>, label: &str) {
    let label = label.to_string();
    thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(size) => {
                    let text = String::from_utf8_lossy(&buffer[..size]);
                    if let Ok(mut slot) = output.lock() {
                        slot.push_str(&format!("[{label}] {text}"));
                        if slot.len() > 160_000 {
                            let mut cut = slot.len() - 120_000;
                            while cut < slot.len() && !slot.is_char_boundary(cut) {
                                cut += 1;
                            }
                            slot.drain(..cut);
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });
}
