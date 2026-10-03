use crate::domain::ChatMessage;
use crate::safety::truncate::truncate_observation;

pub fn fit_messages(messages: &[ChatMessage], max_chars: usize) -> Vec<ChatMessage> {
    if max_chars == 0 || messages.is_empty() {
        return Vec::new();
    }
    let total: usize = messages.iter().map(message_len).sum();
    if total <= max_chars {
        return messages.to_vec();
    }
    if messages.len() <= 2 {
        return messages
            .iter()
            .map(|message| shrink(message, max_chars / messages.len().max(1)))
            .collect();
    }
    let mut tail_start = 2;
    let mut chosen = messages.len();
    while tail_start < messages.len() {
        let estimate = estimate_range(messages, 0, 2) + estimate_range(messages, tail_start, messages.len()) + 80;
        if estimate <= max_chars {
            chosen = tail_start;
            break;
        }
        tail_start += 1;
    }
    let mut fitted = Vec::new();
    fitted.extend(messages.iter().take(2).cloned());
    if chosen > 2 && chosen < messages.len() {
        fitted.push(ChatMessage::user(
            "[Some earlier tool results were omitted because of the context limit.]",
        ));
    }
    if chosen < messages.len() {
        fitted.extend(messages.iter().skip(chosen).cloned());
    } else if chosen >= messages.len() {
        fitted.truncate(2);
        if let Some(last) = messages.last() {
            if messages.len() > 2 {
                fitted.push(shrink(last, max_chars / 2));
            }
        }
    }
    let mut guard = 0;
    while fitted.iter().map(message_len).sum::<usize>() > max_chars && guard < fitted.len() {
        let index = fitted.len().saturating_sub(1).saturating_sub(guard % 3);
        if index < 2 && fitted.len() > 2 {
            guard += 1;
            continue;
        }
        let budget = (max_chars / fitted.len().max(1)).max(200);
        fitted[index] = shrink(&fitted[index], budget);
        guard += 1;
        if guard > 12 {
            break;
        }
    }
    fitted
}

fn estimate_range(messages: &[ChatMessage], start: usize, end: usize) -> usize {
    messages.iter().skip(start).take(end.saturating_sub(start)).map(message_len).sum()
}

fn message_len(message: &ChatMessage) -> usize {
    estimate_tokens(&message.content) + message.tool_name.as_deref().map(estimate_tokens).unwrap_or(0)
}

fn estimate_tokens(text: &str) -> usize {
    let units: usize = text.chars().map(|ch| if ch.is_ascii() { 1 } else { 8 }).sum();
    let tokens = units.div_ceil(4);
    if text.is_empty() { 0 } else { tokens.max(1) }
}

fn shrink(message: &ChatMessage, budget: usize) -> ChatMessage {
    let mut cloned = message.clone();
    cloned.content = truncate_observation(&cloned.content, budget.saturating_mul(4).max(80));
    if budget < 120 {
        cloned.images = None;
    }
    cloned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_middle_tool_results_before_the_latest_turn() {
        let mut messages = vec![
            ChatMessage::system("system prompt"),
            ChatMessage::user("goal"),
        ];
        for index in 0..8 {
            messages.push(ChatMessage::user(format!("tool result {index} {}", "x".repeat(400))));
        }
        messages.push(ChatMessage::assistant("latest"));
        let fitted = fit_messages(&messages, 220);
        let blob = fitted.iter().map(|message| message.content.clone()).collect::<Vec<_>>().join("\n");
        assert!(blob.contains("system prompt"));
        assert!(blob.contains("goal"));
        assert!(blob.contains("latest"));
        assert!(blob.contains("omitted"));
        assert!(!blob.contains("tool result 0"));
    }
}
