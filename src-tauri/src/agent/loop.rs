use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::AppHandle;

use crate::agent::briefing::build_briefing;
use crate::agent::cancel::CancelFlag;
use crate::agent::context::fit_messages;
use crate::agent::loop_detect::{LoopDetector, LoopSignal};
use crate::agent::parser::{expand_tool_requests, parse_model_turn, ParseOutcome, TurnAction};
use crate::agent::verify::run_checks;
use crate::domain::{ChatMessage, Mode, PlanStep, Settings, ToolOutput};
use crate::error::AppError;
use crate::events::AgentEvent;
use crate::ollama::client::OllamaClient;
use crate::permissions::manager::should_prompt;
use crate::permissions::risk::assess_tool;
use crate::state::{emit, Decision, Supervisor};
use crate::storage::database::Database;
use crate::tools::context::ToolCtx;
use crate::tools::process::ProcessManager;
use crate::tools::registry::ToolRegistry;
use crate::util::{canonical_json, new_id, stable_hash};

pub struct TaskLaunch {
    pub app: AppHandle,
    pub db: Arc<Database>,
    pub ollama: OllamaClient,
    pub processes: Arc<ProcessManager>,
    pub supervisor: Arc<Supervisor>,
    pub cancel: CancelFlag,
    pub conversation_id: String,
    pub task_id: String,
    pub workspace: PathBuf,
    pub goal: String,
    pub settings: Settings,
    pub mode: Mode,
    pub instructions: String,
}

struct StreamGate {
    buffer: String,
    emitted: bool,
    suppressed: bool,
}

impl StreamGate {
    fn push(&mut self, token: &str) -> Option<String> {
        if self.suppressed {
            return None;
        }
        if self.emitted {
            return Some(token.to_string());
        }
        self.buffer.push_str(token);
        let trimmed = self.buffer.trim_start();
        if trimmed.is_empty() {
            return None;
        }
        if trimmed.starts_with('{') || trimmed.starts_with("```") {
            self.suppressed = true;
            return None;
        }
        if trimmed.chars().count() >= 16 || trimmed.contains('\n') {
            self.emitted = true;
            return Some(std::mem::take(&mut self.buffer));
        }
        None
    }
}

pub async fn run_task(task: TaskLaunch) {
    let TaskLaunch {
        app,
        db,
        ollama,
        processes,
        supervisor,
        cancel,
        conversation_id,
        task_id,
        workspace,
        goal,
        settings,
        mode,
        instructions,
    } = task;
    if settings.model.trim().is_empty() {
        fail_task(&app, &db, &conversation_id, &task_id, &[], "Choose a model first.");
        return;
    }
    let registry = ToolRegistry::new();
    let schemas = registry.schemas(mode);
    let mut prompt = if settings.system_prompt.trim().is_empty() {
        include_str!("../../prompts/agent_system.md").to_string()
    } else {
        settings.system_prompt.clone()
    };
    prompt.push_str(mode_brief(mode));
    let extra = instructions.trim();
    if !extra.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(extra);
    }
    let briefing = tokio::task::spawn_blocking({
        let workspace = workspace.clone();
        let goal = goal.clone();
        let cancel = cancel.clone();
        move || build_briefing(&workspace, &goal, mode, &cancel)
    })
    .await
    .unwrap_or_else(|_| "Could not build the workspace summary.".into());

    let mut messages = vec![
        ChatMessage::system(prompt),
        ChatMessage::user(briefing),
    ];
    let mut detector = LoopDetector::new();
    let mut plan: Vec<PlanStep> = Vec::new();
    let mut mutated = false;
    let mut repairs = 0;
    let mut verify_failures = 0;
    let mut native_tools = true;
    let vision = model_supports_vision(&settings.model);
    let max_iterations = match mode {
        Mode::Plan => settings.max_iterations.min(6),
        Mode::Do => settings.max_iterations.min(12),
        _ => settings.max_iterations,
    };

    for iteration in 1..=max_iterations {
        if cancel.is_cancelled() {
            finish_cancelled(&app, &db, &conversation_id, &task_id, &plan);
            return;
        }
        emit(
            &app,
            AgentEvent::State {
                state: "thinking".into(),
                detail: Some(format!("{iteration}/{max_iterations}")),
            },
        );
        let fitted = fit_messages(&messages, (settings.context_length as usize).max(2000));
        let mut gate = StreamGate {
            buffer: String::new(),
            emitted: false,
            suppressed: false,
        };
        let app_for_tokens = app.clone();
        let chat = ollama
            .chat(
                &settings.ollama_url,
                &settings.model,
                &fitted,
                native_tools.then_some(&schemas),
                settings.temperature,
                settings.context_length,
                &cancel,
                |token| {
                    if let Some(text) = gate.push(token) {
                        emit(&app_for_tokens, AgentEvent::Token { text });
                    }
                },
            )
            .await;
        let output = match chat {
            Ok(output) => output,
            Err(AppError::Cancelled) => {
                finish_cancelled(&app, &db, &conversation_id, &task_id, &plan);
                return;
            }
            Err(error @ AppError::OllamaConnection) | Err(error @ AppError::Model(_)) => {
                fail_task(&app, &db, &conversation_id, &task_id, &plan, &error.to_string());
                return;
            }
            Err(error) if native_tools => {
                crate::logging::log_line("info", "retrying without native tools");
                native_tools = false;
                messages.push(ChatMessage::user(
                    "Native tool calling was rejected. Continue with the JSON protocol only.",
                ));
                let _ = error;
                continue;
            }
            Err(error) => {
                fail_task(&app, &db, &conversation_id, &task_id, &plan, &error.to_string());
                return;
            }
        };
        let parsed = parse_model_turn(&output.content, &output.tool_calls);
        let turn = match parsed {
            ParseOutcome::Turn(turn) => turn,
            ParseOutcome::InvalidProtocol { preview } => {
                repairs += 1;
                if repairs > 2 {
                    fail_task(
                        &app,
                        &db,
                        &conversation_id,
                        &task_id,
                        &plan,
                        "Could not read the model response as a tool call.",
                    );
                    return;
                }
                messages.push(ChatMessage::assistant(preview));
                messages.push(ChatMessage::user(
                    "The previous reply was not valid JSON. Reply with one JSON object: a tool_call, a plan, or a final answer.",
                ));
                continue;
            }
        };
        repairs = 0;
        emit(
            &app,
            AgentEvent::AssistantDone {
                content: turn.visible_text.clone(),
            },
        );
        if !turn.visible_text.trim().is_empty() {
            let _ = db.insert_message(&conversation_id, "assistant", &turn.visible_text);
        }
        messages.push(ChatMessage::assistant(if output.tool_calls.is_empty() {
            output.content.clone()
        } else {
            turn.visible_text.clone()
        }));
        if !output.tool_calls.is_empty() {
            if let Some(last) = messages.last_mut() {
                last.tool_calls = Some(
                    output
                        .tool_calls
                        .iter()
                        .map(|call| json!({"function": {"name": call.function.name, "arguments": call.function.arguments}}))
                        .collect(),
                );
            }
        }

        match turn.action {
            TurnAction::Final(content) => {
                if verifies(mode) && mutated && verify_failures < 2 {
                    emit(
                        &app,
                        AgentEvent::State {
                            state: "verifying".into(),
                            detail: None,
                        },
                    );
                    let workspace_for_check = workspace.clone();
                    let cancel_for_check = cancel.clone();
                    let timeout = Duration::from_millis(settings.terminal_timeout_ms.min(180_000));
                    let (ok, report) = tokio::task::spawn_blocking(move || {
                        run_checks(&workspace_for_check, timeout, &cancel_for_check)
                    })
                    .await
                    .unwrap_or((false, "Could not start verification.".into()));
                    let report = crate::safety::truncate::truncate_observation(&report, 12_000);
                    emit(
                        &app,
                        AgentEvent::ToolFinished {
                            id: new_id(),
                            success: ok,
                            content: report.clone(),
                            status: if ok { "ok" } else { "error" }.into(),
                        },
                    );
                    if ok {
                        succeed(&app, &db, &conversation_id, &task_id, &plan, &content);
                        return;
                    }
                    verify_failures += 1;
                    messages.push(ChatMessage::user(format!(
                        "Verification failed. Fix the project and continue. Do not repeat the same command.\n{report}"
                    )));
                    continue;
                }
                if verifies(mode) && mutated && verify_failures >= 2 {
                    fail_task(
                        &app,
                        &db,
                        &conversation_id,
                        &task_id,
                        &plan,
                        "Automatic verification kept failing. Check the last error and try again.",
                    );
                    return;
                }
                succeed(&app, &db, &conversation_id, &task_id, &plan, &content);
                return;
            }
            TurnAction::Tools(requests) => {
                for request in expand_tool_requests(requests) {
                    if cancel.is_cancelled() {
                        finish_cancelled(&app, &db, &conversation_id, &task_id, &plan);
                        return;
                    }
                    let outcome = execute_one(
                        &app,
                        &db,
                        &processes,
                        &supervisor,
                        &registry,
                        &cancel,
                        &settings,
                        mode,
                        &workspace,
                        &conversation_id,
                        &task_id,
                        vision,
                        &request.name,
                        request.arguments,
                        &mut detector,
                        &mut plan,
                        &mut mutated,
                    )
                    .await;
                    match outcome {
                        ToolFlow::Continue(message) => messages.push(message),
                        ToolFlow::Stop(message) => {
                            messages.push(message);
                            fail_task(
                                &app,
                                &db,
                                &conversation_id,
                                &task_id,
                                &plan,
                                "Stopped because the same tool call kept returning the same result.",
                            );
                            return;
                        }
                    }
                }
            }
        }
    }
    fail_task(
        &app,
        &db,
        &conversation_id,
        &task_id,
        &plan,
        "Reached the maximum number of steps. Split the job into smaller requests.",
    );
}

enum ToolFlow {
    Continue(ChatMessage),
    Stop(ChatMessage),
}

#[allow(clippy::too_many_arguments)]
async fn execute_one(
    app: &AppHandle,
    db: &Arc<Database>,
    processes: &Arc<ProcessManager>,
    supervisor: &Supervisor,
    registry: &ToolRegistry,
    cancel: &CancelFlag,
    settings: &Settings,
    mode: Mode,
    workspace: &PathBuf,
    conversation_id: &str,
    task_id: &str,
    vision: bool,
    name: &str,
    arguments: Value,
    detector: &mut LoopDetector,
    plan: &mut Vec<PlanStep>,
    mutated: &mut bool,
) -> ToolFlow {
    let Some(def) = registry.get(name).cloned() else {
        let message = format!("Unknown tool: {name}");
        return ToolFlow::Continue(tool_message(name, &message, false));
    };
    if !registry.allowed(name, mode) {
        let message = if mode == Mode::Plan {
            "Do not change files before approval. Describe the plan only.".to_string()
        } else {
            "Ask mode does not change files or run commands.".to_string()
        };
        return ToolFlow::Continue(tool_message(name, &message, false));
    }
    let report = assess_tool(name, &arguments, workspace, def.inherent_risk);
    if let Some(reason) = report.hard_block {
        return ToolFlow::Continue(tool_message(name, &reason, false));
    }
    if should_prompt(report.level, report.force_prompt, settings, &supervisor.approvals, &report.fingerprint) {
        emit(
            app,
            AgentEvent::State {
                state: "waiting_permission".into(),
                detail: Some(name.to_string()),
            },
        );
        let decision = supervisor
            .ask(
                app,
                crate::events::PermissionPrompt {
                    id: new_id(),
                    tool: name.to_string(),
                    arguments: arguments.clone(),
                    risk: report.level.as_str().to_string(),
                    summary: report.summary.clone(),
                },
            )
            .await;
        let _ = db.record_permission(
            conversation_id,
            name,
            &canonical_json(&arguments),
            report.level.as_str(),
            decision.as_str(),
        );
        if cancel.is_cancelled() || decision == Decision::Deny {
            return ToolFlow::Continue(tool_message(name, "The user denied this action.", false));
        }
        if decision == Decision::AllowSession {
            supervisor.approvals.allow(report.fingerprint);
        }
    }
    let call_id = new_id();
    let _ = db.insert_tool_call(
        &call_id,
        conversation_id,
        name,
        &canonical_json(&arguments),
        report.level.as_str(),
        "running",
    );
    emit(
        app,
        AgentEvent::ToolStarted {
            id: call_id.clone(),
            name: name.to_string(),
            arguments: arguments.clone(),
            risk: report.level.as_str().to_string(),
        },
    );
    emit(
        app,
        AgentEvent::State {
            state: "executing_tool".into(),
            detail: Some(name.to_string()),
        },
    );
    let ctx = ToolCtx {
        workspace: workspace.clone(),
        task_id: task_id.to_string(),
        cancel: cancel.clone(),
        timeout: Duration::from_millis(settings.terminal_timeout_ms),
        db: Arc::clone(db),
        processes: Arc::clone(processes),
        vision,
    };
    let registry = registry.clone();
    let tool_name = name.to_string();
    let tool_args = arguments.clone();
    let output = tokio::task::spawn_blocking(move || registry.execute(&tool_name, tool_args, &ctx))
        .await
        .unwrap_or_else(|_| ToolOutput::fail("The tool run was interrupted."));
    if output.mutated {
        *mutated = true;
    }
    if let Some(next_plan) = output.plan.clone() {
        *plan = next_plan.clone();
        let _ = db.update_task(task_id, "running", plan);
        emit(app, AgentEvent::Plan { steps: next_plan });
    }
    for change in &output.file_changes {
        emit(app, AgentEvent::FileChange { change: change.clone() });
    }
    let status = if output.success { "ok" } else { "error" };
    let _ = db.finish_tool_call(&call_id, status, output.success, &output.content);
    emit(
        app,
        AgentEvent::ToolFinished {
            id: call_id,
            success: output.success,
            content: output.content.clone(),
            status: status.into(),
        },
    );
    let signature = format!("{}:{}", output.success, stable_hash(&output.content));
    let key = format!("{name}\n{}", canonical_json(&arguments));
    let signal = detector.observe(&key, &signature);
    let mut note = String::new();
    let stop = match signal {
        LoopSignal::RepeatWarning => {
            note = "The same action is repeating. Try a different approach.".into();
            false
        }
        LoopSignal::Stop => {
            note = "Stopped because the same tool call kept returning the same result.".into();
            true
        }
        LoopSignal::Ok => false,
    };
    let content = if output.images.is_empty() {
        output.content.clone()
    } else {
        format!("{}\n[screenshot attached for a vision model]", output.content)
    };
    let mut message = tool_message(name, &content, output.success);
    if vision && !output.images.is_empty() {
        message.role = "user".into();
        message.images = Some(output.images);
    }
    if !note.is_empty() {
        message.content = format!("{}\n{note}", message.content);
        emit(app, AgentEvent::Error { message: note.clone() });
    }
    if stop {
        ToolFlow::Stop(message)
    } else {
        ToolFlow::Continue(message)
    }
}

fn tool_message(name: &str, content: &str, success: bool) -> ChatMessage {
    ChatMessage {
        role: "tool".into(),
        content: json!({ "success": success, "content": content }).to_string(),
        images: None,
        tool_calls: None,
        tool_name: Some(name.to_string()),
    }
}

fn mode_brief(mode: Mode) -> &'static str {
    match mode {
        Mode::Plan => "\n\nPreview only. Look with read-only tools, then stop. Finish with one short paragraph in the user's language: what you will do, what you will not delete, and the order. Do not edit, move, delete, install, or commit.",
        Mode::Ask => "\n\nAsk mode. Explain only. Do not change files, run mutating commands, or commit.",
        Mode::Do => "\n\nDo mode. The user already approved the plan. Finish that one job and stop. Do not widen the task. Call update_plan with the steps you are taking.",
        Mode::Mission | Mode::Agent => "\n\nMission mode. The user already approved the plan. Carry every step through. Call update_plan first, then verify the result before you finish.",
    }
}

fn verifies(mode: Mode) -> bool {
    matches!(mode, Mode::Mission | Mode::Agent)
}

fn model_supports_vision(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    ["llava", "vision", "-vl", "moondream", "minicpm-v", "bakllava", "qwen2.5vl", "gemma3"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn succeed(app: &AppHandle, db: &Database, _conversation_id: &str, task_id: &str, plan: &[PlanStep], _content: &str) {
    let _ = db.update_task(task_id, "completed", plan);
    emit(
        app,
        AgentEvent::State {
            state: "completed".into(),
            detail: None,
        },
    );
}

fn fail_task(app: &AppHandle, db: &Database, conversation_id: &str, task_id: &str, plan: &[PlanStep], message: &str) {
    let _ = db.insert_message(conversation_id, "assistant", message);
    let _ = db.update_task(task_id, "failed", plan);
    emit(app, AgentEvent::Error { message: message.to_string() });
    emit(
        app,
        AgentEvent::State {
            state: "failed".into(),
            detail: Some(message.to_string()),
        },
    );
}

fn finish_cancelled(app: &AppHandle, db: &Database, conversation_id: &str, task_id: &str, plan: &[PlanStep]) {
    let _ = db.insert_message(conversation_id, "assistant", "The task was cancelled.");
    let _ = db.update_task(task_id, "cancelled", plan);
    emit(
        app,
        AgentEvent::State {
            state: "cancelled".into(),
            detail: None,
        },
    );
}
