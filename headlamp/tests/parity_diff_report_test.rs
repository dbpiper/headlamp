mod parity_support;

use parity_support::diff_report::build_parity_report;

#[test]
fn parity_diff_report_classifies_ansi_only() {
    let ts = "\u{1b}[31mFAIL\u{1b}[0m";
    let rs = "\u{1b}[32mFAIL\u{1b}[0m";
    let report = build_parity_report(ts, rs);
    assert!(report.contains("ANSI-only: yes"), "{report}");
}

#[test]
fn parity_diff_report_classifies_blank_runs_only() {
    let ts = "a\n\n\nb\n";
    let rs = "a\n\nb\n";
    let report = build_parity_report(ts, rs);
    assert!(report.contains("blank-runs-only: yes"), "{report}");
}

#[test]
fn parity_diff_report_classifies_temp_path_only() {
    let ts = "RUN  /private/var/folders/aa/bb/T/hl/foo\n";
    let rs = "RUN  /var/folders/aa/bb/T/hl/foo\n";
    let report = build_parity_report(ts, rs);
    assert!(report.contains("path-only: yes"), "{report}");
}

#[test]
fn parity_diff_report_detects_box_table_blank_row_count_change() {
    let ts = [
        "┌─┐",
        "│a│",
        "│ │",
        "│ │",
        "└─┘",
    ]
    .join("\n");
    let rs = ["┌─┐", "│a│", "│ │", "└─┘"].join("\n");
    let report = build_parity_report(&ts, &rs);
    assert!(report.contains("Box tables: ts=1 rs=1"), "{report}");
    assert!(report.contains("blank_rows: ts=2 rs=1"), "{report}");
    assert!(
        report.contains("extra blank filler row in TS"),
        "{report}"
    );
}

#[test]
fn parity_diff_report_shows_first_aligned_mismatch_row_content() {
    let ts = ["┌─┐", "│a│", "│ │", "│b│", "└─┘"].join("\n");
    let rs = ["┌─┐", "│a│", "│c│", "└─┘"].join("\n");
    let report = build_parity_report(&ts, &rs);
    assert!(report.contains("first aligned mismatch"), "{report}");
    assert!(report.contains("TS: │b│") || report.contains("TS: │ │"), "{report}");
    assert!(report.contains("RS: │c│"), "{report}");
}

#[test]
fn parity_diff_report_locates_first_mismatch() {
    let ts = "l1\nl2\nl3\n";
    let rs = "l1\nx2\nl3\n";
    let report = build_parity_report(ts, rs);
    assert!(report.contains("First mismatch at line 2"), "{report}");
}

#[test]
fn parity_diff_report_detects_istanbul_pipe_table() {
    let ts = [
        "----|---",
        "File | % Lines |",
        "----|---",
        "a.js | 50 |",
        "----|---",
    ]
    .join("\n");
    let rs = [
        "----|---",
        "File | % Lines |",
        "----|---",
        "a.js | 40 |",
        "----|---",
    ]
    .join("\n");
    let report = build_parity_report(&ts, &rs);
    assert!(report.contains("Istanbul pipe tables: ts=1 rs=1"), "{report}");
    assert!(report.contains("first_mismatch_line"), "{report}");
}


