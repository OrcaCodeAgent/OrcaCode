use std::process::Command;

use serde_json::Value;

use crate::domain::ToolOutput;
use crate::safety::paths::{is_sensitive_path, relative_display, resolve_path};
use crate::safety::truncate::truncate_observation;
use crate::tools::context::ToolCtx;

pub fn read_document(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let Some(raw) = args.get("path").and_then(Value::as_str) else {
        return ToolOutput::fail("path is required.");
    };
    let resolved = match resolve_path(&ctx.workspace, raw) {
        Ok(path) => path,
        Err(_) => return ToolOutput::fail("The path is empty."),
    };
    if !resolved.path.is_file() {
        return ToolOutput::fail(format!("File does not exist: {}", resolved.path.display()));
    }
    if let Ok(canonical) = resolved.path.canonicalize() {
        if is_sensitive_path(&canonical) || is_sensitive_path(&resolved.path) {
            return ToolOutput::fail("Sensitive paths are not read.");
        }
    }
    let extension = resolved
        .path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let text = if extension == "pdf" {
        pdf_text(&resolved.path)
    } else {
        office_text(&resolved.path)
    };
    match text {
        Ok(text) if text.trim().is_empty() => ToolOutput::fail("Could not extract text from the document."),
        Ok(text) => ToolOutput::ok(format!(
            "document: {}\n{}",
            relative_display(&ctx.workspace, &resolved.path),
            truncate_observation(&text, 14_000)
        )),
        Err(error) => ToolOutput::fail(error),
    }
}

fn office_text(path: &std::path::Path) -> Result<String, String> {
    let output = Command::new("textutil")
        .args(["-convert", "txt", "-stdout"])
        .arg(path)
        .output()
        .map_err(|_| "Could not run textutil.".to_string())?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }
    Err(format!(
        "Could not read this format. {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

fn pdf_text(path: &std::path::Path) -> Result<String, String> {
    let path_arg = path.to_string_lossy().to_string();
    if let Some(text) = command_stdout("pdftotext", &["-layout", "-q", &path_arg, "-"]) {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }
    if let Some(text) = command_stdout("mdls", &["-raw", "-name", "kMDItemTextContent", &path_arg]) {
        let cleaned = text.trim();
        if !cleaned.is_empty() && cleaned != "(null)" {
            return Ok(cleaned.to_string());
        }
    }
    Err("Could not extract PDF text. pdftotext is missing and Spotlight has no body text.".into())
}

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}
