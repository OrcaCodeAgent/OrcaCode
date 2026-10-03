use std::fs;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::agent::cancel::CancelFlag;
use crate::tools::terminal::run_command;

pub struct Check {
    pub label: String,
    pub command: String,
}

pub fn detect_checks(workspace: &Path) -> Vec<Check> {
    if let Some(checks) = node_checks(workspace) {
        return checks;
    }
    if workspace.join("Cargo.toml").is_file() {
        let mut checks = vec![Check {
            label: "cargo check".into(),
            command: "cargo check".into(),
        }];
        if workspace.join("tests").is_dir() {
            checks.push(Check {
                label: "cargo test".into(),
                command: "cargo test".into(),
            });
        }
        return checks;
    }
    if workspace.join("go.mod").is_file() {
        return vec![Check {
            label: "go build".into(),
            command: "go build ./...".into(),
        }];
    }
    if workspace.join("pyproject.toml").is_file() || workspace.join("requirements.txt").is_file() {
        if workspace.join("tests").is_dir() || workspace.join("pytest.ini").is_file() {
            return vec![Check {
                label: "pytest".into(),
                command: "python3 -m pytest -q".into(),
            }];
        }
    }
    if workspace.join("Makefile").is_file() {
        return vec![Check {
            label: "make".into(),
            command: "make".into(),
        }];
    }
    Vec::new()
}

pub fn run_checks(workspace: &Path, timeout: Duration, cancel: &CancelFlag) -> (bool, String) {
    let checks = detect_checks(workspace);
    if checks.is_empty() {
        return (true, "No build or test command was found. Skipping a separate verification.".into());
    }
    let mut report = String::new();
    for check in checks {
        if cancel.is_cancelled() {
            return (false, "Verification was cancelled.".into());
        }
        report.push_str(&format!("# {}\n$ {}\n", check.label, check.command));
        let output = run_command(&check.command, workspace, timeout, cancel);
        report.push_str(&output.stdout);
        if !output.stderr.is_empty() {
            report.push('\n');
            report.push_str(&output.stderr);
        }
        report.push_str(&format!(
            "\nexit: {}\n",
            output.exit_code.map(|code| code.to_string()).unwrap_or_else(|| "null".into())
        ));
        if !output.succeeded() {
            return (false, report);
        }
    }
    (true, report)
}

fn node_checks(workspace: &Path) -> Option<Vec<Check>> {
    let path = workspace.join("package.json");
    if !path.is_file() {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let scripts = value.get("scripts")?.as_object()?;
    let runner = if workspace.join("pnpm-lock.yaml").is_file() {
        "pnpm"
    } else if workspace.join("yarn.lock").is_file() {
        "yarn"
    } else if workspace.join("bun.lock").is_file() {
        "bun"
    } else {
        "npm"
    };
    let mut checks = Vec::new();
    if scripts.contains_key("build") {
        checks.push(Check {
            label: "build".into(),
            command: format!("{runner} run build"),
        });
    } else if scripts.contains_key("typecheck") {
        checks.push(Check {
            label: "typecheck".into(),
            command: format!("{runner} run typecheck"),
        });
    } else if scripts.contains_key("lint") {
        checks.push(Check {
            label: "lint".into(),
            command: format!("{runner} run lint"),
        });
    }
    if let Some(test) = scripts.get("test").and_then(Value::as_str) {
        if !test.contains("no test specified") {
            checks.push(Check {
                label: "test".into(),
                command: format!("{runner} test"),
            });
        }
    }
    Some(checks)
}
