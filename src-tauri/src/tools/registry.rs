use serde_json::{json, Value};

use crate::domain::{Mode, RiskLevel, ToolOutput};
use crate::safety::redact::redact_secrets;
use crate::tools::context::ToolCtx;
use crate::tools::{document, fs, git, gui, plan, web};

#[derive(Clone, Debug)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
    pub inherent_risk: RiskLevel,
    pub read_only: bool,
}

#[derive(Clone)]
pub struct ToolRegistry {
    tools: Vec<ToolDef>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: definitions() }
    }

    pub fn get(&self, name: &str) -> Option<&ToolDef> {
        self.tools.iter().find(|tool| tool.name == name)
    }

    pub fn allowed(&self, name: &str, mode: Mode) -> bool {
        self.get(name)
            .map(|tool| mode.mutates() || tool.read_only)
            .unwrap_or(false)
    }

    pub fn schemas(&self, mode: Mode) -> Vec<Value> {
        self.tools
            .iter()
            .filter(|tool| mode.mutates() || tool.read_only)
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters
                    }
                })
            })
            .collect()
    }

    pub fn execute(&self, name: &str, args: Value, ctx: &ToolCtx) -> ToolOutput {
        let mut output = dispatch(name, &args, ctx);
        output.content = redact_secrets(&output.content);
        for change in &mut output.file_changes {
            change.diff = redact_secrets(&change.diff);
        }
        output
    }
}

fn dispatch(name: &str, args: &Value, ctx: &ToolCtx) -> ToolOutput {
    match name {
        "list_directory" => fs::list_directory(args, ctx),
        "read_file" => fs::read_file(args, ctx),
        "read_document" => document::read_document(args, ctx),
        "fetch_url" => web::fetch_url(args),
        "write_file" => fs::write_file(args, ctx),
        "edit_file" => fs::edit_file(args, ctx),
        "create_directory" => fs::create_directory(args, ctx),
        "move_file" => fs::move_file(args, ctx),
        "copy_file" => fs::copy_file(args, ctx),
        "delete_file" => fs::delete_file(args, ctx),
        "search_files" => fs::search_files(args, ctx),
        "search_text" => fs::search_text(args, ctx),
        "terminal_execute" => terminal(args, ctx),
        "process_start" => process_start(args, ctx),
        "process_list" => process_list(ctx),
        "process_stop" => process_stop(args, ctx),
        "process_read_output" => process_read(args, ctx),
        "git_status" => git::git_status(ctx),
        "git_diff" => git::git_diff(args, ctx),
        "git_log" => git::git_log(ctx),
        "git_branch" => git::git_branch(ctx),
        "git_add" => git::git_add(args, ctx),
        "git_commit" => git::git_commit(args, ctx),
        "git_checkout" => git::git_checkout(args, ctx),
        "git_create_branch" => git::git_create_branch(args, ctx),
        "update_plan" => plan::update_plan(args),
        "open_application" => gui::open_application(args),
        "open_url" => gui::open_url(args),
        "screenshot" => gui::screenshot(args, ctx),
        "keyboard_type" => gui::keyboard_type(args),
        "keyboard_shortcut" => gui::keyboard_shortcut(args),
        "mouse_click" => gui::mouse_click(args),
        "mouse_move" => gui::mouse_move(args),
        "scroll" => gui::scroll(args),
        other => ToolOutput::fail(format!("Unknown tool: {other}")),
    }
}

fn terminal(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let Some(command) = args.get("command").and_then(Value::as_str) else {
        return ToolOutput::fail("command is required.");
    };
    let cwd = match args.get("cwd").and_then(Value::as_str) {
        Some(raw) if !raw.trim().is_empty() => match crate::safety::paths::resolve_path(&ctx.workspace, raw) {
            Ok(path) => path.path,
            Err(_) => return ToolOutput::fail("cwd is empty."),
        },
        _ => ctx.workspace.clone(),
    };
    if !cwd.exists() {
        return ToolOutput::fail("The working directory does not exist.");
    }
    let timeout_ms = args.get("timeout").and_then(Value::as_u64).unwrap_or(ctx.timeout.as_millis() as u64);
    let timeout = std::time::Duration::from_millis(timeout_ms.clamp(1_000, 600_000));
    let output = crate::tools::terminal::run_command(command, &cwd, timeout, &ctx.cancel);
    let text = crate::safety::truncate::truncate_observation(
        &crate::tools::terminal::format_command_result(command, &cwd, &output),
        14_000,
    );
    if output.succeeded() {
        ToolOutput::ok(text)
    } else {
        ToolOutput::fail(text)
    }
}

fn process_start(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let Some(command) = args.get("command").and_then(Value::as_str) else {
        return ToolOutput::fail("command is required.");
    };
    let cwd = match args.get("cwd").and_then(Value::as_str) {
        Some(raw) if !raw.trim().is_empty() => match crate::safety::paths::resolve_path(&ctx.workspace, raw) {
            Ok(path) => path.path,
            Err(_) => return ToolOutput::fail("cwd is empty."),
        },
        _ => ctx.workspace.clone(),
    };
    match ctx.processes.start(command, cwd) {
        Ok(id) => ToolOutput::ok(format!("process_id: {id}")),
        Err(error) => ToolOutput::fail(error),
    }
}

fn process_list(ctx: &ToolCtx) -> ToolOutput {
    let list = ctx.processes.list();
    if list.is_empty() {
        return ToolOutput::ok("No managed process is running.");
    }
    let lines = list
        .iter()
        .map(|process| {
            format!(
                "{} {} {} {}",
                process.id,
                if process.running { "running" } else { "exited" },
                process.cwd,
                process.command
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    ToolOutput::ok(lines)
}

fn process_stop(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let Some(id) = args.get("process_id").and_then(Value::as_str) else {
        return ToolOutput::fail("process_id is required.");
    };
    match ctx.processes.stop(id) {
        Ok(message) => ToolOutput::ok(message),
        Err(error) => ToolOutput::fail(error),
    }
}

fn process_read(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let Some(id) = args.get("process_id").and_then(Value::as_str) else {
        return ToolOutput::fail("process_id is required.");
    };
    let max_chars = args.get("max_chars").and_then(Value::as_u64).unwrap_or(8_000) as usize;
    match ctx.processes.output(id, max_chars.clamp(200, 20_000)) {
        Ok(text) => ToolOutput::ok(crate::safety::truncate::truncate_observation(&text, 12_000)),
        Err(error) => ToolOutput::fail(error),
    }
}

fn definitions() -> Vec<ToolDef> {
    vec![
        def("list_directory", "List one directory level.", json!({"type":"object","properties":{"path":{"type":"string"},"limit":{"type":"integer"}},"required":[]}), RiskLevel::Safe, true),
        def("read_file", "Read a line range of a text file. Lines are 1-based. Defaults to 200 lines.", json!({"type":"object","properties":{"path":{"type":"string"},"start_line":{"type":"integer"},"end_line":{"type":"integer"}},"required":["path"]}), RiskLevel::Safe, true),
        def("read_document", "Extract text from a PDF, Word, Excel, PowerPoint, or other document on this computer.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}), RiskLevel::Safe, true),
        def("fetch_url", "Read the public text of an http(s) page. Does not use a signed-in browser.", json!({"type":"object","properties":{"url":{"type":"string"}},"required":["url"]}), RiskLevel::Safe, true),
        def("write_file", "Create a new file. Fails if the file already exists.", json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}), RiskLevel::Caution, false),
        def("edit_file", "Replace a unique old_string in an existing file, or apply a unified diff.", json!({"type":"object","properties":{"path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"},"replace_all":{"type":"boolean"},"diff":{"type":"string"}},"required":["path"]}), RiskLevel::Caution, false),
        def("create_directory", "Create a directory.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}), RiskLevel::Caution, false),
        def("move_file", "Move a file or directory inside the workspace.", json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"},"overwrite":{"type":"boolean"}},"required":["from","to"]}), RiskLevel::Caution, false),
        def("copy_file", "Copy a file or directory.", json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"},"overwrite":{"type":"boolean"}},"required":["from","to"]}), RiskLevel::Caution, false),
        def("delete_file", "Move a file or directory to the Trash. Directories require recursive true and approval.", json!({"type":"object","properties":{"path":{"type":"string"},"recursive":{"type":"boolean"}},"required":["path"]}), RiskLevel::Dangerous, false),
        def("search_files", "Find files by name.", json!({"type":"object","properties":{"query":{"type":"string"},"path":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]}), RiskLevel::Safe, true),
        def("search_text", "Search text inside the workspace. Skips dependencies and secret files.", json!({"type":"object","properties":{"query":{"type":"string"},"path":{"type":"string"},"case_sensitive":{"type":"boolean"},"limit":{"type":"integer"}},"required":["query"]}), RiskLevel::Safe, true),
        def("terminal_execute", "Run a command that is expected to exit. Returns stdout, stderr, and exit code.", json!({"type":"object","properties":{"command":{"type":"string"},"cwd":{"type":"string"},"timeout":{"type":"integer"}},"required":["command"]}), RiskLevel::Safe, false),
        def("process_start", "Start a long-running process and return process_id.", json!({"type":"object","properties":{"command":{"type":"string"},"cwd":{"type":"string"}},"required":["command"]}), RiskLevel::Caution, false),
        def("process_list", "List processes started by Orca Code.", json!({"type":"object","properties":{}}), RiskLevel::Safe, true),
        def("process_stop", "Stop an Orca Code-managed process.", json!({"type":"object","properties":{"process_id":{"type":"string"}},"required":["process_id"]}), RiskLevel::Caution, false),
        def("process_read_output", "Read captured output from an Orca Code-managed process.", json!({"type":"object","properties":{"process_id":{"type":"string"},"max_chars":{"type":"integer"}},"required":["process_id"]}), RiskLevel::Safe, true),
        def("git_status", "Show git status.", json!({"type":"object","properties":{}}), RiskLevel::Safe, true),
        def("git_diff", "Show git diff. Set staged true for the index.", json!({"type":"object","properties":{"path":{"type":"string"},"staged":{"type":"boolean"}}}), RiskLevel::Safe, true),
        def("git_log", "Show recent git commits.", json!({"type":"object","properties":{}}), RiskLevel::Safe, true),
        def("git_branch", "List git branches.", json!({"type":"object","properties":{}}), RiskLevel::Safe, true),
        def("git_add", "Stage explicit paths. Does not accept git add .", json!({"type":"object","properties":{"paths":{"type":"array","items":{"type":"string"}}},"required":["paths"]}), RiskLevel::Caution, false),
        def("git_commit", "Commit staged files. Does not skip hooks.", json!({"type":"object","properties":{"message":{"type":"string"}},"required":["message"]}), RiskLevel::Caution, false),
        def("git_checkout", "Check out an existing branch. Does not discard file changes.", json!({"type":"object","properties":{"branch":{"type":"string"}},"required":["branch"]}), RiskLevel::Caution, false),
        def("git_create_branch", "Create and check out a branch.", json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}), RiskLevel::Caution, false),
        def("update_plan", "Replace the visible task plan.", json!({"type":"object","properties":{"steps":{"type":"array"}},"required":["steps"]}), RiskLevel::Safe, true),
        def("open_application", "Open a macOS application by name.", json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}), RiskLevel::Caution, false),
        def("open_url", "Open an http(s) URL.", json!({"type":"object","properties":{"url":{"type":"string"}},"required":["url"]}), RiskLevel::Caution, false),
        def("screenshot", "Capture the screen into the workspace.", json!({"type":"object","properties":{}}), RiskLevel::Caution, false),
        def("keyboard_type", "Type text using accessibility APIs.", json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}), RiskLevel::Dangerous, false),
        def("keyboard_shortcut", "Press a shortcut. modifiers can include command, option, control, shift.", json!({"type":"object","properties":{"key":{"type":"string"},"modifiers":{"type":"array","items":{"type":"string"}}},"required":["key"]}), RiskLevel::Dangerous, false),
        def("mouse_click", "Click at screen coordinates. Prefer accessibility actions when possible.", json!({"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"},"button":{"type":"string"}},"required":["x","y"]}), RiskLevel::Dangerous, false),
        def("mouse_move", "Move the pointer.", json!({"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"]}), RiskLevel::Dangerous, false),
        def("scroll", "Scroll at a screen position. Negative dy scrolls down.", json!({"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"},"dy":{"type":"integer"}},"required":["x","y"]}), RiskLevel::Dangerous, false),
    ]
}

fn def(
    name: &'static str,
    description: &'static str,
    parameters: Value,
    inherent_risk: RiskLevel,
    read_only: bool,
) -> ToolDef {
    ToolDef {
        name,
        description,
        parameters,
        inherent_risk,
        read_only,
    }
}
