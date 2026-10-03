use std::time::Duration;

use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};

use crate::agent::cancel::CancelFlag;
use crate::domain::{ChatMessage, OllamaFunctionCall, OllamaToolCall};
use crate::error::{AppError, AppResult};

#[derive(Clone)]
pub struct OllamaClient {
    http: Client,
}

#[derive(Debug, Clone)]
pub struct ModelOutput {
    pub content: String,
    pub tool_calls: Vec<OllamaToolCall>,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaStatus {
    pub online: bool,
    pub message: String,
}

impl OllamaClient {
    pub fn new() -> AppResult<Self> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .build()
            .map_err(|error| AppError::message(format!("Could not create the HTTP client: {error}")))?;
        Ok(Self { http })
    }

    pub async fn status(&self, base: &str) -> OllamaStatus {
        match self.models(base).await {
            Ok(_) => OllamaStatus {
                online: true,
                message: "Connected to Ollama.".into(),
            },
            Err(error) => OllamaStatus {
                online: false,
                message: error.to_string(),
            },
        }
    }

    pub async fn models(&self, base: &str) -> AppResult<Vec<String>> {
        let response = self
            .http
            .get(format!("{}/api/tags", trim_base(base)))
            .timeout(Duration::from_secs(4))
            .send()
            .await
            .map_err(map_reqwest)?;
        if !response.status().is_success() {
            return Err(AppError::OllamaConnection);
        }
        let body: Value = response.json().await.map_err(map_reqwest)?;
        let models = body
            .get("models")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        item.get("name")
                            .and_then(Value::as_str)
                            .or_else(|| item.get("model").and_then(Value::as_str))
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(models)
    }

    pub async fn unload(&self, base: &str, model: &str) -> AppResult<()> {
        let response = self
            .http
            .post(format!("{}/api/generate", trim_base(base)))
            .json(&json!({ "model": model, "keep_alive": 0 }))
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(map_reqwest)?;
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ollama_error(&text));
        }
        Ok(())
    }

    pub async fn pull(&self, base: &str, name: &str, mut on_status: impl FnMut(String, u64, u64)) -> AppResult<()> {
        let response = self
            .http
            .post(format!("{}/api/pull", trim_base(base)))
            .json(&json!({ "name": name, "stream": true }))
            .send()
            .await
            .map_err(map_reqwest)?;
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ollama_error(&text));
        }
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(map_reqwest)?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(index) = buffer.find('\n') {
                let line = buffer[..index].trim().to_string();
                buffer.drain(..=index);
                if line.is_empty() {
                    continue;
                }
                let value: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
                if let Some(error) = value.get("error").and_then(Value::as_str) {
                    return Err(AppError::message(error.to_string()));
                }
                let status = value.get("status").and_then(Value::as_str).unwrap_or("downloading").to_string();
                let completed = value.get("completed").and_then(Value::as_u64).unwrap_or(0);
                let total = value.get("total").and_then(Value::as_u64).unwrap_or(0);
                on_status(status, completed, total);
            }
        }
        Ok(())
    }

    pub async fn chat(
        &self,
        base: &str,
        model: &str,
        messages: &[ChatMessage],
        tools: Option<&Vec<Value>>,
        temperature: f32,
        num_ctx: u32,
        cancel: &CancelFlag,
        mut on_token: impl FnMut(&str),
    ) -> AppResult<ModelOutput> {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        let mut body = json!({
            "model": model,
            "messages": messages,
            "stream": true,
            "keep_alive": "5m",
            "options": {
                "temperature": temperature,
                "num_ctx": num_ctx
            }
        });
        if let Some(tools) = tools {
            body["tools"] = json!(tools);
        }
        if model.to_ascii_lowercase().contains("qwen3") {
            body["think"] = json!(false);
        }
        let response = tokio::select! {
            _ = cancelled(cancel) => return Err(AppError::Cancelled),
            result = self.http.post(format!("{}/api/chat", trim_base(base))).json(&body).send() => {
                result.map_err(map_reqwest)?
            }
        };
        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(ollama_error(&text));
        }
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut content = String::new();
        let mut tool_calls = Vec::new();
        loop {
            let chunk = tokio::select! {
                _ = cancelled(cancel) => return Err(AppError::Cancelled),
                item = stream.next() => item,
            };
            let Some(item) = chunk else { break };
            let bytes = item.map_err(map_reqwest)?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));
            while let Some(index) = buffer.find('\n') {
                let line = buffer[..index].trim().to_string();
                buffer.drain(..=index);
                if line.is_empty() {
                    continue;
                }
                let value: Value = serde_json::from_str(&line).map_err(|error| {
                    crate::logging::log_line("error", &format!("ollama stream: {error}"));
                    AppError::message("Could not parse the Ollama response.")
                })?;
                if let Some(error) = value.get("error").and_then(Value::as_str) {
                    return Err(ollama_error(error));
                }
                if let Some(delta) = value.pointer("/message/content").and_then(Value::as_str) {
                    if !delta.is_empty() {
                        content.push_str(delta);
                        on_token(delta);
                    }
                }
                if let Some(calls) = value.pointer("/message/tool_calls") {
                    absorb_tool_calls(&mut tool_calls, calls);
                }
            }
        }
        Ok(ModelOutput { content, tool_calls })
    }
}

fn absorb_tool_calls(dst: &mut Vec<OllamaToolCall>, incoming: &Value) {
    let Some(items) = incoming.as_array() else {
        return;
    };
    for (index, item) in items.iter().enumerate() {
        let name = item
            .pointer("/function/name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let arguments = item
            .pointer("/function/arguments")
            .cloned()
            .unwrap_or(Value::Null);
        if dst.len() == index {
            dst.push(OllamaToolCall {
                function: OllamaFunctionCall {
                    name,
                    arguments: normalize_arguments(arguments),
                },
            });
            continue;
        }
        if index >= dst.len() {
            continue;
        }
        if !name.is_empty() {
            dst[index].function.name = name;
        }
        dst[index].function.arguments = merge_arguments(dst[index].function.arguments.clone(), arguments);
    }
}

fn normalize_arguments(value: Value) -> Value {
    match value {
        Value::String(text) => serde_json::from_str(&text).unwrap_or(Value::String(text)),
        other => other,
    }
}

fn merge_arguments(existing: Value, incoming: Value) -> Value {
    match (existing, incoming) {
        (current, Value::Null) => current,
        (Value::String(left), Value::String(right)) => {
            let joined = format!("{left}{right}");
            serde_json::from_str(&joined).unwrap_or(Value::String(joined))
        }
        (_, Value::String(text)) => serde_json::from_str(&text).unwrap_or(Value::String(text)),
        (_, other) => other,
    }
}

async fn cancelled(cancel: &CancelFlag) {
    while !cancel.is_cancelled() {
        tokio::time::sleep(Duration::from_millis(60)).await;
    }
}

fn trim_base(base: &str) -> String {
    base.trim().trim_end_matches('/').to_string()
}

fn map_reqwest(error: reqwest::Error) -> AppError {
    crate::logging::log_line("error", &format!("ollama http: {error}"));
    if error.is_connect() || error.is_timeout() || error.is_request() {
        AppError::OllamaConnection
    } else {
        AppError::OllamaConnection
    }
}

fn ollama_error(raw: &str) -> AppError {
    let text = raw.trim();
    let extracted = serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| value.get("error").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| text.to_string());
    let lower = extracted.to_ascii_lowercase();
    if lower.contains("not found") || lower.contains("model") && lower.contains("pull") {
        AppError::Model(extracted)
    } else if lower.contains("connection") || extracted.is_empty() {
        AppError::OllamaConnection
    } else {
        crate::logging::log_line("error", &extracted);
        AppError::message("Ollama could not handle the request. Check that the model is installed and the server is running.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_streamed_arguments() {
        let mut calls = Vec::new();
        absorb_tool_calls(
            &mut calls,
            &json!([{"function": {"name": "read_file", "arguments": "{\"path\":"}}]),
        );
        absorb_tool_calls(
            &mut calls,
            &json!([{"function": {"name": "read_file", "arguments": "\"src/main.ts\"}"}}]),
        );
        assert_eq!(calls[0].function.name, "read_file");
        assert_eq!(calls[0].function.arguments["path"], "src/main.ts");
    }
}
