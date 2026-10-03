use std::fs;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::agent::cancel::CancelFlag;
use crate::domain::Mode;
use crate::safety::paths::{relative_display, should_skip_entry};
use crate::safety::truncate::truncate_observation;
use crate::tools::terminal::run_command;

const MANIFESTS: &[&str] = &[
    "README.md",
    "readme.md",
    "package.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lock",
    "pyproject.toml",
    "requirements.txt",
    "Cargo.toml",
    "CMakeLists.txt",
    "Makefile",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "composer.json",
    "Gemfile",
    "Package.swift",
];

pub fn build_briefing(workspace: &Path, goal: &str, mode: Mode, cancel: &CancelFlag) -> String {
    let mut text = format!(
        "Workspace: {}\nMode: {}\n\nGoal:\n{goal}\n\nWorkspace briefing:\n",
        workspace.display(),
        mode.as_str()
    );
    text.push_str(&top_level(workspace));
    text.push('\n');
    text.push_str(&manifests(workspace));
    if workspace.join(".git").exists() {
        let status = run_command("git status --short --branch", workspace, Duration::from_secs(8), cancel);
        text.push_str("\nGit status:\n");
        text.push_str(status.stdout.trim());
        if !status.stderr.trim().is_empty() {
            text.push_str("\n");
            text.push_str(status.stderr.trim());
        }
        text.push('\n');
    }
    truncate_observation(&text, 7_000)
}

fn top_level(workspace: &Path) -> String {
    let mut lines = vec!["Top level:".to_string()];
    let Ok(entries) = fs::read_dir(workspace) else {
        return "Top level: unreadable".into();
    };
    let mut names = entries
        .flatten()
        .filter(|entry| !should_skip_entry(&entry.path(), workspace))
        .map(|entry| {
            let dir = entry.path().is_dir();
            format!("{}{}", entry.file_name().to_string_lossy(), if dir { "/" } else { "" })
        })
        .collect::<Vec<_>>();
    names.sort();
    for name in names.into_iter().take(80) {
        lines.push(format!("- {name}"));
    }
    lines.join("\n")
}

fn manifests(workspace: &Path) -> String {
    let mut lines = vec!["Project files:".to_string()];
    for name in MANIFESTS {
        let path = workspace.join(name);
        if !path.is_file() {
            continue;
        }
        lines.push(format!("- {}", relative_display(workspace, &path)));
        if *name == "package.json" {
            if let Some(summary) = package_summary(&path) {
                lines.push(summary);
            }
        } else if !name.ends_with("lock") && *name != "pnpm-lock.yaml" {
            if let Ok(text) = fs::read_to_string(&path) {
                let preview: String = text.chars().take(700).collect();
                lines.push(preview);
            }
        }
    }
    lines.join("\n")
}

fn package_summary(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let name = value.get("name").and_then(Value::as_str).unwrap_or("");
    let scripts = value
        .get("scripts")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(key, script)| format!("  {key}: {}", script.as_str().unwrap_or("")))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    Some(format!("package {name}\nscripts:\n{scripts}"))
}
