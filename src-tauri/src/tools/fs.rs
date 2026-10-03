use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::Value;
use walkdir::WalkDir;

use crate::domain::{FileChange, ToolOutput};
use crate::safety::paths::{is_critical_path, relative_display, resolve_path, should_skip_entry};
use crate::safety::truncate::truncate_observation;
use crate::tools::context::ToolCtx;
use crate::tools::edit::{apply_replacement, apply_unified_diff, unified_diff, EditError};

pub fn list_directory(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let raw = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let resolved = match resolve_path(&ctx.workspace, raw) {
        Ok(path) => path,
        Err(_) => return ToolOutput::fail("The path is empty."),
    };
    if !resolved.path.exists() {
        return ToolOutput::fail(format!("Path does not exist: {}", resolved.path.display()));
    }
    if !resolved.path.is_dir() {
        return ToolOutput::fail("Not a directory.");
    }
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(200).clamp(1, 500) as usize;
    let entries = match fs::read_dir(&resolved.path) {
        Ok(entries) => entries,
        Err(error) => return ToolOutput::fail(format!("Could not read the directory: {error}")),
    };
    let mut lines = Vec::new();
    let mut count = 0;
    let mut files = 0;
    let mut dirs = 0;
    let mut items: Vec<_> = entries.flatten().collect();
    items.sort_by_key(|entry| entry.file_name());
    let total = items.len();
    for entry in items {
        if count >= limit {
            lines.push(format!("... showing {limit} of {total}."));
            break;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = entry.path().is_dir();
        if is_dir {
            dirs += 1;
        } else {
            files += 1;
        }
        let kind = if is_dir { "dir" } else { "file" };
        let meta = entry.metadata().ok();
        let size = meta.as_ref().map(|meta| meta.len()).unwrap_or(0);
        let modified = meta
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_default();
        lines.push(format!("{kind}\t{size}\t{modified}\t{name}"));
        count += 1;
    }
    ToolOutput::ok(format!(
        "directory: {}\nsummary: files={files} dirs={dirs}\n{}",
        relative_display(&ctx.workspace, &resolved.path),
        lines.join("\n")
    ))
}

pub fn read_file(args: &Value, ctx: &ToolCtx) -> ToolOutput {
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
        if crate::safety::paths::is_sensitive_path(&canonical) {
            return ToolOutput::fail("Sensitive paths are not read.");
        }
    }
    let metadata = match fs::metadata(&resolved.path) {
        Ok(metadata) => metadata,
        Err(error) => return ToolOutput::fail(format!("Could not inspect the file: {error}")),
    };
    if metadata.len() > 2_000_000 {
        return ToolOutput::fail("Files larger than 2MB must be read with start_line and end_line.");
    }
    let text = match fs::read(&resolved.path) {
        Ok(bytes) => {
            if bytes.contains(&0) {
                return ToolOutput::fail("Binary files are not read.");
            }
            String::from_utf8_lossy(&bytes).to_string()
        }
        Err(error) => return ToolOutput::fail(format!("Could not read the file: {error}")),
    };
    let lines: Vec<&str> = text.split('\n').collect();
    let total = lines.len();
    let start = args.get("start_line").and_then(Value::as_u64).unwrap_or(1).max(1) as usize;
    let mut end = args
        .get("end_line")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(start + 199);
    if end < start {
        end = start;
    }
    if end - start > 399 {
        end = start + 399;
    }
    let start_index = start.saturating_sub(1).min(total);
    let end_index = end.min(total);
    let mut body = String::new();
    for (offset, line) in lines[start_index..end_index].iter().enumerate() {
        body.push_str(&format!("{:>6}|{}\n", start_index + offset + 1, line));
    }
    ToolOutput::ok(format!(
        "file: {}\nlines: {}-{} of {total}\n{body}",
        relative_display(&ctx.workspace, &resolved.path),
        start_index + 1,
        end_index
    ))
}

pub fn write_file(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let Some(raw) = args.get("path").and_then(Value::as_str) else {
        return ToolOutput::fail("path is required.");
    };
    let Some(content) = args.get("content").and_then(Value::as_str) else {
        return ToolOutput::fail("content is required.");
    };
    let resolved = match resolve_path(&ctx.workspace, raw) {
        Ok(path) => path,
        Err(_) => return ToolOutput::fail("The path is empty."),
    };
    if is_critical_path(&resolved.path) || resolved.path == ctx.workspace {
        return ToolOutput::fail("This path cannot be created.");
    }
    if resolved.path.exists() {
        return ToolOutput::fail("The file already exists. Use edit_file instead of write_file.");
    }
    if let Err(error) = capture(ctx, &resolved.path) {
        return error;
    }
    if let Some(parent) = resolved.path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return ToolOutput::fail(format!("Could not create the parent folder: {error}"));
        }
    }
    if let Err(error) = fs::write(&resolved.path, content) {
        return ToolOutput::fail(format!("Could not write the file: {error}"));
    }
    let diff = unified_diff("", content);
    let change = record_change(ctx, &resolved.path, "added", &diff);
    let mut output = ToolOutput::ok(format!("created {}", relative_display(&ctx.workspace, &resolved.path)));
    output.file_changes = vec![change];
    output.mutated = true;
    output
}

pub fn edit_file(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let Some(raw) = args.get("path").and_then(Value::as_str) else {
        return ToolOutput::fail("path is required.");
    };
    let resolved = match resolve_path(&ctx.workspace, raw) {
        Ok(path) => path,
        Err(_) => return ToolOutput::fail("The path is empty."),
    };
    if !resolved.path.is_file() {
        return ToolOutput::fail("The file to edit does not exist. Use write_file for a new file.");
    }
    let original = match fs::read_to_string(&resolved.path) {
        Ok(text) => text,
        Err(error) => return ToolOutput::fail(format!("Could not read the file: {error}")),
    };
    let updated = if let Some(diff) = args.get("diff").and_then(Value::as_str) {
        match apply_unified_diff(&original, diff) {
            Ok(text) => text,
            Err(error) => {
                return ToolOutput::fail(format!(
                    "Could not apply the patch: {}. Try again with old_string and new_string.",
                    edit_message(&error)
                ))
            }
        }
    } else {
        let Some(old) = args.get("old_string").and_then(Value::as_str) else {
            return ToolOutput::fail("old_string or diff is required.");
        };
        let Some(new) = args.get("new_string").and_then(Value::as_str) else {
            return ToolOutput::fail("new_string is required.");
        };
        let replace_all = args.get("replace_all").and_then(Value::as_bool).unwrap_or(false);
        match apply_replacement(&original, old, new, replace_all) {
            Ok(text) => text,
            Err(error) => return ToolOutput::fail(edit_message(&error)),
        }
    };
    if updated == original {
        return ToolOutput::ok("Nothing changed.");
    }
    if let Err(error) = capture(ctx, &resolved.path) {
        return error;
    }
    if let Err(error) = fs::write(&resolved.path, &updated) {
        return ToolOutput::fail(format!("Could not write the file: {error}"));
    }
    let diff = unified_diff(&original, &updated);
    let change = record_change(ctx, &resolved.path, "modified", &diff);
    let mut output = ToolOutput::ok(format!("updated {}", relative_display(&ctx.workspace, &resolved.path)));
    output.file_changes = vec![change];
    output.mutated = true;
    output
}

pub fn create_directory(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let Some(raw) = args.get("path").and_then(Value::as_str) else {
        return ToolOutput::fail("path is required.");
    };
    let resolved = match resolve_path(&ctx.workspace, raw) {
        Ok(path) => path,
        Err(_) => return ToolOutput::fail("The path is empty."),
    };
    if resolved.path == ctx.workspace || is_critical_path(&resolved.path) {
        return ToolOutput::fail("This path cannot be created.");
    }
    if let Err(error) = capture(ctx, &resolved.path) {
        return error;
    }
    if let Err(error) = fs::create_dir_all(&resolved.path) {
        return ToolOutput::fail(format!("Could not create the directory: {error}"));
    }
    let change = record_change(
        ctx,
        &resolved.path,
        "added",
        &format!("created directory {}", relative_display(&ctx.workspace, &resolved.path)),
    );
    let mut output = ToolOutput::ok(format!("created {}", relative_display(&ctx.workspace, &resolved.path)));
    output.file_changes = vec![change];
    output.mutated = true;
    output
}

pub fn move_file(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    transfer(args, ctx, true)
}

pub fn copy_file(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    transfer(args, ctx, false)
}

fn transfer(args: &Value, ctx: &ToolCtx, rename: bool) -> ToolOutput {
    let Some(from_raw) = args.get("from").and_then(Value::as_str) else {
        return ToolOutput::fail("from is required.");
    };
    let Some(to_raw) = args.get("to").and_then(Value::as_str) else {
        return ToolOutput::fail("to is required.");
    };
    let from = match resolve_path(&ctx.workspace, from_raw) {
        Ok(path) => path,
        Err(_) => return ToolOutput::fail("The from path is empty."),
    };
    let to = match resolve_path(&ctx.workspace, to_raw) {
        Ok(path) => path,
        Err(_) => return ToolOutput::fail("The to path is empty."),
    };
    if !from.path.exists() {
        return ToolOutput::fail("The source path does not exist.");
    }
    if to.path.exists() && !args.get("overwrite").and_then(Value::as_bool).unwrap_or(false) {
        return ToolOutput::fail("The destination already exists. Set overwrite to true to replace it.");
    }
    if is_critical_path(&from.path) || is_critical_path(&to.path) || from.path == ctx.workspace {
        return ToolOutput::fail("This path cannot be moved or copied.");
    }
    if let Err(error) = capture(ctx, &from.path) {
        return error;
    }
    if to.path.exists() {
        if let Err(error) = capture(ctx, &to.path) {
            return error;
        }
    } else if let Err(error) = capture(ctx, &to.path) {
        return error;
    }
    if let Some(parent) = to.path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return ToolOutput::fail(format!("Could not create the destination folder: {error}"));
        }
    }
    let result = if rename {
        fs::rename(&from.path, &to.path).or_else(|_| {
            if from.path.is_dir() {
                copy_dir(&from.path, &to.path).and_then(|_| fs::remove_dir_all(&from.path))
            } else {
                fs::copy(&from.path, &to.path).and_then(|_| fs::remove_file(&from.path))
            }
        })
    } else if from.path.is_dir() {
        copy_dir(&from.path, &to.path)
    } else {
        fs::copy(&from.path, &to.path).map(|_| ())
    };
    if let Err(error) = result {
        return ToolOutput::fail(format!("The file operation failed: {error}"));
    }
    let kind = if rename { "moved" } else { "added" };
    let diff = format!(
        "{} {} -> {}",
        if rename { "moved" } else { "copied" },
        relative_display(&ctx.workspace, &from.path),
        relative_display(&ctx.workspace, &to.path)
    );
    let change = record_change(ctx, &to.path, kind, &diff);
    let mut output = ToolOutput::ok(diff);
    output.file_changes = vec![change];
    output.mutated = true;
    output
}

pub fn delete_file(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let Some(raw) = args.get("path").and_then(Value::as_str) else {
        return ToolOutput::fail("path is required.");
    };
    let resolved = match resolve_path(&ctx.workspace, raw) {
        Ok(path) => path,
        Err(_) => return ToolOutput::fail("The path is empty."),
    };
    if !resolved.path.exists() {
        return ToolOutput::fail("The path to delete does not exist.");
    }
    if resolved.path == ctx.workspace || is_critical_path(&resolved.path) {
        return ToolOutput::fail("The workspace root and system paths cannot be deleted.");
    }
    if resolved.path.is_dir() {
        let recursive = args.get("recursive").and_then(Value::as_bool).unwrap_or(false);
        if !recursive {
            return ToolOutput::fail("Deleting a directory requires recursive: true.");
        }
        let count = WalkDir::new(&resolved.path).into_iter().filter_map(Result::ok).count();
        if count > 200 {
            return ToolOutput::fail("More than 200 items cannot be deleted at once.");
        }
        for entry in WalkDir::new(&resolved.path).into_iter().filter_map(Result::ok) {
            if entry.path().is_file() {
                if let Err(error) = capture(ctx, entry.path()) {
                    return error;
                }
            }
        }
        if let Err(error) = capture(ctx, &resolved.path) {
            return error;
        }
        if let Err(error) = move_to_trash(&resolved.path) {
            return ToolOutput::fail(format!("Could not move the directory to the Trash: {error}"));
        }
    } else {
        if let Err(error) = capture(ctx, &resolved.path) {
            return error;
        }
        if let Err(error) = move_to_trash(&resolved.path) {
            return ToolOutput::fail(format!("Could not move the file to the Trash: {error}"));
        }
    }
    let change = record_change(
        ctx,
        &resolved.path,
        "deleted",
        &format!("trashed {}", relative_display(&ctx.workspace, &resolved.path)),
    );
    let mut output = ToolOutput::ok(format!(
        "trashed {}",
        relative_display(&ctx.workspace, &resolved.path)
    ));
    output.file_changes = vec![change];
    output.mutated = true;
    output
}

pub fn search_files(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let Some(query) = args.get("query").and_then(Value::as_str) else {
        return ToolOutput::fail("query is required.");
    };
    let root = search_root(args, ctx);
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(100).clamp(1, 300) as usize;
    let needle = query.to_ascii_lowercase();
    let mut matches = Vec::new();
    for entry in walk(&root) {
        if matches.len() >= limit {
            break;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if wildcard(&needle, &name) || name.contains(&needle) {
            matches.push(relative_display(&ctx.workspace, entry.path()));
        }
    }
    if matches.is_empty() {
        ToolOutput::ok("No matching files.")
    } else {
        ToolOutput::ok(matches.join("\n"))
    }
}

pub fn search_text(args: &Value, ctx: &ToolCtx) -> ToolOutput {
    let Some(query) = args.get("query").and_then(Value::as_str) else {
        return ToolOutput::fail("query is required.");
    };
    if query.is_empty() {
        return ToolOutput::fail("query is empty.");
    }
    let root = search_root(args, ctx);
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(80).clamp(1, 200) as usize;
    let case_sensitive = args.get("case_sensitive").and_then(Value::as_bool).unwrap_or(false);
    let mut matches = Vec::new();
    for entry in walk(&root) {
        if matches.len() >= limit || !entry.path().is_file() {
            if matches.len() >= limit {
                break;
            }
            continue;
        }
        if crate::safety::paths::is_sensitive_path(entry.path()) {
            continue;
        }
        if let Ok(canonical) = entry.path().canonicalize() {
            if crate::safety::paths::is_sensitive_path(&canonical) {
                continue;
            }
        }
        let Ok(metadata) = entry.metadata() else { continue };
        if metadata.len() > 1_000_000 {
            continue;
        }
        let Ok(bytes) = fs::read(entry.path()) else { continue };
        if bytes.contains(&0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        for (index, line) in text.lines().enumerate() {
            let hit = if case_sensitive {
                line.contains(query)
            } else {
                line.to_ascii_lowercase().contains(&query.to_ascii_lowercase())
            };
            if hit {
                matches.push(format!(
                    "{}:{}: {}",
                    relative_display(&ctx.workspace, entry.path()),
                    index + 1,
                    line.trim()
                ));
                if matches.len() >= limit {
                    break;
                }
            }
        }
    }
    let body = if matches.is_empty() {
        "No matching text.".to_string()
    } else {
        matches.join("\n")
    };
    ToolOutput::ok(truncate_observation(&body, 12_000))
}

fn search_root(args: &Value, ctx: &ToolCtx) -> PathBuf {
    args.get("path")
        .and_then(Value::as_str)
        .and_then(|raw| resolve_path(&ctx.workspace, raw).ok())
        .filter(|resolved| resolved.inside_workspace)
        .map(|resolved| resolved.path)
        .unwrap_or_else(|| ctx.workspace.clone())
}

fn walk(root: &Path) -> Vec<walkdir::DirEntry> {
    WalkDir::new(root)
        .max_depth(8)
        .into_iter()
        .filter_entry(|entry| !should_skip_entry(entry.path(), root))
        .filter_map(Result::ok)
        .take(20_000)
        .collect()
}

fn wildcard(pattern: &str, name: &str) -> bool {
    if !pattern.contains('*') && !pattern.contains('?') {
        return false;
    }
    simple_glob(pattern, name)
}

fn simple_glob(pattern: &str, text: &str) -> bool {
    fn rec(pattern: &[char], text: &[char]) -> bool {
        if pattern.is_empty() {
            return text.is_empty();
        }
        match pattern[0] {
            '*' => rec(&pattern[1..], text) || (!text.is_empty() && rec(pattern, &text[1..])),
            '?' => !text.is_empty() && rec(&pattern[1..], &text[1..]),
            other => !text.is_empty() && text[0] == other && rec(&pattern[1..], &text[1..]),
        }
    }
    rec(
        &pattern.chars().collect::<Vec<_>>(),
        &text.chars().collect::<Vec<_>>(),
    )
}

fn move_to_trash(path: &Path) -> std::io::Result<()> {
    let Some(home) = dirs::home_dir() else {
        return if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };
    };
    let trash = home.join(".Trash");
    fs::create_dir_all(&trash)?;
    let name = path.file_name().unwrap_or_default();
    let mut dest = trash.join(name);
    if dest.exists() {
        dest = trash.join(format!("{}-{}", crate::util::new_id(), name.to_string_lossy()));
    }
    fs::rename(path, &dest).or_else(|_| {
        if path.is_dir() {
            copy_dir(path, &dest).and_then(|_| fs::remove_dir_all(path))
        } else {
            fs::copy(path, &dest).and_then(|_| fs::remove_file(path))
        }
    })
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn capture(ctx: &ToolCtx, path: &Path) -> Result<(), ToolOutput> {
    ctx.db
        .capture_snapshot(&ctx.task_id, path)
        .map_err(|error| ToolOutput::fail(error.to_string()))
}

fn record_change(ctx: &ToolCtx, path: &Path, kind: &str, diff: &str) -> FileChange {
    let change = FileChange {
        path: relative_display(&ctx.workspace, path),
        kind: kind.to_string(),
        diff: truncate_observation(diff, 60_000),
    };
    let _ = ctx.db.insert_file_change(&ctx.task_id, &change);
    change
}

fn edit_message(error: &EditError) -> String {
    match error {
        EditError::EmptyOld => "old_string is empty.".into(),
        EditError::NotFound => "old_string does not match the file. Read the file again and copy it exactly.".into(),
        EditError::Ambiguous(count) => format!("old_string appears {count} times. Include more surrounding context."),
        EditError::Patch(message) => format!("diff error: {message}"),
    }
}
