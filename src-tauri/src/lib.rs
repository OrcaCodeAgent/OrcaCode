mod agent;
mod commands;
mod domain;
mod error;
mod events;
mod logging;
mod ollama;
mod permissions;
mod platform;
mod safety;
mod state;
mod storage;
mod tools;
mod util;

use state::AppState;
use storage::database::Database;
use tools::process::ProcessManager;

pub fn run() {
    let log_path = logging::init();
    logging::log_line("info", &format!("platform {}", platform::os_name()));
    let _ = log_path;
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            use tauri::Manager;
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let db = Database::open(&directory.join("orca.db"))?;
            let ollama = ollama::client::OllamaClient::new()?;
            app.manage(AppState {
                db: std::sync::Arc::new(db),
                supervisor: std::sync::Arc::new(state::Supervisor::new()),
                processes: std::sync::Arc::new(ProcessManager::new()),
                ollama,
                host: ollama::host::OllamaHost::new(),
                keep_alive: std::sync::atomic::AtomicBool::new(false),
            });
            let handle = app.handle().clone();
            if let Some(window) = app.get_webview_window("main") {
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let keep = handle
                            .try_state::<AppState>()
                            .map(|state| state.keep_alive.load(std::sync::atomic::Ordering::SeqCst))
                            .unwrap_or(false);
                        if keep {
                            if let Some(window) = handle.get_webview_window("main") {
                                let _ = window.hide();
                            }
                            return;
                        }
                        if let Some(state) = handle.try_state::<AppState>() {
                            state.host.shutdown();
                        }
                        let app = handle.clone();
                        std::thread::spawn(move || app.exit(0));
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::list_workspaces,
            commands::remember_workspace,
            commands::list_conversations,
            commands::get_conversation,
            commands::remove_conversation,
            commands::rename_conversation,
            commands::workspace_diff,
            commands::create_worktree,
            commands::open_external_url,
            commands::ollama_status,
            commands::ollama_models,
            commands::ollama_present,
            commands::pull_ollama_model,
            commands::unload_ollama_model,
            commands::install_ollama,
            commands::bootstrap_runtime,
            commands::ensure_ollama_server,
            commands::accessibility_status,
            commands::open_accessibility_settings,
            commands::list_processes,
            commands::read_process,
            commands::stop_process,
            commands::respond_permission,
            commands::undo_task,
            commands::cancel_task,
            commands::computer_home,
            commands::computer_desktop,
            commands::set_runs_in_background,
            commands::start_task,
        ])
        .build(tauri::generate_context!())
        .expect("Orca Code failed to start")
        .run(|app, event| {
            use tauri::Manager;
            if let tauri::RunEvent::Reopen { .. } = &event {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            if matches!(event, tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit) {
                if let Some(state) = app.try_state::<AppState>() {
                    state.host.shutdown();
                }
            }
        });
}
