use std::cmp::min;

use super::utils::{
    collapse_blank_runs, normalize_temp_paths, strip_ansi, strip_osc8, trim_line_ends,
};

pub(super) fn build_classification_section(out_ts: &str, out_rs: &str) -> String {
    let ts_no_ansi = strip_ansi(out_ts);
    let rs_no_ansi = strip_ansi(out_rs);
    let ts_no_osc8 = strip_osc8(out_ts);
    let rs_no_osc8 = strip_osc8(out_rs);

    let ansi_only = out_ts != out_rs && ts_no_ansi == rs_no_ansi;
    let osc8_only = out_ts != out_rs && ts_no_osc8 == rs_no_osc8;

    let ts_trimmed = trim_line_ends(out_ts);
    let rs_trimmed = trim_line_ends(out_rs);
    let trailing_ws_only = out_ts != out_rs && ts_trimmed == rs_trimmed;

    let ts_collapsed = collapse_blank_runs(&ts_trimmed);
    let rs_collapsed = collapse_blank_runs(&rs_trimmed);
    let blank_runs_only = out_ts != out_rs && ts_collapsed == rs_collapsed;

    let ts_path_norm = normalize_temp_paths(&ts_no_ansi);
    let rs_path_norm = normalize_temp_paths(&rs_no_ansi);
    let path_only = out_ts != out_rs && ts_path_norm == rs_path_norm;

    let bullet = |label: &str, yes: bool| format!("- {label}: {}", if yes { "yes" } else { "no" });
    [
        "Parity mismatch analysis".to_string(),
        bullet("ANSI-only", ansi_only),
        bullet("OSC8-only", osc8_only),
        bullet("trailing-whitespace-only", trailing_ws_only),
        bullet("blank-runs-only", blank_runs_only),
        bullet("path-only", path_only),
    ]
    .join("\n")
}

pub(super) fn build_first_mismatch_section(
    label_0: &str,
    label_1: &str,
    out_0: &str,
    out_1: &str,
) -> String {
    let side_0_lines = out_0.lines().collect::<Vec<_>>();
    let side_1_lines = out_1.lines().collect::<Vec<_>>();
    let max_len = side_0_lines.len().max(side_1_lines.len());
    let first = (0..max_len).find(|&i| side_0_lines.get(i) != side_1_lines.get(i));
    let Some(first) = first else {
        return String::new();
    };
    let window_before = 2usize;
    let window_after = 3usize;
    let start = first.saturating_sub(window_before);
    let end = min(max_len, first + window_after);
    let mut out: Vec<String> = vec![];
    out.push(format!("First mismatch at line {}", first + 1));
    for i in start..end {
        let side_0 = side_0_lines.get(i).copied().unwrap_or("<missing>");
        let side_1 = side_1_lines.get(i).copied().unwrap_or("<missing>");
        if side_0 == side_1 {
            out.push(format!(" {ln:>4}  =  {side_0}", ln = i + 1));
            continue;
        }
        out.push(format!(" {ln:>4} {label_0} {side_0}", ln = i + 1));
        out.push(format!(" {ln:>4} {label_1} {side_1}", ln = i + 1));
        out.push(format!(
            "      {label_0} len={} vis={} | {label_1} len={} vis={}",
            side_0.chars().count(),
            strip_ansi(side_0).chars().count(),
            side_1.chars().count(),
            strip_ansi(side_1).chars().count()
        ));
    }
    out.join("\n")
}

pub(super) fn build_counts_section(
    label_0: &str,
    label_1: &str,
    out_ts: &str,
    out_rs: &str,
) -> String {
    let ts = strip_ansi(out_ts);
    let rs = strip_ansi(out_rs);
    let needles = [
        "Hotspots:",
        "Uncovered functions:",
        "Coverage summary",
        "Uncovered Line #s",
    ];
    let mut out: Vec<String> = vec!["Section marker counts (ANSI-stripped)".to_string()];
    needles.iter().for_each(|needle| {
        let c_ts = ts.matches(needle).count();
        let c_rs = rs.matches(needle).count();
        if c_ts != c_rs {
            out.push(format!("- '{needle}': {label_0}={c_ts} {label_1}={c_rs}"));
        }
    });
    if out.len() == 1 {
        String::new()
    } else {
        out.join("\n")
    }
}

pub(super) fn build_blank_runs_section(
    label_0: &str,
    label_1: &str,
    out_ts: &str,
    out_rs: &str,
) -> String {
    let ts = strip_ansi(out_ts);
    let rs = strip_ansi(out_rs);
    let ts_stats = blank_run_stats(&ts);
    let rs_stats = blank_run_stats(&rs);
    if ts_stats == rs_stats {
        return String::new();
    }
    [
        "Blank run stats (ANSI-stripped)".to_string(),
        format!(
            "- {label_0}: blank_lines={} runs={} max_run={}",
            ts_stats.blank_lines, ts_stats.runs, ts_stats.max_run
        ),
        format!(
            "- {label_1}: blank_lines={} runs={} max_run={}",
            rs_stats.blank_lines, rs_stats.runs, rs_stats.max_run
        ),
    ]
    .join("\n")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlankRunStats {
    blank_lines: usize,
    runs: usize,
    max_run: usize,
}

fn blank_run_stats(text: &str) -> BlankRunStats {
    let mut blank_lines = 0usize;
    let mut runs = 0usize;
    let mut max_run = 0usize;
    let mut current_run = 0usize;
    for line in text.lines() {
        if line.trim().is_empty() {
            blank_lines += 1;
            current_run += 1;
            continue;
        }
        if current_run > 0 {
            runs += 1;
            max_run = max_run.max(current_run);
            current_run = 0;
        }
    }
    if current_run > 0 {
        runs += 1;
        max_run = max_run.max(current_run);
    }
    BlankRunStats {
        blank_lines,
        runs,
        max_run,
    }
}
