use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Agent,
    Ask,
    Plan,
    Do,
    Mission,
}

impl Mode {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "ask" => Self::Ask,
            "plan" | "preview" => Self::Plan,
            "do" => Self::Do,
            "mission" => Self::Mission,
            _ => Self::Agent,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Ask => "ask",
            Self::Plan => "plan",
            Self::Do => "do",
            Self::Mission => "mission",
        }
    }

    pub fn mutates(self) -> bool {
        matches!(self, Self::Agent | Self::Do | Self::Mission)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Safe,
    Caution,
    Dangerous,
}

impl RiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Caution => "caution",
            Self::Dangerous => "dangerous",
        }
    }

    pub fn raise(self, other: Self) -> Self {
        match (self, other) {
            (Self::Dangerous, _) | (_, Self::Dangerous) => Self::Dangerous,
            (Self::Caution, _) | (_, Self::Caution) => Self::Caution,
            _ => Self::Safe,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub ollama_url: String,
    pub model: String,
    pub temperature: f32,
    pub context_length: u32,
    pub max_iterations: u32,
    pub auto_approve_safe: bool,
    pub auto_approve_file_edits: bool,
    pub terminal_timeout_ms: u64,
    pub system_prompt: String,
    pub mode: String,
    pub workspace_path: Option<String>,
    #[serde(default)]
    pub auto_approve_dangerous: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ollama_url: "http://127.0.0.1:11434".into(),
            model: String::new(),
            temperature: 0.2,
            context_length: 8192,
            max_iterations: 40,
            auto_approve_safe: true,
            auto_approve_file_edits: true,
            terminal_timeout_ms: 120_000,
            system_prompt: String::new(),
            mode: "agent".into(),
            workspace_path: None,
            auto_approve_dangerous: false,
        }
    }
}

impl Settings {
    pub fn normalized(mut self) -> Self {
        self.ollama_url = self.ollama_url.trim().trim_end_matches('/').to_string();
        if self.ollama_url.is_empty() {
            self.ollama_url = Settings::default().ollama_url;
        }
        self.temperature = self.temperature.clamp(0.0, 2.0);
        self.context_length = self.context_length.clamp(1024, 131_072);
        self.max_iterations = self.max_iterations.clamp(1, 100);
        self.terminal_timeout_ms = self.terminal_timeout_ms.clamp(1_000, 600_000);
        self.mode = match Mode::parse(&self.mode) {
            Mode::Ask => "ask".to_string(),
            Mode::Do => "do".to_string(),
            Mode::Mission => "mission".to_string(),
            Mode::Plan => "plan".to_string(),
            Mode::Agent => "agent".to_string(),
        };
        if let Some(path) = self.workspace_path.as_mut() {
            let trimmed = path.trim().to_string();
            if trimmed.is_empty() {
                self.workspace_path = None;
            } else {
                *path = trimmed;
            }
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub id: String,
    pub title: String,
    pub status: String,
}

impl PlanStep {
    pub fn normalized(mut self, index: usize) -> Self {
        if self.id.trim().is_empty() {
            self.id = (index + 1).to_string();
        }
        self.title = self.title.trim().to_string();
        self.status = match self.status.trim() {
            "running" | "completed" | "failed" => self.status.trim().to_string(),
            _ => "pending".into(),
        };
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub kind: String,
    pub diff: String,
}

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub success: bool,
    pub content: String,
    pub file_changes: Vec<FileChange>,
    pub plan: Option<Vec<PlanStep>>,
    pub mutated: bool,
    pub images: Vec<String>,
}

impl ToolOutput {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            success: true,
            content: content.into(),
            file_changes: Vec::new(),
            plan: None,
            mutated: false,
            images: Vec::new(),
        }
    }

    pub fn fail(content: impl Into<String>) -> Self {
        Self {
            success: false,
            content: content.into(),
            file_changes: Vec::new(),
            plan: None,
            mutated: false,
            images: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
            images: None,
            tool_calls: None,
            tool_name: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            images: None,
            tool_calls: None,
            tool_name: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            images: None,
            tool_calls: None,
            tool_name: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaToolCall {
    pub function: OllamaFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaFunctionCall {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolRequest {
    pub name: String,
    pub arguments: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ask_do_mission_and_preview() {
        assert_eq!(Mode::parse("plan"), Mode::Plan);
        assert_eq!(Mode::parse("ask"), Mode::Ask);
        assert_eq!(Mode::parse("do"), Mode::Do);
        assert_eq!(Mode::parse("mission"), Mode::Mission);
        assert!(Mode::Do.mutates());
        assert!(!Mode::Ask.mutates());
        assert!(!Mode::Plan.mutates());
        let settings = Settings {
            mode: "plan".into(),
            ..Settings::default()
        }
        .normalized();
        assert_eq!(settings.mode, "plan");
    }
}
