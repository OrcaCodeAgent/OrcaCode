use serde_json::Value;

use crate::domain::{OllamaToolCall, PlanStep, ToolRequest};

#[derive(Debug, Clone, PartialEq)]
pub enum TurnAction {
    Tools(Vec<ToolRequest>),
    Final(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelTurn {
    pub visible_text: String,
    pub action: TurnAction,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParseOutcome {
    Turn(ModelTurn),
    InvalidProtocol { preview: String },
}

pub fn parse_model_turn(content: &str, native: &[OllamaToolCall]) -> ParseOutcome {
    if !native.is_empty() {
        let tools = native
            .iter()
            .filter(|call| !call.function.name.trim().is_empty())
            .map(|call| ToolRequest {
                name: call.function.name.clone(),
                arguments: normalize_arguments(call.function.arguments.clone()),
            })
            .collect::<Vec<_>>();
        if !tools.is_empty() {
            return ParseOutcome::Turn(ModelTurn {
                visible_text: content.trim().to_string(),
                action: TurnAction::Tools(tools),
            });
        }
    }

    let objects = extract_json_objects(content);
    let mut parsed = Vec::new();
    let mut failed_protocol = false;
    for object in &objects {
        match parse_protocol_value(object) {
            Some(item) => parsed.push(item),
            None if looks_like_protocol(object) => failed_protocol = true,
            None => {}
        }
    }

    if parsed.is_empty() && (failed_protocol || looks_like_protocol(content)) {
        let preview: String = content.chars().take(280).collect();
        return ParseOutcome::InvalidProtocol { preview };
    }

    let mut tools = Vec::new();
    let mut final_text = None;
    for item in parsed {
        match item {
            Protocol::Tool(request) => tools.push(request),
            Protocol::Final(text) => final_text = Some(text),
        }
    }
    if !tools.is_empty() {
        return ParseOutcome::Turn(ModelTurn {
            visible_text: preamble_before_json(content),
            action: TurnAction::Tools(tools),
        });
    }
    if let Some(text) = final_text {
        return ParseOutcome::Turn(ModelTurn {
            visible_text: text.clone(),
            action: TurnAction::Final(text),
        });
    }
    let prose = content.trim().to_string();
    ParseOutcome::Turn(ModelTurn {
        visible_text: prose.clone(),
        action: TurnAction::Final(prose),
    })
}

enum Protocol {
    Tool(ToolRequest),
    Final(String),
}

fn parse_protocol_value(raw: &str) -> Option<Protocol> {
    for candidate in [raw.to_string(), strip_trailing_commas(raw)] {
        let Ok(value) = serde_json::from_str::<Value>(&candidate) else {
            continue;
        };
        if let Some(protocol) = classify_value(&value) {
            return Some(protocol);
        }
    }
    None
}

fn classify_value(value: &Value) -> Option<Protocol> {
    let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
    if kind == "final" {
        let content = value
            .get("content")
            .or_else(|| value.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        return Some(Protocol::Final(content));
    }
    if kind == "plan" {
        let steps = normalize_steps(value.get("steps"));
        return Some(Protocol::Tool(ToolRequest {
            name: "update_plan".into(),
            arguments: serde_json::json!({ "steps": steps }),
        }));
    }
    if kind == "tool_calls" {
        let calls = value.get("calls").and_then(Value::as_array)?;
        let mut combined = Vec::new();
        for call in calls {
            if let Some(Protocol::Tool(request)) = tool_from_value(call) {
                combined.push(request);
            }
        }
        if combined.len() == 1 {
            return Some(Protocol::Tool(combined.remove(0)));
        }
        if !combined.is_empty() {
            return Some(Protocol::Tool(ToolRequest {
                name: "__batch__".into(),
                arguments: serde_json::json!(combined
                    .into_iter()
                    .map(|request| serde_json::json!({"tool": request.name, "arguments": request.arguments}))
                    .collect::<Vec<_>>()),
            }));
        }
    }
    if kind == "tool_call" || value.get("tool").is_some() {
        return tool_from_value(value);
    }
    None
}

fn tool_from_value(value: &Value) -> Option<Protocol> {
    if let Some(calls) = value.get("calls").and_then(Value::as_array) {
        let mut requests = Vec::new();
        for call in calls {
            if let Some(Protocol::Tool(request)) = single_tool(call) {
                requests.push(request);
            }
        }
        if requests.is_empty() {
            return None;
        }
        if requests.len() == 1 {
            return Some(Protocol::Tool(requests.remove(0)));
        }
        return Some(Protocol::Tool(ToolRequest {
            name: "__batch__".into(),
            arguments: Value::Array(
                requests
                    .into_iter()
                    .map(|request| serde_json::json!({"tool": request.name, "arguments": request.arguments}))
                    .collect(),
            ),
        }));
    }
    single_tool(value)
}

fn single_tool(value: &Value) -> Option<Protocol> {
    let name = value
        .get("tool")
        .or_else(|| value.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        return None;
    }
    let arguments = value
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
    Some(Protocol::Tool(ToolRequest {
        name,
        arguments: normalize_arguments(arguments),
    }))
}

pub fn expand_tool_requests(requests: Vec<ToolRequest>) -> Vec<ToolRequest> {
    let mut expanded = Vec::new();
    for request in requests {
        if request.name == "__batch__" {
            if let Some(items) = request.arguments.as_array() {
                for item in items {
                    if let Some(Protocol::Tool(tool)) = single_tool(item) {
                        expanded.push(tool);
                    }
                }
            }
        } else {
            expanded.push(request);
        }
    }
    expanded
}

fn normalize_steps(value: Option<&Value>) -> Vec<PlanStep> {
    let Some(items) = value.and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            if let Some(text) = item.as_str() {
                return Some(
                    PlanStep {
                        id: (index + 1).to_string(),
                        title: text.to_string(),
                        status: "pending".into(),
                    }
                    .normalized(index),
                );
            }
            let step: PlanStep = serde_json::from_value(item.clone()).ok()?;
            let step = step.normalized(index);
            if step.title.is_empty() {
                None
            } else {
                Some(step)
            }
        })
        .collect()
}

fn normalize_arguments(value: Value) -> Value {
    match value {
        Value::String(text) => serde_json::from_str(&text).unwrap_or(Value::String(text)),
        other => other,
    }
}

fn looks_like_protocol(text: &str) -> bool {
    let trimmed = text.trim();
    let fenced = trimmed.starts_with("```") || trimmed.starts_with('{');
    let mentions = trimmed.contains("\"tool\"")
        || trimmed.contains("\"type\"")
        || trimmed.contains("tool_call");
    fenced && mentions
}

fn preamble_before_json(text: &str) -> String {
    if let Some(index) = text.find('{') {
        text[..index].trim().to_string()
    } else {
        String::new()
    }
}

fn extract_json_objects(text: &str) -> Vec<String> {
    let mut objects = Vec::new();
    for fence in extract_fences(text) {
        push_objects(&fence, &mut objects);
    }
    push_objects(text, &mut objects);
    objects
}

fn extract_fences(text: &str) -> Vec<String> {
    let mut fences = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let after = after
            .find('\n')
            .map(|index| &after[index + 1..])
            .unwrap_or(after);
        if let Some(end) = after.find("```") {
            fences.push(after[..end].trim().to_string());
            rest = &after[end + 3..];
        } else {
            break;
        }
    }
    fences
}

fn push_objects(text: &str, into: &mut Vec<String>) {
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '{' {
            if let Some(end) = matching_brace(&chars, index) {
                into.push(chars[index..=end].iter().collect());
                index = end + 1;
                continue;
            }
        }
        index += 1;
    }
}

fn matching_brace(chars: &[char], start: usize) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, ch) in chars.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if *ch == '\\' {
                escaped = true;
            } else if *ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn strip_trailing_commas(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if ch == '"' {
            in_string = true;
            output.push(ch);
            index += 1;
            continue;
        }
        if ch == ',' {
            let mut look = index + 1;
            while look < chars.len() && chars[look].is_whitespace() {
                look += 1;
            }
            if look < chars.len() && (chars[look] == '}' || chars[look] == ']') {
                index += 1;
                continue;
            }
        }
        output.push(ch);
        index += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tool_call_and_final() {
        let tool = parse_model_turn(
            r#"{"type":"tool_call","tool":"read_file","arguments":{"path":"src/main.ts"}}"#,
            &[],
        );
        match tool {
            ParseOutcome::Turn(turn) => match turn.action {
                TurnAction::Tools(tools) => {
                    assert_eq!(tools[0].name, "read_file");
                    assert_eq!(tools[0].arguments["path"], "src/main.ts");
                }
                other => panic!("unexpected {other:?}"),
            },
            other => panic!("unexpected {other:?}"),
        }

        let done = parse_model_turn(r#"{"type":"final","content":"done"}"#, &[]);
        match done {
            ParseOutcome::Turn(turn) => {
                assert_eq!(turn.action, TurnAction::Final("done".into()));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn repairs_trailing_comma_inside_fence() {
        let raw = "```json\n{\"type\":\"tool_call\",\"tool\":\"git_status\",\"arguments\":{},}\n```";
        let parsed = parse_model_turn(raw, &[]);
        match parsed {
            ParseOutcome::Turn(turn) => match turn.action {
                TurnAction::Tools(tools) => assert_eq!(tools[0].name, "git_status"),
                other => panic!("unexpected {other:?}"),
            },
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn prefers_native_tool_calls() {
        let native = vec![OllamaToolCall {
            function: crate::domain::OllamaFunctionCall {
                name: "list_directory".into(),
                arguments: serde_json::json!({"path": "."}),
            },
        }];
        let parsed = parse_model_turn("I'll look around.", &native);
        match parsed {
            ParseOutcome::Turn(turn) => {
                assert_eq!(turn.visible_text, "I'll look around.");
                match turn.action {
                    TurnAction::Tools(tools) => assert_eq!(tools[0].name, "list_directory"),
                    other => panic!("unexpected {other:?}"),
                }
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn broken_protocol_requests_repair() {
        let parsed = parse_model_turn("{\"type\":\"tool_call\",\"tool\":", &[]);
        assert!(matches!(parsed, ParseOutcome::InvalidProtocol { .. }));
    }

    #[test]
    fn prose_is_final() {
        let parsed = parse_model_turn("The project has a build script.", &[]);
        match parsed {
            ParseOutcome::Turn(turn) => {
                assert!(matches!(turn.action, TurnAction::Final(_)));
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
