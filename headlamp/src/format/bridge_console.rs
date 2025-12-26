use crate::format::console::ConsoleEntry;
use json5;
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
            console_list.push(ConsoleEntry {
                type_name: entry.type_name.clone(),
                message: Some(raw),
                origin: entry.origin.clone(),
                test_path: None,
                current_test_name: None,
            });
            continue;
        }

        let json_text = raw.split("[JEST-BRIDGE-EVENT]").last().unwrap_or("").trim();
        let Ok(meta) = json5::from_str::<BridgeEventMeta>(json_text) else {
            continue;
        };
        let t = meta.type_name.as_deref().unwrap_or("");

        let timestamp_ms = meta.timestamp_ms.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0)
        });

        match t {
            "httpResponse" => {
                let Ok(evt) = json5::from_str::<HttpResponseBridgeEvent>(json_text) else {
                    continue;
                };
                http.push(HttpEvent {
                    timestamp_ms,
                    kind: Some("response".to_string()),
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
                });
            }
            "httpResponseBatch" => {
                let Ok(evt) = json5::from_str::<HttpResponseBatchBridgeEvent>(json_text) else {
                    continue;
                };
                let test_path = evt.test_path;
                let current_test_name = evt.current_test_name;
                for item in evt.events.unwrap_or_default() {
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
                }
            }
            "httpAbort" => {
                let Ok(evt) = json5::from_str::<HttpAbortBridgeEvent>(json_text) else {
                    continue;
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
            "assertionFailure" => {
                let Ok(evt) = json5::from_str::<AssertionFailureBridgeEvent>(json_text) else {
                    continue;
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
            "console" => {
                let Ok(evt) = json5::from_str::<ConsoleBridgeEvent>(json_text) else {
                    continue;
                };
                console_list.push(ConsoleEntry {
                    type_name: evt.level,
                    message: evt.message,
                    origin: None,
                    test_path: evt.test_path,
                    current_test_name: evt.current_test_name,
                });
            }
            "consoleBatch" => {
                let Ok(evt) = json5::from_str::<ConsoleBatchBridgeEvent>(json_text) else {
                    continue;
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
            _ => {}
        }
    }

    (http, assertions, console_list)
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
