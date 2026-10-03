pub fn truncate_observation(input: &str, limit: usize) -> String {
    if limit == 0 {
        return String::new();
    }
    let count = input.chars().count();
    if count <= limit {
        return input.to_string();
    }
    let tail_len = (limit * 4 / 5).max(1).min(limit);
    let head_budget = limit.saturating_sub(tail_len);
    let errors: String = input
        .lines()
        .filter(|line| is_error_line(line))
        .take(40)
        .collect::<Vec<_>>()
        .join("\n");
    let errors: String = errors.chars().take(head_budget).collect();
    let tail: String = input
        .chars()
        .rev()
        .take(tail_len)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("[Output was shortened. Original length: {count} characters]\n--- Lines that look like errors ---\n{errors}\n--- End ---\n{tail}")
}

fn is_error_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "error",
        "failed",
        "failure",
        "panic",
        "exception",
        "traceback",
        "undefined",
        "cannot find",
        "not found",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_short_text() {
        assert_eq!(truncate_observation("ok", 10), "ok");
    }

    #[test]
    fn keeps_error_line_and_tail() {
        let mut input = "error: boom\n".to_string();
        input.push_str(&"x".repeat(500));
        input.push_str("TAIL");
        let output = truncate_observation(&input, 80);
        assert!(output.contains("error: boom"));
        assert!(output.contains("TAIL"));
        assert!(output.contains("Original"));
    }
}
