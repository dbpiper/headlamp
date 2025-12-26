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

    let push_maybe = |value: &Value, bucket: &mut Vec<String>| {
        if let Some(s) = value.as_str()
            && !s.trim().is_empty()
        {
            bucket.extend(s.lines().map(|l| l.to_string()));
        }
    };

    fn visit_deep(
        value: &Value,
        depth: usize,
        stacks: &mut Vec<String>,
        messages: &mut Vec<String>,
    ) {
        if depth > 3 {
            return;
        }
        match value {
            Value::Null => {}
            Value::Bool(_) | Value::Number(_) => {}
            Value::String(s) => {
                if !s.trim().is_empty() {
                    messages.extend(s.lines().map(|l| l.to_string()));
                }
            }
            Value::Array(arr) => {
                arr.iter()
                    .for_each(|v| visit_deep(v, depth + 1, stacks, messages));
            }
            Value::Object(obj) => {
                obj.get("message")
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
                    .into_iter()
                    .for_each(|s| {
                        messages.extend(s.lines().map(|l| l.to_string()));
                    });
                obj.get("stack")
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
                    .into_iter()
                    .for_each(|s| {
                        stacks.extend(s.lines().map(|l| l.to_string()));
                    });
                ["expected", "received"].into_iter().for_each(|k| {
                    obj.get(k)
                        .and_then(Value::as_str)
                        .filter(|s| !s.trim().is_empty())
                        .into_iter()
                        .for_each(|s| {
                            messages.extend(s.lines().map(|l| l.to_string()));
                        });
                });

                [
                    "errors",
                    "details",
                    "issues",
                    "inner",
                    "causes",
                    "aggregatedErrors",
                ]
                .into_iter()
                .filter_map(|k| obj.get(k))
                .filter_map(|v| v.as_array())
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

    for detail in details {
        match detail {
            Value::String(s) => {
                if s.contains(" at ") && s.contains(':') {
                    stacks.extend(s.lines().map(|l| l.to_string()));
                } else {
                    messages.extend(s.lines().map(|l| l.to_string()));
                }
            }
            Value::Object(obj) => {
                obj.get("stack")
                    .into_iter()
                    .for_each(|v| push_maybe(v, &mut stacks));
                obj.get("message")
                    .into_iter()
                    .for_each(|v| push_maybe(v, &mut messages));
                if let Some(err) = obj.get("error")
                    && let Value::Object(err_obj) = err
                {
                    err_obj
                        .get("stack")
                        .into_iter()
                        .for_each(|v| push_maybe(v, &mut stacks));
                    err_obj
                        .get("message")
                        .into_iter()
                        .for_each(|v| push_maybe(v, &mut messages));
                };
                if let Some(mr) = obj.get("matcherResult")
                    && let Value::Object(mr_obj) = mr
                {
                    ["stack", "message", "expected", "received"]
                        .into_iter()
                        .filter_map(|k| mr_obj.get(k))
                        .for_each(|v| push_maybe(v, &mut messages));
                };
                visit_deep(detail, 0, &mut stacks, &mut messages);
            }
            Value::Array(arr) => {
                arr.iter()
                    .for_each(|v| visit_deep(v, 0, &mut stacks, &mut messages));
            }
            _ => {}
        }
    }

    (stacks, messages)
}
