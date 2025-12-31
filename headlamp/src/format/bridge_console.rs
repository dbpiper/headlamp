use crate::format::console::ConsoleEntry;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct HttpEvent {
    pub timestamp_ms: u64,
    pub kind: Option<String>,
    pub method: Option<String>,
    pub url: Option<String>,
    pub route: Option<String>,
    pub status_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub content_type: Option<String>,
    pub request_id: Option<String>,
    pub json: Option<serde_json::Value>,
    pub body_preview: Option<String>,
    pub test_path: Option<String>,
    pub current_test_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AssertionEvt {
    pub timestamp_ms: Option<u64>,
    pub matcher: Option<String>,
    pub expected_number: Option<i64>,
    pub received_number: Option<i64>,
    pub message: Option<String>,
    pub stack: Option<String>,
    pub test_path: Option<String>,
    pub current_test_name: Option<String>,
    pub expected_preview: Option<String>,
    pub actual_preview: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct BridgeEventMeta {
    #[serde(rename = "type")]
    type_name: Option<String>,
    #[serde(rename = "timestampMs")]
    timestamp_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpResponseBridgeEvent {
    method: Option<String>,
    url: Option<String>,
    route: Option<String>,
    status_code: Option<i64>,
    duration_ms: Option<i64>,
    content_type: Option<String>,
    request_id: Option<String>,
    json: Option<serde_json::Value>,
    body_preview: Option<String>,
    test_path: Option<String>,
    current_test_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpAbortBridgeEvent {
    method: Option<String>,
    url: Option<String>,
    route: Option<String>,
    duration_ms: Option<i64>,
    test_path: Option<String>,
    current_test_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssertionFailureBridgeEvent {
    timestamp_ms: Option<u64>,
    matcher: Option<String>,
    expected_number: Option<i64>,
    received_number: Option<i64>,
    message: Option<String>,
    stack: Option<String>,
    test_path: Option<String>,
    current_test_name: Option<String>,
    expected_preview: Option<String>,
    actual_preview: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConsoleBridgeEvent {
    level: Option<String>,
    message: Option<String>,
    test_path: Option<String>,
    current_test_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConsoleBatchBridgeEntry {
    #[serde(rename = "type")]
    type_name: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConsoleBatchBridgeEvent {
    entries: Option<Vec<ConsoleBatchBridgeEntry>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpResponseBatchBridgeEvent {
    events: Option<Vec<HttpResponseBridgeEvent>>,
    test_path: Option<String>,
    current_test_name: Option<String>,
}

pub fn parse_bridge_console(
    console_entries: Option<&Vec<crate::format::bridge::BridgeConsoleEntry>>,
) -> (Vec<HttpEvent>, Vec<AssertionEvt>, Vec<ConsoleEntry>) {
    let mut http: Vec<HttpEvent> = vec![];
    let mut assertions: Vec<AssertionEvt> = vec![];
    let mut console_list: Vec<ConsoleEntry> = vec![];

    let Some(entries) = console_entries else {
        return (http, assertions, console_list);
    };

    for entry in entries {
        let raw = render_console_message(entry.message.as_ref());

        if !raw.contains("[JEST-BRIDGE-EVENT]") {
            push_plain_console_entry(&mut console_list, entry, raw);
            continue;
        }

        let Some((event_type, timestamp_ms, json_text)) = parse_bridge_event(&raw) else {
            continue;
        };
        dispatch_bridge_event(
            &event_type,
            timestamp_ms,
            json_text,
            &mut http,
            &mut assertions,
            &mut console_list,
        );
    }

    (http, assertions, console_list)
}

fn push_plain_console_entry(
    console_list: &mut Vec<ConsoleEntry>,
    entry: &crate::format::bridge::BridgeConsoleEntry,
    raw: String,
) {
    console_list.push(ConsoleEntry {
        type_name: entry.type_name.clone(),
        message: Some(raw),
        origin: entry.origin.clone(),
        test_path: None,
        current_test_name: None,
    });
}

fn parse_bridge_event(raw: &str) -> Option<(String, u64, &str)> {
    let json_text = raw.split("[JEST-BRIDGE-EVENT]").last().unwrap_or("").trim();
    let meta = crate::config::jsonish::parse_jsonish::<BridgeEventMeta>(json_text).ok()?;
    let event_type = meta.type_name.as_deref().unwrap_or("").to_string();
    let timestamp_ms = meta.timestamp_ms.unwrap_or_else(now_timestamp_ms);
    Some((event_type, timestamp_ms, json_text))
}

fn now_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn dispatch_bridge_event(
    event_type: &str,
    timestamp_ms: u64,
    json_text: &str,
    http: &mut Vec<HttpEvent>,
    assertions: &mut Vec<AssertionEvt>,
    console_list: &mut Vec<ConsoleEntry>,
) {
    match event_type {
        "httpResponse" => push_http_response(http, timestamp_ms, json_text),
        "httpResponseBatch" => push_http_response_batch(http, timestamp_ms, json_text),
        "httpAbort" => push_http_abort(http, timestamp_ms, json_text),
        "assertionFailure" => push_assertion_failure(assertions, json_text),
        "console" => push_console_entry(console_list, json_text),
        "consoleBatch" => push_console_batch_entries(console_list, json_text),
        _ => {}
    }
}

fn push_http_response(http: &mut Vec<HttpEvent>, timestamp_ms: u64, json_text: &str) {
    let Ok(evt) = crate::config::jsonish::parse_jsonish::<HttpResponseBridgeEvent>(json_text)
    else {
        return;
    };
    http.push(http_event_from_response(
        timestamp_ms,
        Some("response"),
        evt,
    ));
}

fn push_http_response_batch(http: &mut Vec<HttpEvent>, timestamp_ms: u64, json_text: &str) {
    let Ok(evt) = crate::config::jsonish::parse_jsonish::<HttpResponseBatchBridgeEvent>(json_text)
    else {
        return;
    };
    let test_path = evt.test_path;
    let current_test_name = evt.current_test_name;
    evt.events.unwrap_or_default().into_iter().for_each(|item| {
        http.push(HttpEvent {
            timestamp_ms,
            kind: Some("response".to_string()),
            method: item.method,
            url: item.url,
            route: item.route,
            status_code: item.status_code,
            duration_ms: item.duration_ms,
            content_type: item.content_type,
            request_id: item.request_id,
            json: item.json,
            body_preview: item.body_preview,
            test_path: test_path.clone(),
            current_test_name: current_test_name.clone(),
        });
    });
}

fn push_http_abort(http: &mut Vec<HttpEvent>, timestamp_ms: u64, json_text: &str) {
    let Ok(evt) = crate::config::jsonish::parse_jsonish::<HttpAbortBridgeEvent>(json_text) else {
        return;
    };
    http.push(HttpEvent {
        timestamp_ms,
        kind: Some("abort".to_string()),
        method: evt.method,
        url: evt.url,
        route: evt.route,
        status_code: None,
        duration_ms: evt.duration_ms,
        content_type: None,
        request_id: None,
        json: None,
        body_preview: None,
        test_path: evt.test_path,
        current_test_name: evt.current_test_name,
    });
}

fn http_event_from_response(
    timestamp_ms: u64,
    kind: Option<&'static str>,
    evt: HttpResponseBridgeEvent,
) -> HttpEvent {
    HttpEvent {
        timestamp_ms,
        kind: kind.map(|k| k.to_string()),
        method: evt.method,
        url: evt.url,
        route: evt.route,
        status_code: evt.status_code,
        duration_ms: evt.duration_ms,
        content_type: evt.content_type,
        request_id: evt.request_id,
        json: evt.json,
        body_preview: evt.body_preview,
        test_path: evt.test_path,
        current_test_name: evt.current_test_name,
    }
}

fn push_assertion_failure(assertions: &mut Vec<AssertionEvt>, json_text: &str) {
    let Ok(evt) = crate::config::jsonish::parse_jsonish::<AssertionFailureBridgeEvent>(json_text)
    else {
        return;
    };
    assertions.push(AssertionEvt {
        timestamp_ms: evt.timestamp_ms,
        matcher: evt.matcher,
        expected_number: evt.expected_number,
        received_number: evt.received_number,
        message: evt.message,
        stack: evt.stack,
        test_path: evt.test_path,
        current_test_name: evt.current_test_name,
        expected_preview: evt.expected_preview,
        actual_preview: evt.actual_preview,
    });
}

fn push_console_entry(console_list: &mut Vec<ConsoleEntry>, json_text: &str) {
    let Ok(evt) = crate::config::jsonish::parse_jsonish::<ConsoleBridgeEvent>(json_text) else {
        return;
    };
    console_list.push(ConsoleEntry {
        type_name: evt.level,
        message: evt.message,
        origin: None,
        test_path: evt.test_path,
        current_test_name: evt.current_test_name,
    });
}

fn push_console_batch_entries(console_list: &mut Vec<ConsoleEntry>, json_text: &str) {
    let Ok(evt) = crate::config::jsonish::parse_jsonish::<ConsoleBatchBridgeEvent>(json_text)
    else {
        return;
    };
    evt.entries.unwrap_or_default().into_iter().for_each(|e| {
        console_list.push(ConsoleEntry {
            type_name: e.type_name,
            message: e.message,
            origin: None,
            test_path: None,
            current_test_name: None,
        });
    });
}

fn render_console_message(message_value: Option<&serde_json::Value>) -> String {
    match message_value {
        None => String::new(),
        Some(serde_json::Value::Array(values)) => values
            .iter()
            .map(serde_json::Value::to_string)
            .collect::<Vec<_>>()
            .join(" "),
        Some(serde_json::Value::String(s)) => s.to_string(),
        Some(other) => other.to_string(),
    }
}
