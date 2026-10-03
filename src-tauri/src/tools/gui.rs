use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::domain::ToolOutput;
use crate::platform;
use crate::tools::context::ToolCtx;
use crate::util::new_id;

pub fn open_application(args: &Value) -> ToolOutput {
    let Some(name) = args.get("name").and_then(Value::as_str) else {
        return ToolOutput::fail("name is required.");
    };
    if name.trim().is_empty() || name.contains('\0') {
        return ToolOutput::fail("The application name is not valid.");
    }
    match Command::new("open").arg("-a").arg(name).status() {
        Ok(status) if status.success() => ToolOutput::ok(format!("Opened {name}.")),
        Ok(_) => ToolOutput::fail("Could not open the application."),
        Err(error) => ToolOutput::fail(format!("Could not open the application: {error}")),
    }
}

pub fn open_url(args: &Value) -> ToolOutput {
    let Some(url) = args.get("url").and_then(Value::as_str) else {
        return ToolOutput::fail("url is required.");
    };
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return ToolOutput::fail("Only http or https addresses can be opened.");
    }
    match Command::new("open").arg(url).status() {
        Ok(status) if status.success() => ToolOutput::ok(format!("Opened: {url}")),
        _ => ToolOutput::fail("Could not open the address."),
    }
}

pub fn screenshot(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let directory = ctx.workspace.join(".orca").join("screenshots");
    if let Err(error) = fs::create_dir_all(&directory) {
        return ToolOutput::fail(format!("Could not create the screenshot folder: {error}"));
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let path = directory.join(format!("{stamp}-{}.jpg", &new_id()[..8]));
    let status = Command::new("screencapture")
        .arg("-x")
        .arg("-t")
        .arg("jpg")
        .arg(&path)
        .status();
    if !matches!(status, Ok(status) if status.success()) {
        return ToolOutput::fail("Could not take a screenshot.");
    }
    let _ = Command::new("sips").arg("-Z").arg("1280").arg(&path).status();
    let display = path.display().to_string();
    let mut output = ToolOutput::ok(if ctx.vision {
        format!("screenshot: {display}")
    } else {
        format!("screenshot: {display}\nThe current model cannot see images. Only the file path is passed on.")
    });
    if ctx.vision {
        if let Ok(bytes) = fs::read(&path) {
            if bytes.len() <= 4_000_000 {
                output.images.push(encode_base64(&bytes));
            }
        }
    }
    let _ = args;
    output
}

pub fn keyboard_type(args: &Value) -> ToolOutput {
    let Some(text) = args.get("text").and_then(Value::as_str) else {
        return ToolOutput::fail("text is required.");
    };
    match platform::keystroke(text) {
        Ok(()) => ToolOutput::ok("Sent keyboard input."),
        Err(error) => ToolOutput::fail(error),
    }
}

pub fn keyboard_shortcut(args: &Value) -> ToolOutput {
    let Some(key) = args.get("key").and_then(Value::as_str) else {
        return ToolOutput::fail("key is required.");
    };
    let modifiers: Vec<String> = args
        .get("modifiers")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    match platform::shortcut(key, &modifiers) {
        Ok(()) => ToolOutput::ok("Sent the shortcut."),
        Err(error) => ToolOutput::fail(error),
    }
}

pub fn mouse_click(args: &Value) -> ToolOutput {
    let Some((x, y)) = point(args) else {
        return ToolOutput::fail("x and y are required.");
    };
    let button = args.get("button").and_then(Value::as_str).unwrap_or("left");
    match platform::mouse_click(x, y, button) {
        Ok(()) => ToolOutput::ok(format!("Clicked: {x}, {y}")),
        Err(error) => ToolOutput::fail(error),
    }
}

pub fn mouse_move(args: &Value) -> ToolOutput {
    let Some((x, y)) = point(args) else {
        return ToolOutput::fail("x and y are required.");
    };
    match platform::mouse_move(x, y) {
        Ok(()) => ToolOutput::ok(format!("Moved: {x}, {y}")),
        Err(error) => ToolOutput::fail(error),
    }
}

pub fn scroll(args: &Value) -> ToolOutput {
    let Some((x, y)) = point(args) else {
        return ToolOutput::fail("x and y are required.");
    };
    let dy = args.get("dy").and_then(Value::as_i64).unwrap_or(-3) as i32;
    match platform::scroll(x, y, dy) {
        Ok(()) => ToolOutput::ok(format!("Scrolled: {dy}")),
        Err(error) => ToolOutput::fail(error),
    }
}

fn point(args: &Value) -> Option<(f64, f64)> {
    let x = args.get("x")?.as_f64().or_else(|| args.get("x")?.as_i64().map(|value| value as f64))?;
    let y = args.get("y")?.as_f64().or_else(|| args.get("y")?.as_i64().map(|value| value as f64))?;
    Some((x, y))
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (a << 16) | (b << 8) | c;
        out.push(TABLE[((triple >> 18) & 63) as usize] as char);
        out.push(TABLE[((triple >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
