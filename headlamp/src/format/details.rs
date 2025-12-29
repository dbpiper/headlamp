use indexmap::IndexSet;
use serde_json::Value;

pub type DetailLines = (Vec<String>, Vec<String>);

pub fn condense_blank_runs(lines: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut last_blank = false;
    for ln in lines {
        let is_blank = ln.trim().is_empty();
        if is_blank {
            if !last_blank {
                out.push(String::new());
            }
            last_blank = true;
        } else {
            out.push(ln.to_string());
            last_blank = false;
        }
    }
    out
}

pub fn merge_msg_lines(primary_raw: &str, detail_msgs: &[String]) -> Vec<String> {
    let primary = if primary_raw.trim().is_empty() {
        vec![]
    } else {
        primary_raw
            .lines()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    };

    let mut seen = primary
        .iter()
        .map(|line| line.trim())
        .filter(|k| !k.is_empty())
        .map(str::to_owned)
        .collect::<IndexSet<String>>();

    let merged = primary
        .into_iter()
        .chain(detail_msgs.iter().filter_map(|msg| {
            let msg_key = msg.trim();
            if msg_key.is_empty() || !seen.insert(msg_key.to_string()) {
                None
            } else {
                Some(msg.to_string())
            }
        }))
        .collect::<Vec<_>>();

    condense_blank_runs(&merged)
}

pub fn lines_from_details(details: Option<&Vec<Value>>) -> DetailLines {
    let mut stacks: Vec<String> = vec![];
    let mut messages: Vec<String> = vec![];

    let Some(details) = details else {
        return (stacks, messages);
    };

    for detail in details {
        collect_detail_lines(detail, &mut stacks, &mut messages);
    }

    (stacks, messages)
}

fn push_lines(value: &Value, bucket: &mut Vec<String>) {
    value
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .into_iter()
        .flat_map(|s| s.lines())
        .for_each(|line| bucket.push(line.to_string()));
}

fn collect_detail_lines(detail: &Value, stacks: &mut Vec<String>, messages: &mut Vec<String>) {
    match detail {
        Value::String(s) => push_string_detail(s, stacks, messages),
        Value::Object(obj) => collect_object_detail(obj, stacks, messages),
        Value::Array(arr) => arr.iter().for_each(|v| visit_deep(v, 0, stacks, messages)),
        _ => {}
    }
}

fn push_string_detail(text: &str, stacks: &mut Vec<String>, messages: &mut Vec<String>) {
    let target = if text.contains(" at ") && text.contains(':') {
        stacks
    } else {
        messages
    };
    target.extend(text.lines().map(|l| l.to_string()));
}

fn collect_object_detail(
    obj: &serde_json::Map<String, Value>,
    stacks: &mut Vec<String>,
    messages: &mut Vec<String>,
) {
    obj.get("stack")
        .into_iter()
        .for_each(|v| push_lines(v, stacks));
    obj.get("message")
        .into_iter()
        .for_each(|v| push_lines(v, messages));
    obj.get("error")
        .and_then(Value::as_object)
        .into_iter()
        .for_each(|err_obj| {
            err_obj
                .get("stack")
                .into_iter()
                .for_each(|v| push_lines(v, stacks));
            err_obj
                .get("message")
                .into_iter()
                .for_each(|v| push_lines(v, messages));
        });
    obj.get("matcherResult")
        .and_then(Value::as_object)
        .into_iter()
        .for_each(|mr_obj| {
            ["stack", "message", "expected", "received"]
                .into_iter()
                .filter_map(|k| mr_obj.get(k))
                .for_each(|v| push_lines(v, messages));
        });
    visit_deep(&Value::Object(obj.clone()), 0, stacks, messages);
}

fn visit_deep(value: &Value, depth: usize, stacks: &mut Vec<String>, messages: &mut Vec<String>) {
    if depth > 3 {
        return;
    }
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
        Value::String(_) => push_lines(value, messages),
        Value::Array(arr) => arr
            .iter()
            .for_each(|v| visit_deep(v, depth + 1, stacks, messages)),
        Value::Object(obj) => {
            obj.get("message")
                .into_iter()
                .for_each(|v| push_lines(v, messages));
            obj.get("stack")
                .into_iter()
                .for_each(|v| push_lines(v, stacks));
            ["expected", "received"]
                .into_iter()
                .filter_map(|k| obj.get(k))
                .for_each(|v| push_lines(v, messages));
            [
                "errors",
                "details",
                "issues",
                "inner",
                "causes",
                "aggregatedErrors",
            ]
            .into_iter()
            .filter_map(|k| obj.get(k).and_then(Value::as_array))
            .for_each(|arr| {
                arr.iter()
                    .for_each(|v| visit_deep(v, depth + 1, stacks, messages))
            });
            ["error", "cause", "matcherResult", "context", "data"]
                .into_iter()
                .filter_map(|k| obj.get(k))
                .for_each(|v| visit_deep(v, depth + 1, stacks, messages));
        }
    }
}
