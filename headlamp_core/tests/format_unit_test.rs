use headlamp_core::format::bridge::BridgeConsoleEntry;
use headlamp_core::format::bridge_console::parse_bridge_console;
use headlamp_core::format::codeframe::{Loc, build_code_frame_section};
use headlamp_core::format::details::merge_msg_lines;
use headlamp_core::format::paths::preferred_editor_href;
use headlamp_core::format::stacks::collapse_stacks;

#[test]
fn preferred_editor_href_file_scheme() {
    // In Rust 2024, mutating process env is `unsafe` due to potential data races.
    unsafe {
        std::env::remove_var("COVERAGE_EDITOR");
        std::env::remove_var("VSCODE_IPC_HOOK");
        std::env::set_var("TERM_PROGRAM", "not-vscode");
    }

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.rs");
    std::fs::write(&file, "fn main() {}\n").unwrap();

    let href = preferred_editor_href(file.to_string_lossy().as_ref(), Some(12), None);
    assert!(href.starts_with("file://"));
    assert!(href.contains("#L12"));
}

#[test]
fn collapse_stacks_hides_noisy_frames() {
    let lines = vec![
        "Error: boom".to_string(),
        "    at Object.<anonymous> (/repo/node_modules/pkg/index.js:1:2)".to_string(),
        "    at node:internal/process/task_queues:105:5".to_string(),
        "    at myFn (/repo/src/main.js:10:2)".to_string(),
    ];
    let out = collapse_stacks(&lines);
    let joined = out.join("\n");
    insta::assert_snapshot!("collapse_stacks_hides_noisy_frames", joined);
}

#[test]
fn build_code_frame_from_source_location() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("x.js");
    std::fs::write(&file, "line1\nline2\nline3\nline4\nline5\n").unwrap();

    let loc = Loc {
        file: file.to_string_lossy().to_string(),
        line: 3,
        column: None,
    };
    let out = build_code_frame_section(&[], true, Some(&loc));
    let joined = out.join("\n");
    insta::assert_snapshot!("code_frame_from_source_location", joined);
}

#[test]
fn merge_msg_lines_dedupes() {
    let merged = merge_msg_lines("a\nb\n", &vec!["b".to_string(), "c".to_string()]);
    insta::assert_snapshot!("merge_msg_lines_dedupes", merged.join("\n"));
}

#[test]
fn parse_bridge_console_extracts_console_event() {
    let entries = vec![BridgeConsoleEntry {
        message: Some(serde_json::Value::String(
            "[JEST-BRIDGE-EVENT] {type:'console', level:'error', message:'oops'}".to_string(),
        )),
        type_name: Some("log".to_string()),
        origin: Some("origin".to_string()),
    }];
    let (_http, _asserts, console) = parse_bridge_console(Some(&entries));
    let summary = console
        .iter()
        .map(|e| {
            format!(
                "{}: {}",
                e.type_name.clone().unwrap_or_default(),
                e.message.clone().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!("parse_bridge_console_extracts_console_event", summary);
}
