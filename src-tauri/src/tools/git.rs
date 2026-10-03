use std::time::Duration;

use serde_json::Value;

use crate::domain::ToolOutput;
use crate::safety::paths::resolve_path;
use crate::safety::truncate::truncate_observation;
use crate::tools::context::ToolCtx;
use crate::tools::terminal::{format_command_result, run_git};

pub fn git_status(ctx: &ToolCtx) -> ToolOutput {
    git(ctx, vec!["status".into(), "--short".into(), "--branch".into()])
}

pub fn git_diff(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let mut command = vec!["diff".into()];
    if args.get("staged").and_then(Value::as_bool).unwrap_or(false) {
        command.push("--cached".into());
    }
    if let Some(path) = args.get("path").and_then(Value::as_str) {
        if !path.trim().is_empty() {
            command.push("--".into());
            command.push(path.to_string());
        }
    }
    git(ctx, command)
}

pub fn git_log(ctx: &ToolCtx) -> ToolOutput {
    git(
        ctx,
        vec![
            "log".into(),
            "-n".into(),
            "30".into(),
            "--oneline".into(),
            "--decorate".into(),
        ],
    )
}

pub fn git_branch(ctx: &ToolCtx) -> ToolOutput {
    git(ctx, vec!["branch".into(), "-vv".into()])
}

pub fn git_add(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let Some(paths) = args.get("paths").and_then(Value::as_array) else {
        return ToolOutput::fail("A paths array is required. git add . is not allowed.");
    };
    if paths.is_empty() {
        return ToolOutput::fail("There is no path to add.");
    }
    let mut command = vec!["add".into(), "--".into()];
    for path in paths {
        let Some(raw) = path.as_str() else {
            return ToolOutput::fail("Each paths entry must be a string.");
        };
        if raw.trim().is_empty() || raw == "." || raw == "-A" || raw == "--all" {
            return ToolOutput::fail("Adding everything is not allowed. Name the paths.");
        }
        let resolved = match resolve_path(&ctx.workspace, raw) {
            Ok(path) => path,
            Err(_) => return ToolOutput::fail("An empty path cannot be added."),
        };
        if !resolved.inside_workspace {
            return ToolOutput::fail("Files outside the workspace cannot be git added.");
        }
        if crate::safety::paths::is_sensitive_path(&resolved.path) {
            return ToolOutput::fail(format!("Sensitive files are not added automatically: {raw}"));
        }
        command.push(raw.to_string());
    }
    git(ctx, command)
}

pub fn git_commit(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let Some(message) = args.get("message").and_then(Value::as_str) else {
        return ToolOutput::fail("message is required.");
    };
    let message = message.trim();
    if message.is_empty() {
        return ToolOutput::fail("The commit message is empty.");
    }
    if message.len() > 8_000 {
        return ToolOutput::fail("The commit message is too long.");
    }
    let file = ctx.workspace.join(".orca").join("commit-message.txt");
    if let Some(parent) = file.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            return ToolOutput::fail(format!("Could not create the commit message folder: {error}"));
        }
    }
    if let Err(error) = std::fs::write(&file, message) {
        return ToolOutput::fail(format!("Could not save the commit message: {error}"));
    }
    let output = git(ctx, vec!["commit".into(), "-F".into(), file.display().to_string()]);
    let _ = std::fs::remove_file(&file);
    output
}

pub fn git_checkout(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let Some(branch) = args.get("branch").and_then(Value::as_str) else {
        return ToolOutput::fail("branch is required.");
    };
    if !valid_branch(branch) {
        return ToolOutput::fail("The branch name is not valid.");
    }
    git(ctx, vec!["checkout".into(), branch.to_string()])
}

pub fn git_create_branch(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let Some(name) = args.get("name").and_then(Value::as_str) else {
        return ToolOutput::fail("name is required.");
    };
    if !valid_branch(name) {
        return ToolOutput::fail("The branch name is not valid.");
    }
    git(ctx, vec!["checkout".into(), "-b".into(), name.to_string()])
}

fn git(ctx: &ToolCtx, args: Vec<String>) -> ToolOutput {
    let probe = run_git(
        &ctx.workspace,
        &["rev-parse".into(), "--is-inside-work-tree".into()],
        Duration::from_secs(10),
        &ctx.cancel,
    );
    if !probe.succeeded() || !probe.stdout.to_ascii_lowercase().contains("true") {
        return ToolOutput::fail("This folder is not a Git repository.");
    }
    let output = run_git(&ctx.workspace, &args, Duration::from_secs(30), &ctx.cancel);
    let mut text = format_command_result(&format!("git {}", args.join(" ")), &ctx.workspace, &output);
    text = truncate_observation(&text, 14_000);
    if output.succeeded() {
        ToolOutput::ok(text)
    } else {
        ToolOutput::fail(text)
    }
}

fn valid_branch(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first == '-' || name.contains("..") || name.ends_with('/') || name.ends_with(".lock") {
        return false;
    }
    name.chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '/' | '-'))
}
