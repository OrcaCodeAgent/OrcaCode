use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::agent::run::{run_task, TaskLaunch};
use crate::domain::{Mode, Settings};
use crate::error::{AppError, AppResult};
use crate::events::AgentEvent;
use crate::state::{emit, AppState, Decision};
use crate::storage::database::{ConversationDetail, ConversationSummary, WorkspaceRecord};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPayload {
    pub settings: Settings,
    pub default_prompt: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRequest {
    pub conversation_id: Option<String>,
    pub goal: String,
    pub mode: String,
    pub workspace_path: String,
    #[serde(default)]
    pub approval: String,
    #[serde(default)]
    pub effort: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default = "default_true")]
    pub record_user: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartResponse {
    pub conversation_id: String,
    pub task_id: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionAnswer {
    pub request_id: String,
    pub decision: String,
}

fn default_prompt() -> String {
    include_str!("../prompts/agent_system.md").to_string()
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppResult<SettingsPayload> {
    Ok(SettingsPayload {
        settings: state.db.load_settings()?,
        default_prompt: default_prompt(),
    })
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> AppResult<Settings> {
    let settings = settings.normalized();
    state.db.save_settings(&settings)?;
    Ok(settings)
}

#[tauri::command]
pub fn list_workspaces(state: State<'_, AppState>) -> AppResult<Vec<WorkspaceRecord>> {
    state.db.list_workspaces()
}

#[tauri::command]
pub fn remember_workspace(state: State<'_, AppState>, path: String) -> AppResult<WorkspaceRecord> {
    let path = path.trim().to_string();
    if path.is_empty() || !std::path::Path::new(&path).is_dir() {
        return Err(AppError::message("Could not find the working folder."));
    }
    let record = state.db.remember_workspace(&path)?;
    let mut settings = state.db.load_settings()?;
    settings.workspace_path = Some(path);
    state.db.save_settings(&settings)?;
    Ok(record)
}

#[tauri::command]
pub fn list_conversations(state: State<'_, AppState>) -> AppResult<Vec<ConversationSummary>> {
    state.db.list_conversations()
}

#[tauri::command]
pub fn get_conversation(state: State<'_, AppState>, id: String) -> AppResult<Option<ConversationDetail>> {
    state.db.get_conversation(&id)
}

#[tauri::command]
pub fn remove_conversation(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.delete_conversation(&id)
}

#[tauri::command]
pub fn rename_conversation(state: State<'_, AppState>, id: String, title: String) -> AppResult<()> {
    state.db.rename_conversation(&id, &title)
}

#[tauri::command]
pub fn workspace_diff(path: String) -> AppResult<String> {
    let path = path.trim();
    if !std::path::Path::new(path).is_dir() {
        return Err(AppError::message("Could not find the working folder."));
    }
    let output = std::process::Command::new("git")
        .args(["diff", "--no-ext-diff", "--"])
        .current_dir(path)
        .output()
        .map_err(|error| AppError::message(format!("Could not run git diff: {error}")))?;
    if !output.status.success() && output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        return Err(AppError::message(if detail.is_empty() {
            "This is not a Git repository, or the diff could not be created.".into()
        } else {
            detail.to_string()
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[tauri::command]
pub fn create_worktree(repo: String) -> AppResult<String> {
    let repo = repo.trim();
    let repo_path = std::path::Path::new(repo);
    if !repo_path.join(".git").exists() {
        return Err(AppError::message("A worktree can only be created inside a Git repository."));
    }
    let parent = repo_path
        .parent()
        .ok_or_else(|| AppError::message("There is no parent folder for the worktree."))?;
    let folder_name = repo_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repo");
    let id = crate::util::new_id();
    let short = &id[..8];
    let dest = parent.join(format!("{folder_name}-worktrees")).join(short);
    if let Some(parent_dir) = dest.parent() {
        std::fs::create_dir_all(parent_dir).map_err(|error| AppError::message(error.to_string()))?;
    }
    let branch = format!("orca/{short}");
    let output = std::process::Command::new("git")
        .args(["worktree", "add", "-b", &branch])
        .arg(&dest)
        .current_dir(repo_path)
        .output()
        .map_err(|error| AppError::message(format!("Could not create the git worktree: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::message(stderr.trim().to_string()));
    }
    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
pub fn open_external_url(url: String) -> AppResult<()> {
    let url = url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(AppError::message("Only http or https addresses can be opened."));
    }
    let status = std::process::Command::new("open")
        .arg(url)
        .status()
        .map_err(|error| AppError::message(error.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::message("Could not open the address."))
    }
}

#[tauri::command]
pub async fn ollama_status(state: State<'_, AppState>) -> Result<crate::ollama::client::OllamaStatus, AppError> {
    let settings = state.db.load_settings()?;
    Ok(state.ollama.status(&settings.ollama_url).await)
}

#[tauri::command]
pub async fn ollama_models(state: State<'_, AppState>) -> Result<Vec<String>, AppError> {
    let settings = state.db.load_settings()?;
    state.ollama.models(&settings.ollama_url).await
}

#[tauri::command]
pub async fn unload_ollama_model(state: State<'_, AppState>, name: String) -> AppResult<()> {
    let name = name.trim().to_string();
    if !valid_model_name(&name) {
        return Err(AppError::message("The model name is not valid."));
    }
    let settings = state.db.load_settings()?;
    state.ollama.unload(&settings.ollama_url, &name).await
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OllamaProgress {
    pub kind: String,
    pub status: String,
    pub completed: u64,
    pub total: u64,
    pub done: bool,
    pub error: Option<String>,
}

fn emit_progress(app: &AppHandle, progress: OllamaProgress) {
    if let Err(error) = app.emit("ollama-progress", &progress) {
        crate::logging::log_line("error", &format!("ollama progress: {error}"));
    }
}

fn valid_model_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '.' | '_' | '-' | '/'))
}

#[tauri::command]
pub fn ollama_present() -> bool {
    ollama_is_installed()
}

#[tauri::command]
pub async fn pull_ollama_model(app: AppHandle, state: State<'_, AppState>, name: String) -> AppResult<()> {
    let name = name.trim().to_string();
    if !valid_model_name(&name) {
        return Err(AppError::message("The model name is not valid."));
    }
    if PULLING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err(AppError::message("Another install is already running."));
    }
    let result: AppResult<()> = async {
        let settings = state.db.load_settings()?;
        emit_progress(
            &app,
            OllamaProgress {
                kind: "model".into(),
                status: format!("Downloading {name}"),
                completed: 0,
                total: 0,
                done: false,
                error: None,
            },
        );
        state
            .ollama
            .pull(&settings.ollama_url, &name, |status, completed, total| {
                emit_progress(
                    &app,
                    OllamaProgress {
                        kind: "model".into(),
                        status,
                        completed,
                        total,
                        done: false,
                        error: None,
                    },
                );
            })
            .await?;
        emit_progress(
            &app,
            OllamaProgress {
                kind: "model".into(),
                status: format!("Finished installing {name}."),
                completed: 1,
                total: 1,
                done: true,
                error: None,
            },
        );
        Ok(())
    }
    .await;
    PULLING.store(false, std::sync::atomic::Ordering::SeqCst);
    if let Err(error) = &result {
        emit_progress(
            &app,
            OllamaProgress {
                kind: "model".into(),
                status: error.to_string(),
                completed: 0,
                total: 0,
                done: true,
                error: Some(error.to_string()),
            },
        );
    }
    result
}

pub const DEFAULT_MODEL: &str = "qwen2.5-coder:7b";

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapStatus {
    pub online: bool,
    pub model: String,
    pub models: Vec<String>,
}

#[tauri::command]
pub async fn bootstrap_runtime(app: AppHandle, state: State<'_, AppState>) -> AppResult<BootstrapStatus> {
    if PULLING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err(AppError::message("Another install is already running."));
    }
    let result = bootstrap_inner(&app, &state).await;
    PULLING.store(false, std::sync::atomic::Ordering::SeqCst);
    if let Err(error) = &result {
        emit_progress(
            &app,
            OllamaProgress {
                kind: "app".into(),
                status: error.to_string(),
                completed: 0,
                total: 0,
                done: true,
                error: Some(error.to_string()),
            },
        );
    }
    result
}

async fn bootstrap_inner(app: &AppHandle, state: &AppState) -> AppResult<BootstrapStatus> {
    let settings = state.db.load_settings()?;
    let online = state.ollama.status(&settings.ollama_url).await.online;
    if !online {
        if ollama_is_installed() {
            emit_progress(
                app,
                OllamaProgress {
                    kind: "app".into(),
                    status: "Starting the Ollama server".into(),
                    completed: 0,
                    total: 0,
                    done: false,
                    error: None,
                },
            );
            launch_ollama(state)?;
        } else {
            install_ollama_inner(app, state).await?;
        }
        wait_until_online(state, 45).await?;
    }
    let url = state.db.load_settings()?.ollama_url;
    let mut models = state.ollama.models(&url).await.unwrap_or_default();
    if models.is_empty() {
        pull_named(app, state, DEFAULT_MODEL).await?;
        models = state.ollama.models(&url).await.unwrap_or_default();
    }
    if models.is_empty() {
        return Err(AppError::message("Could not download the default model qwen2.5-coder:7b."));
    }
    let mut settings = state.db.load_settings()?;
    let installed = models.iter().any(|name| name == &settings.model);
    if settings.model.trim().is_empty() || !installed {
        settings.model = models
            .iter()
            .find(|name| *name == DEFAULT_MODEL || name.starts_with("qwen2.5-coder"))
            .cloned()
            .unwrap_or_else(|| models[0].clone());
        state.db.save_settings(&settings)?;
    }
    emit_progress(
        app,
        OllamaProgress {
            kind: "app".into(),
            status: "Ready.".into(),
            completed: 1,
            total: 1,
            done: true,
            error: None,
        },
    );
    Ok(BootstrapStatus {
        online: true,
        model: settings.model,
        models,
    })
}

async fn pull_named(app: &AppHandle, state: &AppState, name: &str) -> AppResult<()> {
    let settings = state.db.load_settings()?;
    emit_progress(
        app,
        OllamaProgress {
            kind: "model".into(),
            status: format!("Downloading {name}"),
            completed: 0,
            total: 0,
            done: false,
            error: None,
        },
    );
    state
        .ollama
        .pull(&settings.ollama_url, name, |status, completed, total| {
            emit_progress(
                app,
                OllamaProgress {
                    kind: "model".into(),
                    status,
                    completed,
                    total,
                    done: false,
                    error: None,
                },
            );
        })
        .await?;
    emit_progress(
        app,
        OllamaProgress {
            kind: "model".into(),
            status: format!("Finished installing {name}."),
            completed: 1,
            total: 1,
            done: true,
            error: None,
        },
    );
    Ok(())
}

async fn wait_until_online(state: &AppState, seconds: u64) -> AppResult<()> {
    for _ in 0..seconds {
        let settings = state.db.load_settings()?;
        if state.ollama.status(&settings.ollama_url).await.online {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    Err(AppError::message("The Ollama server did not start. Try again in a moment."))
}

#[tauri::command]
pub async fn ensure_ollama_server(state: State<'_, AppState>) -> AppResult<()> {
    if PULLING.load(std::sync::atomic::Ordering::SeqCst) {
        return Ok(());
    }
    let settings = state.db.load_settings()?;
    if state.ollama.status(&settings.ollama_url).await.online {
        return Ok(());
    }
    if !ollama_is_installed() {
        return Err(AppError::message("Ollama is not installed. Reopen the app to continue setup."));
    }
    launch_ollama(&state)?;
    wait_until_online(&state, 25).await
}

#[tauri::command]
pub async fn install_ollama(app: AppHandle, state: State<'_, AppState>) -> AppResult<String> {
    if PULLING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err(AppError::message("Another install is already running."));
    }
    let result = install_ollama_inner(&app, &state).await;
    PULLING.store(false, std::sync::atomic::Ordering::SeqCst);
    if let Err(error) = &result {
        emit_progress(
            &app,
            OllamaProgress {
                kind: "app".into(),
                status: error.to_string(),
                completed: 0,
                total: 0,
                done: true,
                error: Some(error.to_string()),
            },
        );
    }
    result
}

fn ollama_is_installed() -> bool {
    app_paths().iter().any(|path| path.exists()) || which_ollama()
}

fn app_paths() -> Vec<std::path::PathBuf> {
    let mut paths = vec![std::path::PathBuf::from("/Applications/Ollama.app")];
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(std::path::PathBuf::from(home).join("Applications/Ollama.app"));
    }
    paths
}

fn which_ollama() -> bool {
    std::process::Command::new("which")
        .arg("ollama")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn launch_ollama(state: &AppState) -> AppResult<()> {
    if app_paths().iter().any(|path| path.exists()) {
        let status = std::process::Command::new("open")
            .arg("-a")
            .arg("Ollama")
            .status()
            .map_err(|error| AppError::message(error.to_string()))?;
        if status.success() {
            state.host.mark_launched_app();
            return Ok(());
        }
    }
    if which_ollama() {
        let child = std::process::Command::new("ollama")
            .arg("serve")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| AppError::message(error.to_string()))?;
        state.host.store_child(child);
        return Ok(());
    }
    Err(AppError::message("Could not start Ollama."))
}

async fn install_ollama_inner(app: &AppHandle, state: &AppState) -> AppResult<String> {
    if ollama_is_installed() {
        emit_progress(
            app,
            OllamaProgress {
                kind: "app".into(),
                status: "Starting Ollama".into(),
                completed: 0,
                total: 0,
                done: false,
                error: None,
            },
        );
        launch_ollama(state)?;
        emit_progress(
            app,
            OllamaProgress {
                kind: "app".into(),
                status: "Ollama is running.".into(),
                completed: 1,
                total: 1,
                done: true,
                error: None,
            },
        );
        return Ok("Ollama is running. You can install a model once the server is up.".into());
    }
    emit_progress(
        app,
        OllamaProgress {
            kind: "app".into(),
            status: "Downloading the Ollama installer".into(),
            completed: 0,
            total: 0,
            done: false,
            error: None,
        },
    );
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|error| AppError::message(error.to_string()))?;
    let response = client
        .get("https://ollama.com/download/Ollama-darwin.zip")
        .send()
        .await
        .map_err(|error| AppError::message(format!("Could not download Ollama: {error}")))?;
    if !response.status().is_success() {
        return Err(AppError::message("Could not download the Ollama installer."));
    }
    let total = response.content_length().unwrap_or(0);
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    let mut completed = 0u64;
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| AppError::message(error.to_string()))?;
        completed += chunk.len() as u64;
        bytes.extend_from_slice(&chunk);
        emit_progress(
            app,
            OllamaProgress {
                kind: "app".into(),
                status: "Downloading the Ollama installer".into(),
                completed,
                total,
                done: false,
                error: None,
            },
        );
    }
    let temp = std::env::temp_dir().join(format!("orca-ollama-{}.zip", crate::util::new_id()));
    let extract = std::env::temp_dir().join(format!("orca-ollama-{}", crate::util::new_id()));
    std::fs::write(&temp, &bytes).map_err(|error| AppError::message(error.to_string()))?;
    std::fs::create_dir_all(&extract).map_err(|error| AppError::message(error.to_string()))?;
    let unpacked = std::process::Command::new("ditto")
        .args(["-x", "-k"])
        .arg(&temp)
        .arg(&extract)
        .status()
        .map_err(|error| AppError::message(error.to_string()))?;
    if !unpacked.success() {
        return Err(AppError::message("Could not unpack the Ollama installer."));
    }
    let bundled = extract.join("Ollama.app");
    if !bundled.exists() {
        return Err(AppError::message("Could not find Ollama.app inside the installer."));
    }
    let destination = install_destination()?;
    if destination.exists() {
        std::fs::remove_dir_all(&destination).map_err(|error| AppError::message(error.to_string()))?;
    }
    if std::fs::rename(&bundled, &destination).is_err() {
        copy_dir(&bundled, &destination)?;
    }
    let _ = std::fs::remove_file(&temp);
    let _ = std::fs::remove_dir_all(&extract);
    launch_ollama(state)?;
    emit_progress(
        app,
        OllamaProgress {
            kind: "app".into(),
            status: "Finished installing Ollama.".into(),
            completed: 1,
            total: 1,
            done: true,
            error: None,
        },
    );
    Ok("Installed and started Ollama.".into())
}

fn install_destination() -> AppResult<std::path::PathBuf> {
    let system = std::path::PathBuf::from("/Applications/Ollama.app");
    if std::fs::metadata("/Applications")
        .map(|meta| !meta.permissions().readonly())
        .unwrap_or(false)
    {
        return Ok(system);
    }
    let home = std::env::var_os("HOME").ok_or_else(|| AppError::message("Could not find the home folder."))?;
    let apps = std::path::PathBuf::from(home).join("Applications");
    std::fs::create_dir_all(&apps).map_err(|error| AppError::message(error.to_string()))?;
    Ok(apps.join("Ollama.app"))
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> AppResult<()> {
    let status = std::process::Command::new("cp")
        .args(["-R"])
        .arg(from)
        .arg(to)
        .status()
        .map_err(|error| AppError::message(error.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::message("Could not copy Ollama.app."))
    }
}

static PULLING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
pub fn accessibility_status() -> bool {
    crate::platform::accessibility_trusted()
}

#[tauri::command]
pub fn open_accessibility_settings() -> AppResult<()> {
    crate::platform::open_accessibility_settings().map_err(AppError::message)
}

#[tauri::command]
pub fn list_processes(state: State<'_, AppState>) -> Vec<crate::tools::process::ProcessInfo> {
    state.processes.list()
}

#[tauri::command]
pub fn read_process(state: State<'_, AppState>, id: String) -> AppResult<String> {
    state
        .processes
        .output(&id, 20_000)
        .map_err(AppError::message)
}

#[tauri::command]
pub fn stop_process(state: State<'_, AppState>, id: String) -> AppResult<String> {
    state.processes.stop(&id).map_err(AppError::message)
}

#[tauri::command]
pub fn respond_permission(state: State<'_, AppState>, answer: PermissionAnswer) -> AppResult<()> {
    if state.supervisor.respond(&answer.request_id, Decision::parse(&answer.decision)) {
        Ok(())
    } else {
        Err(AppError::message("This approval request is already closed."))
    }
}

#[tauri::command]
pub fn undo_task(state: State<'_, AppState>, task_id: String) -> AppResult<UndoReport> {
    if state.supervisor.current_task().as_deref() == Some(task_id.as_str()) {
        return Err(AppError::message("A running task cannot be undone. Stop it first."));
    }
    let (restored, skipped) = state.db.undo_task(&task_id)?;
    Ok(UndoReport { restored, skipped })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoReport {
    pub restored: Vec<String>,
    pub skipped: Vec<String>,
}

#[tauri::command]
pub fn cancel_task(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    state.supervisor.cancel();
    let ids: Vec<String> = state.processes.list().into_iter().map(|process| process.id).collect();
    for id in ids {
        let _ = state.processes.stop(&id);
    }
    emit(
        &app,
        AgentEvent::State {
            state: "cancelled".into(),
            detail: None,
        },
    );
    Ok(())
}

#[tauri::command]
pub fn computer_home() -> AppResult<String> {
    dirs::home_dir()
        .map(|path| path.to_string_lossy().to_string())
        .ok_or_else(|| AppError::message("Could not find the home folder."))
}

#[tauri::command]
pub fn set_runs_in_background(state: State<'_, AppState>, enabled: bool) {
    state.keep_alive.store(enabled, std::sync::atomic::Ordering::SeqCst);
}

#[tauri::command]
pub fn computer_desktop() -> AppResult<String> {
    let home = dirs::home_dir().ok_or_else(|| AppError::message("Could not find the home folder."))?;
    let desktop = home.join("Desktop");
    if !desktop.is_dir() {
        std::fs::create_dir_all(&desktop)
            .map_err(|error| AppError::message(format!("Could not create the Desktop folder: {error}")))?;
    }
    Ok(desktop.to_string_lossy().to_string())
}

#[tauri::command]
pub fn start_task(app: AppHandle, state: State<'_, AppState>, request: StartRequest) -> AppResult<StartResponse> {
    let goal = request.goal.trim().to_string();
    if goal.is_empty() {
        return Err(AppError::message("Enter a request."));
    }
    let workspace = request.workspace_path.trim().to_string();
    if !std::path::Path::new(&workspace).is_dir() {
        return Err(AppError::message("Choose a workspace first."));
    }
    let mode = Mode::parse(&request.mode);
    let mut settings = state.db.load_settings()?;
    apply_run_overrides(&mut settings, &request);
    if settings.model.trim().is_empty() {
        return Err(AppError::message("Choose a model first."));
    }
    let workspace_record = state.db.remember_workspace(&workspace)?;
    let conversation_id = if let Some(id) = request.conversation_id.as_deref() {
        if state.db.get_conversation(id)?.is_none() {
            state.db.create_conversation(Some(&workspace_record.id), &goal, mode.as_str())?
        } else {
            state.db.touch_conversation(id, mode.as_str())?;
            id.to_string()
        }
    } else {
        state.db.create_conversation(Some(&workspace_record.id), &goal, mode.as_str())?
    };
    if request.record_user {
        let _ = state.db.insert_message(&conversation_id, "user", &goal);
    }
    let task_id = state.db.create_task(&conversation_id, &goal)?;
    let Some(cancel) = state.supervisor.try_start(task_id.clone()) else {
        return Err(AppError::message("A task is already running. Stop it and try again."));
    };
    let launch = TaskLaunch {
        app: app.clone(),
        db: Arc::clone(&state.db),
        ollama: state.ollama.clone(),
        processes: Arc::clone(&state.processes),
        supervisor: Arc::clone(&state.supervisor),
        cancel,
        conversation_id: conversation_id.clone(),
        task_id: task_id.clone(),
        workspace: std::path::PathBuf::from(workspace),
        goal,
        settings,
        mode,
        instructions: request.instructions.chars().take(24_000).collect(),
    };
    let supervisor = Arc::clone(&state.supervisor);
    let finishing_task = task_id.clone();
    tauri::async_runtime::spawn(async move {
        let _guard = FinishGuard {
            supervisor,
            task_id: finishing_task,
        };
        run_task(launch).await;
    });
    Ok(StartResponse {
        conversation_id,
        task_id,
    })
}

fn apply_run_overrides(settings: &mut Settings, request: &StartRequest) {
    match request.approval.as_str() {
        "default" => {
            settings.auto_approve_safe = true;
            settings.auto_approve_file_edits = false;
            settings.auto_approve_dangerous = false;
        }
        "auto" => {
            settings.auto_approve_safe = true;
            settings.auto_approve_file_edits = true;
            settings.auto_approve_dangerous = false;
        }
        "full" => {
            settings.auto_approve_safe = true;
            settings.auto_approve_file_edits = true;
            settings.auto_approve_dangerous = true;
        }
        _ => {}
    }
    match request.effort.as_str() {
        "low" => settings.temperature = 0.1,
        "medium" => settings.temperature = 0.2,
        "high" => settings.temperature = 0.45,
        "xhigh" => settings.temperature = 0.6,
        _ => {}
    }
}

struct FinishGuard {
    supervisor: Arc<crate::state::Supervisor>,
    task_id: String,
}

impl Drop for FinishGuard {
    fn drop(&mut self) {
        self.supervisor.finish(&self.task_id);
    }
}
