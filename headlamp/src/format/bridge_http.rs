use crate::format::ansi;
use crate::format::bridge_console::{AssertionEvt, HttpEvent};
use crate::format::time::format_duration;

const METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

pub fn render_http_card(
    rel_path: &str,
    assertion_full_name: &str,
    assertion_title: &str,
    assertion_failure_text: &str,
    file_test_path_abs: &str,
    assertion_events: &[AssertionEvt],
    http_sorted: &[HttpEvent],
) -> Vec<String> {
    let per_test_http = http_in_same_test(http_sorted, file_test_path_abs, assertion_full_name);
    let corresponding = find_corresponding_assertion_event(
        assertion_events,
        file_test_path_abs,
        assertion_full_name,
        assertion_title,
    );

    let transport = is_transport_card(&corresponding, &per_test_http);
    if !is_http_relevant(
        rel_path,
        assertion_full_name,
        &corresponding,
        per_test_http.len(),
        transport,
    ) {
        return vec![];
    }
    if transport {
        return render_transport_http_card(&corresponding, &per_test_http);
    }

    let corr = infer_missing_http_numbers(&corresponding, assertion_failure_text);
    let Some(relevant) = pick_relevant_http(
        &corr,
        http_sorted,
        file_test_path_abs,
        assertion_full_name,
        assertion_full_name,
    ) else {
        return vec![];
    };
    render_status_http_card(&relevant, &corr)
}

fn find_corresponding_assertion_event(
    assertion_events: &[AssertionEvt],
    file_test_path_abs: &str,
    assertion_full_name: &str,
    assertion_title: &str,
) -> AssertionEvt {
    assertion_events
        .iter()
        .find(|evt| {
            same_test_ctx(
                evt.test_path.as_deref(),
                evt.current_test_name.as_deref(),
                file_test_path_abs,
                assertion_full_name,
            )
        })
        .cloned()
        .unwrap_or_else(|| AssertionEvt {
            timestamp_ms: None,
            matcher: None,
            expected_number: None,
            received_number: None,
            message: None,
            stack: None,
            test_path: Some(file_test_path_abs.to_string()),
            current_test_name: Some(assertion_title.to_string()),
            expected_preview: None,
            actual_preview: None,
        })
}

fn is_transport_card(corresponding: &AssertionEvt, per_test_http: &[HttpEvent]) -> bool {
    let has_abort = per_test_http
        .iter()
        .any(|evt| evt.kind.as_deref() == Some("abort"));
    is_transport_error(corresponding.message.as_deref()) || has_abort
}

fn infer_missing_http_numbers(
    corresponding: &AssertionEvt,
    assertion_failure_text: &str,
) -> AssertionEvt {
    let mut corr = corresponding.clone();
    if !is_http_status_number(corr.expected_number)
        && !is_http_status_number(corr.received_number)
        && let Some((expected_number, received_number)) =
            infer_http_numbers_from_text(assertion_failure_text)
    {
        corr.expected_number = expected_number.or(corr.expected_number);
        corr.received_number = received_number.or(corr.received_number);
    };
    corr
}

fn render_status_http_card(relevant: &HttpEvent, corr: &AssertionEvt) -> Vec<String> {
    vec![
        render_http_header_and_expectations(relevant, corr),
        String::from("\n"),
    ]
}

fn render_transport_http_card(
    corresponding: &AssertionEvt,
    per_test_http: &[HttpEvent],
) -> Vec<String> {
    let ts_base = corresponding.timestamp_ms.unwrap_or(0) as i64;
    let nearest_abort = per_test_http
        .iter()
        .filter(|evt| evt.kind.as_deref() == Some("abort"))
        .min_by_key(|evt| ((evt.timestamp_ms as i64) - ts_base).abs());
    let Some(nearest_abort) = nearest_abort else {
        return vec![];
    };
    let where_text = summarize_url(
        nearest_abort.method.as_deref(),
        nearest_abort.url.as_deref(),
        nearest_abort.route.as_deref(),
    );
    let duration = nearest_abort
        .duration_ms
        .and_then(|n| u64::try_from(n).ok())
        .map(|ms| format_duration(std::time::Duration::from_millis(ms)))
        .map(|formatted| format!(" {}", ansi::dim(&format!("({formatted})"))))
        .unwrap_or_default();
    let header = format!(
        "  HTTP:\n    {where_text} {} {}{duration} \n",
        ansi::dim("->"),
        ansi::yellow("connection aborted")
    );
    vec![header]
}

fn render_http_header_and_expectations(relevant: &HttpEvent, corr: &AssertionEvt) -> String {
    let where_text = summarize_url(
        relevant.method.as_deref(),
        relevant.url.as_deref(),
        relevant.route.as_deref(),
    );
    let status = relevant
        .status_code
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".to_string());
    let duration = relevant
        .duration_ms
        .and_then(|n| u64::try_from(n).ok())
        .map(|ms| format_duration(std::time::Duration::from_millis(ms)))
        .map(|formatted| format!(" {} ", ansi::dim(&format!("({formatted})"))))
        .unwrap_or_else(|| " ".to_string());
    let content_type = relevant
        .content_type
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| ansi::dim(&format!("({s})")))
        .unwrap_or_default();
    let request_id = relevant
        .request_id
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| ansi::dim(&format!("  reqId={s}")))
        .unwrap_or_default();

    let header = format!(
        "  HTTP:\n    {where_text} {} {status}{duration}{content_type}{request_id}",
        ansi::dim("->")
    );
    let exp_vs_act = match (corr.expected_number, corr.received_number) {
        (Some(expected), Some(received)) => Some(format!(
            "\n      Expected: {}   Received: {}",
            ansi::yellow(&expected.to_string()),
            ansi::yellow(&received.to_string())
        )),
        (Some(expected), None) => Some(format!(
            "\n      Expected: {}   Received: {}",
            ansi::yellow(&expected.to_string()),
            ansi::yellow(&status)
        )),
        _ => None,
    };
    [header, exp_vs_act.unwrap_or_default()].join("")
}

fn http_in_same_test(
    http_sorted: &[HttpEvent],
    test_path: &str,
    assertion_full_name: &str,
) -> Vec<HttpEvent> {
    http_sorted
        .iter()
        .filter(|evt| {
            same_test_ctx(
                evt.test_path.as_deref(),
                evt.current_test_name.as_deref(),
                test_path,
                assertion_full_name,
            )
        })
        .cloned()
        .collect()
}

fn same_test_ctx(
    left_test_path: Option<&str>,
    left_name: Option<&str>,
    right_test_path: &str,
    right_name: &str,
) -> bool {
    left_test_path == Some(right_test_path) && name_matches(left_name, Some(right_name))
}

fn name_matches(left: Option<&str>, right: Option<&str>) -> bool {
    let Some(left) = left else { return false };
    let Some(right) = right else { return false };
    left == right || left.contains(right) || right.contains(left)
}

fn pick_relevant_http(
    assertion: &AssertionEvt,
    http_sorted: &[HttpEvent],
    ctx_test_path: &str,
    ctx_current_test_name: &str,
    title: &str,
) -> Option<HttpEvent> {
    let (method_hint, path_hint) = parse_method_path_from_title(title);
    let mut pool = http_sorted
        .iter()
        .filter(|evt| {
            same_test_ctx(
                evt.test_path.as_deref(),
                evt.current_test_name.as_deref(),
                ctx_test_path,
                ctx_current_test_name,
            ) || same_test_ctx(
                assertion.test_path.as_deref(),
                assertion.current_test_name.as_deref(),
                ctx_test_path,
                ctx_current_test_name,
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    if pool.is_empty() {
        pool = http_sorted
            .iter()
            .filter(|evt| evt.test_path.as_deref() == Some(ctx_test_path))
            .cloned()
            .collect();
    }
    if pool.is_empty() {
        return None;
    }
    pool.into_iter()
        .map(|evt| {
            let score = score_http_for_assertion(assertion, &method_hint, &path_hint, &evt);
            (evt, score)
        })
        .max_by_key(|(_evt, score)| *score)
        .and_then(|(evt, score)| (score >= 1200).then_some(evt))
}

fn score_http_for_assertion(
    assertion: &AssertionEvt,
    method_hint: &Option<String>,
    path_hint: &Option<String>,
    candidate: &HttpEvent,
) -> i64 {
    let status_score = match (
        assertion.received_number,
        assertion.expected_number,
        candidate.status_code,
    ) {
        (Some(received), _, Some(status)) if received == status => 1500,
        (_, Some(expected), Some(status)) if expected == status => 1200,
        (_, _, Some(status)) if status >= 400 => 800,
        _ => 0,
    };
    let route_score = route_similarity_score(
        method_hint.as_deref(),
        path_hint.as_deref(),
        candidate.method.as_deref(),
        candidate.route.as_deref().or(candidate.url.as_deref()),
    );
    let specificity = if candidate.route.as_deref().unwrap_or("").trim().is_empty() {
        if candidate.url.as_deref().unwrap_or("").trim().is_empty() {
            0
        } else {
            40
        }
    } else {
        80
    };
    status_score + route_score + specificity
}

fn route_similarity_score(
    hint_method: Option<&str>,
    hint_path: Option<&str>,
    event_method: Option<&str>,
    event_path: Option<&str>,
) -> i64 {
    if hint_method.is_none() && hint_path.is_none() {
        return 0;
    }
    let method_ok = match (hint_method, event_method) {
        (Some(h), Some(e)) if h == e => 1,
        _ => 0,
    };
    let Some(route) = event_path else {
        return (method_ok * 10) as i64;
    };
    let hint_path = hint_path.unwrap_or("");
    if !hint_path.is_empty() && route == hint_path {
        return (500 + method_ok * 50) as i64;
    }
    if !hint_path.is_empty() && route.ends_with(hint_path) {
        return (300 + method_ok * 50) as i64;
    }
    if !hint_path.is_empty() && route.contains(hint_path) {
        return (200 + method_ok * 50) as i64;
    }
    (method_ok * 10) as i64
}

fn parse_method_path_from_title(title: &str) -> (Option<String>, Option<String>) {
    let mut words = title.split_whitespace();
    let first = words.next().unwrap_or("");
    let second = words.next().unwrap_or("");
    let method = METHODS
        .iter()
        .find(|m| first.eq_ignore_ascii_case(m))
        .map(|s| s.to_string());
    let path = second.starts_with('/').then(|| second.to_string());
    (method, path)
}

fn title_suggests_http(title: &str) -> bool {
    let (method, path) = parse_method_path_from_title(title);
    method.is_some() || path.is_some()
}

fn file_suggests_http(rel_path: &str) -> bool {
    let lower = rel_path.to_ascii_lowercase();
    lower.contains("/routes/")
        || lower.contains("/route/")
        || lower.contains("/api/")
        || lower.contains("/controller")
        || lower.contains("/e2e")
        || lower.contains("/integration")
        || lower.contains(".test.")
}

fn has_status_semantics(assertion: &AssertionEvt, title: &str) -> bool {
    if is_http_status_number(assertion.expected_number)
        || is_http_status_number(assertion.received_number)
    {
        return true;
    }
    let combined = format!(
        "{} {} {}",
        assertion.matcher.as_deref().unwrap_or(""),
        assertion.message.as_deref().unwrap_or(""),
        title
    )
    .to_ascii_lowercase();
    combined.contains("status")
        || combined.contains("statuscode")
        || combined.contains("tohavestatus")
}

fn is_http_relevant(
    rel_path: &str,
    title: &str,
    assertion: &AssertionEvt,
    http_count_in_same_test: usize,
    has_transport_signal: bool,
) -> bool {
    has_transport_signal
        || http_count_in_same_test > 0
        || title_suggests_http(title)
        || has_status_semantics(assertion, title)
        || file_suggests_http(rel_path)
}

fn summarize_url(method: Option<&str>, url: Option<&str>, route: Option<&str>) -> String {
    let base = route.or(url).unwrap_or("");
    let qs = url
        .and_then(|u| u.split_once('?').map(|(_, q)| format!(" ? {q}")))
        .unwrap_or_default();
    [method.unwrap_or(""), base, qs.as_str()]
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn is_http_status_number(value: Option<i64>) -> bool {
    value.is_some_and(|n| (100..=599).contains(&n))
}

fn infer_http_numbers_from_text(text: &str) -> Option<(Option<i64>, Option<i64>)> {
    let expected = find_three_digit_after_label(text, "Expected:")?;
    let received = find_three_digit_after_label(text, "Received:")?;
    Some((Some(expected), Some(received)))
}

fn find_three_digit_after_label(text: &str, label: &str) -> Option<i64> {
    let idx = text.find(label)?;
    let after = &text[idx + label.len()..];
    let digits = after
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    (digits.len() == 3)
        .then(|| digits.parse::<i64>().ok())
        .flatten()
}

fn is_transport_error(msg: Option<&str>) -> bool {
    let lower = msg.unwrap_or("").to_ascii_lowercase();
    lower.contains("socket hang up")
        || lower.contains("econnreset")
        || lower.contains("etimedout")
        || lower.contains("econnrefused")
        || lower.contains("write epipe")
}
