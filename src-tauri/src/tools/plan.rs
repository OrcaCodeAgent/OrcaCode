use serde_json::Value;

use crate::domain::{PlanStep, ToolOutput};

pub fn update_plan(args: &Value) -> ToolOutput {
    let Some(items) = args.get("steps").and_then(Value::as_array) else {
        return ToolOutput::fail("A steps array is required.");
    };
    let steps: Vec<PlanStep> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            if let Some(title) = item.as_str() {
                return Some(
                    PlanStep {
                        id: (index + 1).to_string(),
                        title: title.to_string(),
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
        .collect();
    if steps.is_empty() {
        return ToolOutput::fail("The plan has no steps.");
    }
    let summary = steps
        .iter()
        .map(|step| format!("{} [{}] {}", step.id, step.status, step.title))
        .collect::<Vec<_>>()
        .join("\n");
    let mut output = ToolOutput::ok(summary);
    output.plan = Some(steps);
    output
}
