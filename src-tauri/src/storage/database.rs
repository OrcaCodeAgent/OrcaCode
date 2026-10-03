use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::domain::{FileChange, PlanStep, Settings};
use crate::error::{AppError, AppResult};
use crate::util::{new_id, now_ms, title_from_goal, workspace_name};

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_opened_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    workspace_id TEXT,
    title TEXT NOT NULL,
    mode TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS tool_calls (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    name TEXT NOT NULL,
    arguments TEXT NOT NULL,
    risk TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS tool_results (
    id TEXT PRIMARY KEY,
    tool_call_id TEXT NOT NULL,
    success INTEGER NOT NULL,
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    goal TEXT NOT NULL,
    status TEXT NOT NULL,
    plan_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS permission_history (
    id TEXT PRIMARY KEY,
    conversation_id TEXT,
    tool_name TEXT NOT NULL,
    arguments TEXT NOT NULL,
    risk TEXT NOT NULL,
    decision TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS snapshots (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    path TEXT NOT NULL,
    existed INTEGER NOT NULL,
    is_dir INTEGER NOT NULL,
    original_content BLOB,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS file_changes (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    path TEXT NOT NULL,
    kind TEXT NOT NULL,
    diff TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_snapshots_task_path ON snapshots(task_id, path);
CREATE INDEX IF NOT EXISTS idx_messages_conv ON messages(conversation_id, created_at);
CREATE INDEX IF NOT EXISTS idx_tools_conv ON tool_calls(conversation_id, created_at);
CREATE INDEX IF NOT EXISTS idx_file_changes_task ON file_changes(task_id, created_at);
"#;

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRecord {
    pub id: String,
    pub path: String,
    pub name: String,
    pub last_opened_at: i64,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub mode: String,
    pub updated_at: i64,
    pub workspace_path: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptItem {
    pub id: String,
    pub kind: String,
    pub content: String,
    pub name: Option<String>,
    pub arguments: Option<Value>,
    pub success: Option<bool>,
    pub risk: Option<String>,
    pub status: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDetail {
    pub id: String,
    pub title: String,
    pub mode: String,
    pub workspace_path: Option<String>,
    pub items: Vec<TranscriptItem>,
    pub plan: Vec<PlanStep>,
    pub file_changes: Vec<FileChange>,
    pub task_id: Option<String>,
}

struct SnapshotRow {
    path: String,
    existed: bool,
    is_dir: bool,
    content: Option<Vec<u8>>,
}

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                AppError::message(format!("Could not create the data folder: {error}"))
            })?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|error| error.into_inner())
    }

    pub fn load_settings(&self) -> AppResult<Settings> {
        let conn = self.lock();
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'settings'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(value) = value else {
            return Ok(Settings::default());
        };
        match serde_json::from_str::<Settings>(&value) {
            Ok(settings) => Ok(settings.normalized()),
            Err(error) => {
                crate::logging::log_line("error", &format!("settings parse: {error}"));
                Ok(Settings::default())
            }
        }
    }

    pub fn save_settings(&self, settings: &Settings) -> AppResult<()> {
        let json = serde_json::to_string(settings).map_err(|error| {
            AppError::message(format!("Could not save settings: {error}"))
        })?;
        self.lock().execute(
            "INSERT INTO settings (key, value) VALUES ('settings', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    }

    pub fn remember_workspace(&self, path: &str) -> AppResult<WorkspaceRecord> {
        let now = now_ms();
        let conn = self.lock();
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM workspaces WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .optional()?;
        let id = existing.unwrap_or_else(new_id);
        let name = workspace_name(path);
        conn.execute(
            "INSERT INTO workspaces (id, path, name, created_at, last_opened_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET name = excluded.name, last_opened_at = excluded.last_opened_at",
            params![id, path, name, now, now],
        )?;
        Ok(WorkspaceRecord {
            id,
            path: path.to_string(),
            name,
            last_opened_at: now,
        })
    }

    pub fn list_workspaces(&self) -> AppResult<Vec<WorkspaceRecord>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, path, name, last_opened_at FROM workspaces ORDER BY last_opened_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(WorkspaceRecord {
                id: row.get(0)?,
                path: row.get(1)?,
                name: row.get(2)?,
                last_opened_at: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn create_conversation(
        &self,
        workspace_id: Option<&str>,
        goal: &str,
        mode: &str,
    ) -> AppResult<String> {
        let id = new_id();
        let now = now_ms();
        self.lock().execute(
            "INSERT INTO conversations (id, workspace_id, title, mode, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, workspace_id, title_from_goal(goal), mode, now, now],
        )?;
        Ok(id)
    }

    pub fn rename_conversation(&self, id: &str, title: &str) -> AppResult<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(crate::error::AppError::message("Enter a title."));
        }
        let title: String = title.chars().take(80).collect();
        let changed = self.lock().execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now_ms(), id],
        )?;
        if changed == 0 {
            return Err(crate::error::AppError::message("Could not find that conversation."));
        }
        Ok(())
    }

    pub fn touch_conversation(&self, id: &str, mode: &str) -> AppResult<()> {
        self.lock().execute(
            "UPDATE conversations SET updated_at = ?1, mode = ?2 WHERE id = ?3",
            params![now_ms(), mode, id],
        )?;
        Ok(())
    }

    pub fn list_conversations(&self) -> AppResult<Vec<ConversationSummary>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT c.id, c.title, c.mode, c.updated_at, w.path
             FROM conversations c
             LEFT JOIN workspaces w ON w.id = c.workspace_id
             ORDER BY c.updated_at DESC
             LIMIT 200",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ConversationSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                mode: row.get(2)?,
                updated_at: row.get(3)?,
                workspace_path: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_conversation(&self, id: &str) -> AppResult<Option<ConversationDetail>> {
        let summary = {
            let conn = self.lock();
            let found = conn
                .query_row(
                    "SELECT c.id, c.title, c.mode, w.path
                     FROM conversations c
                     LEFT JOIN workspaces w ON w.id = c.workspace_id
                     WHERE c.id = ?1",
                    params![id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .optional()?;
            found
        };
        let Some((conv_id, title, mode, workspace_path)) = summary else {
            return Ok(None);
        };
        let mut items = self.transcript(&conv_id)?;
        items.sort_by_key(|item| item.created_at);
        let task = self.latest_task(&conv_id)?;
        let (plan, file_changes, task_id) = if let Some((task_id, plan_json)) = task {
            let plan = serde_json::from_str(&plan_json).unwrap_or_default();
            let changes = self.file_changes(&task_id)?;
            (plan, changes, Some(task_id))
        } else {
            (Vec::new(), Vec::new(), None)
        };
        Ok(Some(ConversationDetail {
            id: conv_id,
            title,
            mode,
            workspace_path,
            items,
            plan,
            file_changes,
            task_id,
        }))
    }

    fn transcript(&self, conversation_id: &str) -> AppResult<Vec<TranscriptItem>> {
        let conn = self.lock();
        let mut items = Vec::new();
        let mut messages = conn.prepare(
            "SELECT id, role, content, created_at FROM messages WHERE conversation_id = ?1",
        )?;
        let rows = messages.query_map(params![conversation_id], |row| {
            Ok(TranscriptItem {
                id: row.get(0)?,
                kind: row.get(1)?,
                content: row.get(2)?,
                name: None,
                arguments: None,
                success: None,
                risk: None,
                status: None,
                created_at: row.get(3)?,
            })
        })?;
        for row in rows {
            items.push(row?);
        }
        drop(messages);
        let mut tools = conn.prepare(
            "SELECT c.id, c.name, c.arguments, c.risk, c.status, c.created_at, r.success, r.content
             FROM tool_calls c
             LEFT JOIN tool_results r ON r.tool_call_id = c.id
             WHERE c.conversation_id = ?1",
        )?;
        let rows = tools.query_map(params![conversation_id], |row| {
            let arguments_text: String = row.get(2)?;
            let success: Option<i64> = row.get(6)?;
            Ok(TranscriptItem {
                id: row.get(0)?,
                kind: "tool".into(),
                content: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
                name: Some(row.get(1)?),
                arguments: serde_json::from_str(&arguments_text).ok(),
                success: success.map(|value| value != 0),
                risk: Some(row.get(3)?),
                status: Some(row.get(4)?),
                created_at: row.get(5)?,
            })
        })?;
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn delete_conversation(&self, id: &str) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM messages WHERE conversation_id = ?1", params![id])?;
        let tool_ids: Vec<String> = {
            let mut stmt =
                conn.prepare("SELECT id FROM tool_calls WHERE conversation_id = ?1")?;
            let rows = stmt.query_map(params![id], |row| row.get(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for tool_id in tool_ids {
            conn.execute(
                "DELETE FROM tool_results WHERE tool_call_id = ?1",
                params![tool_id],
            )?;
        }
        conn.execute("DELETE FROM tool_calls WHERE conversation_id = ?1", params![id])?;
        let task_ids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT id FROM tasks WHERE conversation_id = ?1")?;
            let rows = stmt.query_map(params![id], |row| row.get(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for task_id in task_ids {
            conn.execute("DELETE FROM snapshots WHERE task_id = ?1", params![task_id])?;
            conn.execute(
                "DELETE FROM file_changes WHERE task_id = ?1",
                params![task_id],
            )?;
        }
        conn.execute("DELETE FROM tasks WHERE conversation_id = ?1", params![id])?;
        conn.execute(
            "DELETE FROM permission_history WHERE conversation_id = ?1",
            params![id],
        )?;
        conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn insert_message(&self, conversation_id: &str, role: &str, content: &str) -> AppResult<String> {
        let id = new_id();
        self.lock().execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, conversation_id, role, content, now_ms()],
        )?;
        Ok(id)
    }

    pub fn insert_tool_call(
        &self,
        id: &str,
        conversation_id: &str,
        name: &str,
        arguments: &str,
        risk: &str,
        status: &str,
    ) -> AppResult<()> {
        self.lock().execute(
            "INSERT INTO tool_calls (id, conversation_id, name, arguments, risk, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, conversation_id, name, arguments, risk, status, now_ms()],
        )?;
        Ok(())
    }

    pub fn finish_tool_call(
        &self,
        id: &str,
        status: &str,
        success: bool,
        content: &str,
    ) -> AppResult<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE tool_calls SET status = ?1 WHERE id = ?2",
            params![status, id],
        )?;
        conn.execute(
            "INSERT INTO tool_results (id, tool_call_id, success, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![new_id(), id, success as i64, content, now_ms()],
        )?;
        Ok(())
    }

    pub fn create_task(&self, conversation_id: &str, goal: &str) -> AppResult<String> {
        let id = new_id();
        let now = now_ms();
        self.lock().execute(
            "INSERT INTO tasks (id, conversation_id, goal, status, plan_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'running', '[]', ?4, ?5)",
            params![id, conversation_id, goal, now, now],
        )?;
        Ok(id)
    }

    pub fn update_task(&self, task_id: &str, status: &str, plan: &[PlanStep]) -> AppResult<()> {
        let plan_json = serde_json::to_string(plan).unwrap_or_else(|_| "[]".into());
        self.lock().execute(
            "UPDATE tasks SET status = ?1, plan_json = ?2, updated_at = ?3 WHERE id = ?4",
            params![status, plan_json, now_ms(), task_id],
        )?;
        Ok(())
    }

    fn latest_task(&self, conversation_id: &str) -> AppResult<Option<(String, String)>> {
        let row = self
            .lock()
            .query_row(
                "SELECT id, plan_json FROM tasks WHERE conversation_id = ?1 ORDER BY created_at DESC LIMIT 1",
                params![conversation_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(row)
    }

    pub fn record_permission(
        &self,
        conversation_id: &str,
        tool_name: &str,
        arguments: &str,
        risk: &str,
        decision: &str,
    ) -> AppResult<()> {
        self.lock().execute(
            "INSERT INTO permission_history
             (id, conversation_id, tool_name, arguments, risk, decision, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![new_id(), conversation_id, tool_name, arguments, risk, decision, now_ms()],
        )?;
        Ok(())
    }

    pub fn capture_snapshot(&self, task_id: &str, path: &Path) -> AppResult<()> {
        let path_text = path.to_string_lossy().to_string();
        {
            let exists: Option<String> = self
                .lock()
                .query_row(
                    "SELECT id FROM snapshots WHERE task_id = ?1 AND path = ?2",
                    params![task_id, path_text],
                    |row| row.get(0),
                )
                .optional()?;
            if exists.is_some() {
                return Ok(());
            }
        }
        let metadata = fs::metadata(path).ok();
        let existed = metadata.is_some();
        let is_dir = metadata.as_ref().map(|meta| meta.is_dir()).unwrap_or(false);
        if let Some(meta) = &metadata {
            if meta.is_file() && meta.len() > 20_000_000 {
                return Err(AppError::message(
                    "Files larger than 20MB cannot be snapshotted, so this file was not changed.",
                ));
            }
        }
        let content = if existed && !is_dir {
            Some(fs::read(path).map_err(|error| {
                AppError::message(format!("Could not read the original file: {error}"))
            })?)
        } else {
            None
        };
        self.lock().execute(
            "INSERT INTO snapshots (id, task_id, path, existed, is_dir, original_content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![new_id(), task_id, path_text, existed as i64, is_dir as i64, content, now_ms()],
        )?;
        Ok(())
    }

    pub fn insert_file_change(&self, task_id: &str, change: &FileChange) -> AppResult<()> {
        self.lock().execute(
            "INSERT INTO file_changes (id, task_id, path, kind, diff, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![new_id(), task_id, change.path, change.kind, change.diff, now_ms()],
        )?;
        Ok(())
    }

    pub fn file_changes(&self, task_id: &str) -> AppResult<Vec<FileChange>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT path, kind, diff FROM file_changes WHERE task_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![task_id], |row| {
            Ok(FileChange {
                path: row.get(0)?,
                kind: row.get(1)?,
                diff: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn undo_task(&self, task_id: &str) -> AppResult<(Vec<String>, Vec<String>)> {
        let rows = {
            let conn = self.lock();
            let mut stmt = conn.prepare(
                "SELECT path, existed, is_dir, original_content FROM snapshots WHERE task_id = ?1",
            )?;
            let mapped = stmt.query_map(params![task_id], |row| {
                Ok(SnapshotRow {
                    path: row.get(0)?,
                    existed: row.get::<_, i64>(1)? != 0,
                    is_dir: row.get::<_, i64>(2)? != 0,
                    content: row.get(3)?,
                })
            })?;
            mapped.collect::<Result<Vec<_>, _>>()?
        };
        if rows.is_empty() {
            return Err(AppError::message("There are no agent changes to undo."));
        }
        let mut restored = Vec::new();
        let mut skipped = Vec::new();
        let mut created_dirs: Vec<&SnapshotRow> = rows.iter().filter(|row| !row.existed && row.is_dir).collect();
        created_dirs.sort_by_key(|row| std::cmp::Reverse(Path::new(&row.path).components().count()));
        let existed_dirs: Vec<&SnapshotRow> = rows.iter().filter(|row| row.existed && row.is_dir).collect();
        let mut created_counts: std::collections::HashMap<std::ffi::OsString, usize> = std::collections::HashMap::new();
        for row in &created_dirs {
            if let Some(name) = Path::new(&row.path).file_name() {
                *created_counts.entry(name.to_os_string()).or_default() += 1;
            }
        }
        let mut missing_sources: std::collections::HashMap<std::ffi::OsString, Vec<&SnapshotRow>> = std::collections::HashMap::new();
        for row in &existed_dirs {
            let path = Path::new(&row.path);
            if path.exists() {
                continue;
            }
            if let Some(name) = path.file_name() {
                missing_sources.entry(name.to_os_string()).or_default().push(row);
            }
        }
        let mut used_sources = std::collections::HashSet::new();
        let mut renamed_destinations = Vec::new();
        for row in &created_dirs {
            let path = PathBuf::from(&row.path);
            let Some(name) = path.file_name().map(|name| name.to_os_string()) else {
                continue;
            };
            let sources = missing_sources.get(&name).map(Vec::as_slice).unwrap_or(&[]);
            let unique = created_counts.get(&name).copied().unwrap_or(0) == 1 && sources.len() == 1;
            if unique && path.exists() {
                let source = sources[0];
                if let Some(parent) = Path::new(&source.path).parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        AppError::message(format!("Could not create the parent folder: {error}"))
                    })?;
                }
                fs::rename(&path, &source.path).map_err(|error| {
                    AppError::message(format!("Could not move {} back: {error}", row.path))
                })?;
                used_sources.insert(source.path.clone());
                renamed_destinations.push(path);
                restored.push(source.path.clone());
            }
        }
        for row in rows.iter().filter(|row| !row.existed && !row.is_dir) {
            let path = PathBuf::from(&row.path);
            if renamed_destinations.iter().any(|dir| path.starts_with(dir)) {
                continue;
            }
            if path.exists() {
                fs::remove_file(&path).map_err(|error| {
                    AppError::message(format!("Could not delete {}: {error}", row.path))
                })?;
            }
            restored.push(row.path.clone());
        }
        for row in &created_dirs {
            if restored.iter().any(|item| item == &row.path) || used_sources.contains(&row.path) {
                continue;
            }
            let path = PathBuf::from(&row.path);
            if renamed_destinations.iter().any(|dir| path == *dir) {
                continue;
            }
            if !path.exists() {
                restored.push(row.path.clone());
                continue;
            }
            let non_empty = path.read_dir().map(|mut items| items.next().is_some()).unwrap_or(false);
            if non_empty {
                skipped.push(row.path.clone());
                continue;
            }
            fs::remove_dir_all(&path).map_err(|error| {
                AppError::message(format!("Could not delete {}: {error}", row.path))
            })?;
            restored.push(row.path.clone());
        }
        for row in rows.iter().filter(|row| row.existed) {
            let path = PathBuf::from(&row.path);
            if row.is_dir {
                if !used_sources.contains(&row.path) {
                    fs::create_dir_all(&path).map_err(|error| {
                        AppError::message(format!("Could not restore the directory: {error}"))
                    })?;
                }
            } else if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    AppError::message(format!("Could not create the parent folder: {error}"))
                })?;
                let mut file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&path)
                    .map_err(|error| AppError::message(format!("Could not restore the file: {error}")))?;
                file.write_all(row.content.as_deref().unwrap_or_default())
                    .map_err(|error| AppError::message(format!("Could not restore the file: {error}")))?;
            }
            restored.push(row.path.clone());
        }
        Ok((restored, skipped))
    }
}

