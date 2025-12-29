pub fn classify_runner_line_for_progress(line: &str) -> Option<String> {
    let s = line.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();

    let prefix = |p: &str| s.starts_with(p).then(|| trim_hint(s));
    classify_json_line_for_progress(s).or_else(|| {
        prefix("START ")
            .or_else(|| prefix("RUNS "))
            .or_else(|| prefix("PASS "))
            .or_else(|| prefix("FAIL "))
            .or_else(|| prefix("Compiling "))
            .or_else(|| prefix("Finished "))
            .or_else(|| prefix("Running "))
            .or_else(|| prefix("test "))
            .or_else(|| prefix("error:"))
            .or_else(|| {
                ["downloading", "installing", "resolving"]
                    .iter()
                    .find(|needle| lower.contains(**needle))
                    .map(|_| trim_hint(s))
            })
            .or_else(|| lower.contains("collecting").then(|| trim_hint(s)))
            .or_else(|| {
                (lower.contains("building") || lower.contains("compiling")).then(|| trim_hint(s))
            })
            .or_else(|| {
                (lower.contains("running") || lower.contains("executing")).then(|| trim_hint(s))
            })
            .or_else(|| {
                (lower.contains("failed") || lower.contains("fail ") || lower.contains("timeout"))
                    .then(|| trim_hint(s))
            })
    })
}

pub(super) fn recent_summary(stdout: Option<String>, stderr: Option<String>) -> String {
    match (stdout, stderr) {
        (None, None) => "no activity yet".to_string(),
        (Some(s), None) => format!("stdout: {s}"),
        (None, Some(e)) => format!("stderr: {e}"),
        (Some(s), Some(e)) => {
            let mut items = [("stdout", s), ("stderr", e)];
            items.sort_by_key(|(_, h)| -hint_score(h));
            format!(
                "{}: {}\n{}: {}",
                items[0].0, items[0].1, items[1].0, items[1].1
            )
        }
    }
}

fn hint_score(hint: &str) -> i32 {
    if hint.starts_with("START ") || hint.starts_with("RUNS ") {
        return 100;
    }
    if hint.starts_with("test ") || hint.starts_with("Running ") {
        return 90;
    }
    if hint.starts_with("FAIL ")
        || hint.starts_with("error:")
        || hint.to_ascii_lowercase().contains("failed")
    {
        return 95;
    }
    if hint.starts_with("Finished ") {
        return 10;
    }
    50
}

fn trim_hint(raw: &str) -> String {
    let compact = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let max_chars: usize = super::frame::terminal_columns()
        .saturating_mul(6)
        .clamp(120, 1200);
    if compact.chars().count() <= max_chars {
        return compact;
    }
    compact.chars().take(max_chars).collect::<String>()
}

fn classify_json_line_for_progress(line: &str) -> Option<String> {
    let s = line.trim();
    if !s.starts_with('{') {
        return None;
    }
    let value = serde_json::from_str::<serde_json::Value>(s).ok()?;
    let obj = value.as_object()?;

    let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let event = obj.get("event").and_then(|v| v.as_str()).unwrap_or("");
    let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("");

    if !event.is_empty() && !name.is_empty() {
        let tag = if ty.is_empty() { "event" } else { ty };
        return Some(trim_hint(&format!("{tag} {event}: {name}")));
    }

    if ty == "suite" && !event.is_empty() {
        let nextest = obj.get("nextest").and_then(|v| v.as_object());
        let crate_name = nextest
            .and_then(|n| n.get("crate"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let test_binary = nextest
            .and_then(|n| n.get("test_binary"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let kind = nextest
            .and_then(|n| n.get("kind"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let label = if !crate_name.is_empty() && !test_binary.is_empty() {
            format!("{crate_name}::{test_binary}")
        } else if !test_binary.is_empty() {
            test_binary.to_string()
        } else {
            String::new()
        };
        let kind_prefix = if kind.is_empty() {
            String::new()
        } else {
            format!("{kind} ")
        };
        if !label.is_empty() {
            return Some(trim_hint(&format!("suite {event}: {kind_prefix}{label}")));
        }
        return Some(trim_hint(&format!("suite {event}")));
    }

    let lower = s.to_ascii_lowercase();
    if lower.contains("failed") || lower.contains("error") || lower.contains("panic") {
        return Some(trim_hint(s));
    }
    None
}
