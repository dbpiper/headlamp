use std::path::Path;

use path_slash::PathExt;
use regex::Regex;

use super::parity_meta::{NormalizationMeta, NormalizationStageStats, NormalizerKind};

struct RenderNormalizerState {
    out: Vec<String>,
    skip_until_sep: bool,
    capturing_logs: bool,
    capturing_http: bool,
    capturing_stack: bool,
    kept_project_stack_lines: usize,
    pending_blank_after_logs: bool,
}

pub fn normalize(text: String, root: &Path) -> String {
    normalize_with_meta(text, root).0
}

pub fn normalize_tty_ui(text: String, root: &Path) -> String {
    normalize_tty_ui_with_meta(text, root).0
}

pub fn normalize_tty_ui_runner_parity(text: String, root: &Path) -> String {
    let (normalized, _meta) = normalize_tty_ui_with_meta(text, root);
    strip_failure_details(&normalized)
}

pub fn normalize_tty_ui_runner_parity_with_meta(
    text: String,
    root: &Path,
) -> (String, NormalizationMeta) {
    let (normalized, meta) = normalize_tty_ui_with_meta(text, root);
    (strip_failure_details(&normalized), meta)
}

pub fn normalize_with_meta(text: String, root: &Path) -> (String, NormalizationMeta) {
    let normalized_paths = normalize_paths(text, root);
    let filtered = drop_nondeterministic_lines(&normalized_paths);
    let stripped = strip_terminal_sequences(&filtered);
    let final_block = pick_final_render_block(&stripped);
    let normalized = trim_leading_blank_lines(&normalize_render_block(&final_block));

    let (last_failed_tests_line, last_test_files_line, last_box_table_top_line) =
        compute_render_indices(&stripped);
    let stages = vec![
        stage_stats("normalized_paths", &normalized_paths),
        stage_stats("filtered", &filtered),
        stage_stats("stripped", &stripped),
        stage_stats("final_block", &final_block),
        stage_stats("normalized", &normalized),
    ];
    let meta = NormalizationMeta {
        normalizer: NormalizerKind::NonTty,
        used_fallback: false,
        last_failed_tests_line,
        last_test_files_line,
        last_box_table_top_line,
        stages,
    };
    (normalized, meta)
}

pub fn normalize_tty_ui_with_meta(text: String, root: &Path) -> (String, NormalizationMeta) {
    let normalized_paths = normalize_paths(text, root);
    let no_osc8 = strip_osc8_sequences(&normalized_paths);
    // Normalize CRLF and CR progress frames without introducing extra blank lines.
    // - Convert CRLF to LF (avoid turning '\r\n' into '\n\n')
    // - Convert "erase+CR" progress frames to a newline boundary
    // - Convert any remaining CR to LF
    let normalized_cr = no_osc8
        .replace("\u{1b}[2K\r", "\n")
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let filtered = normalized_cr
        .lines()
        .filter(|raw_line| should_keep_line_tty(raw_line))
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let filtered = drop_box_table_interior_blank_lines(&filtered);

    let final_block = pick_final_render_block_tty(&filtered);
    let (normalized, used_fallback) = if final_block.trim().is_empty() {
        (
            trim_leading_blank_lines(&pick_final_render_block_tty(&normalized_cr)),
            true,
        )
    } else {
        (trim_leading_blank_lines(&final_block), false)
    };

    let (last_failed_tests_line, last_test_files_line, last_box_table_top_line) =
        compute_render_indices(&normalized);
    let stages = vec![
        stage_stats("normalized_paths", &normalized_paths),
        stage_stats("no_osc8", &no_osc8),
        stage_stats("normalized_cr", &normalized_cr),
        stage_stats("filtered", &filtered),
        stage_stats("normalized", &normalized),
    ];
    let meta = NormalizationMeta {
        normalizer: NormalizerKind::TtyUi,
        used_fallback,
        last_failed_tests_line,
        last_test_files_line,
        last_box_table_top_line,
        stages,
    };
    (normalized, meta)
}

fn trim_leading_blank_lines(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(lines.len());
    lines[start..].join("\n")
}

fn strip_failure_details(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return String::new();
    }
    let stripped = lines
        .iter()
        .map(|l| strip_all_ansi_like_sequences(l))
        .collect::<Vec<_>>();

    let fail_line = stripped
        .iter()
        .position(|l| l.trim_start().starts_with("FAIL "));
    let summary_line = stripped.iter().position(|l| l.contains("Failed Tests"));
    let (Some(fail_i), Some(summary_i)) = (fail_line, summary_line) else {
        return text.to_string();
    };
    if summary_i <= fail_i {
        return text.to_string();
    }
    lines
        .iter()
        .take(fail_i + 1)
        .chain(lines.iter().skip(summary_i.saturating_sub(1)))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_all_ansi_like_sequences(text: &str) -> String {
    // Normalization replaces numeric ANSI parameters with placeholders like `<N>`, which makes
    // our usual ANSI stripper ineffective. For parity normalization we just strip any CSI SGR
    // sequence shaped like ESC [ ... m.
    let re = regex::Regex::new(r"\x1b\[[^m]*m").unwrap();
    re.replace_all(text, "").to_string()
}

fn normalize_paths(mut text: String, root: &Path) -> String {
    let root_s = root.to_slash_lossy().to_string();
    text = text.replace('\\', "/");
    text = text.replace(&root_s, "<ROOT>");
    text = regex_replace(&text, r"jest-bridge-[0-9]+\.json", "jest-bridge-<PID>.json");
    text = regex_replace(&text, r"\+[0-9]+s\]", "+<N>s]");
    text = regex_replace(&text, r"\b[0-9]{1,5}ms\b", "<N>ms");
    text = regex_replace(
        &text,
        r"((?:\x1b\[[0-9;]*m)*Time(?:\x1b\[[0-9;]*m)*)\s+<N>ms\s+",
        "$1 ",
    );
    text = regex_replace(
        &text,
        r"((?:\x1b\[[0-9;]*m)*Time(?:\x1b\[[0-9;]*m)*)\s+((?:\x1b\[[0-9;]*m)*)\(in thread",
        "$1 $2(in thread",
    );
    text = regex_replace(&text, r":\d+:\d+\b", ":<LINE>:<COL>");
    text = regex_replace(&text, r":\d+\b", ":<LINE>");
    normalize_fixture_exts(&text)
}

fn normalize_fixture_exts(text: &str) -> String {
    // Avoid \b boundaries because ANSI color sequences end with 'm', which is a word char and
    // breaks boundary matching right before the filename.
    let test_ext = Regex::new(r"\.test\.(js|rs)").unwrap();
    let test_suffix_ext = Regex::new(r#"(?m)(tests/[^ \t\r\n:'"]+_test)\.(js|rs)"#).unwrap();
    let src_ext = Regex::new(r#"(?m)(src/[^ \t\r\n:'"]+)\.(js|rs)"#).unwrap();
    let tests_ext = Regex::new(r#"(?m)(tests/[^ \t\r\n:'"]+)\.(js|rs)"#).unwrap();

    let with_tests = test_ext.replace_all(text, ".test.<EXT>").to_string();
    let with_test_suffix = test_suffix_ext
        .replace_all(&with_tests, "$1.<EXT>")
        .to_string();
    let with_src = src_ext
        .replace_all(&with_test_suffix, "$1.<EXT>")
        .to_string();
    normalize_coverage_numbers(&tests_ext.replace_all(&with_src, "$1.<EXT>").to_string())
}

fn normalize_coverage_numbers(text: &str) -> String {
    let summary = Regex::new(r"Lines: [0-9]+(\.[0-9]+)?% \([0-9]+/[0-9]+\)").unwrap();
    let pct = Regex::new(r"\b[0-9]+(\.[0-9]+)?%").unwrap();
    let count = Regex::new(r"\b[0-9]{1,6}\b").unwrap();

    let with_summary = summary
        .replace_all(text, "Lines: <PCT>% (<N>/<N>)")
        .to_string();
    let with_pct = pct.replace_all(&with_summary, "<PCT>%").to_string();
    count.replace_all(&with_pct, "<N>").to_string()
}

fn should_keep_line_tty(raw_line: &str) -> bool {
    // Decide based on a stripped version, but keep the original ANSI line.
    let stripped = headlamp_core::format::stacks::strip_ansi_simple(raw_line);
    should_keep_line(&stripped)
}

fn pick_final_render_block_tty(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return String::new();
    }
    let last_box_table_top = find_last_box_table_top(&lines);
    let last_test_files = lines
        .iter()
        .rposition(|line| {
            headlamp_core::format::stacks::strip_ansi_simple(line).starts_with("Test Files ")
        })
        .unwrap_or_else(|| lines.len().saturating_sub(1));

    let last_failed_tests = (0..=last_test_files).rev().find(|&i| {
        headlamp_core::format::stacks::strip_ansi_simple(lines[i]).contains("Failed Tests")
    });

    let start = last_failed_tests
        .and_then(|failed_i| find_render_block_start_tty(&lines, failed_i))
        .or_else(|| {
            last_box_table_top.and_then(|box_top| find_render_block_start_tty(&lines, box_top))
        })
        .or(last_box_table_top)
        .unwrap_or(0);
    lines[start..].join("\n")
}

fn find_render_block_start_tty(lines: &[&str], failed_i: usize) -> Option<usize> {
    (0..=failed_i).rev().find(|&i| {
        let stripped = headlamp_core::format::stacks::strip_ansi_simple(lines[i]);
        let ln = stripped.trim_start();
        ln.starts_with("RUN  ") || ln.starts_with("FAIL ") || ln.starts_with("PASS ")
    })
}

fn find_last_box_table_top(lines: &[&str]) -> Option<usize> {
    (0..lines.len()).rev().find(|&i| {
        let stripped = headlamp_core::format::stacks::strip_ansi_simple(lines[i]);
        if !stripped.trim_start().starts_with('┌') {
            return false;
        }
        let maybe_header_idx = lines
            .iter()
            .enumerate()
            .skip(i.saturating_add(1))
            .take(8)
            .find_map(|(j, l)| {
                let s = headlamp_core::format::stacks::strip_ansi_simple(l);
                if s.trim().is_empty() {
                    return None;
                }
                Some((j, s))
            });
        let Some((_header_j, header_line)) = maybe_header_idx else {
            return false;
        };
        header_line.contains("│File") || header_line.contains("File ")
    })
}

fn stage_stats(stage: &'static str, text: &str) -> NormalizationStageStats {
    let stripped = headlamp_core::format::stacks::strip_ansi_simple(text);
    let mut markers = std::collections::BTreeMap::new();
    markers.insert(
        "RUN",
        stripped
            .lines()
            .filter(|l| l.trim_start().starts_with("RUN  "))
            .count(),
    );
    markers.insert(
        "PASS",
        stripped
            .lines()
            .filter(|l| l.trim_start().starts_with("PASS "))
            .count(),
    );
    markers.insert(
        "FAIL",
        stripped
            .lines()
            .filter(|l| l.trim_start().starts_with("FAIL "))
            .count(),
    );
    markers.insert(
        "TestFiles",
        stripped
            .lines()
            .filter(|l| l.trim_start().starts_with("Test Files "))
            .count(),
    );
    markers.insert(
        "FailedTests",
        stripped
            .lines()
            .filter(|l| l.contains("Failed Tests"))
            .count(),
    );
    markers.insert(
        "BoxTableTop",
        stripped
            .lines()
            .filter(|l| l.trim_start().starts_with('┌'))
            .count(),
    );
    NormalizationStageStats {
        stage,
        bytes: text.as_bytes().len(),
        lines: text.lines().count(),
        markers,
    }
}

fn compute_render_indices(text: &str) -> (Option<usize>, Option<usize>, Option<usize>) {
    let stripped = headlamp_core::format::stacks::strip_ansi_simple(text);
    let stripped_lines = stripped.lines().collect::<Vec<_>>();
    let last_failed_tests_line = stripped_lines
        .iter()
        .rposition(|l| l.contains("Failed Tests"))
        .map(|i| i + 1);
    let last_test_files_line = stripped_lines
        .iter()
        .rposition(|l| l.trim_start().starts_with("Test Files "))
        .map(|i| i + 1);
    let last_box_table_top_line = stripped_lines
        .iter()
        .rposition(|l| l.trim_start().starts_with('┌'))
        .map(|i| i + 1);
    (
        last_failed_tests_line,
        last_test_files_line,
        last_box_table_top_line,
    )
}

fn drop_box_table_interior_blank_lines(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let is_table_line = |s: &str| {
        let stripped = headlamp_core::format::stacks::strip_ansi_simple(s);
        let trimmed = stripped.trim_start();
        trimmed.starts_with('│')
            || trimmed.starts_with('┌')
            || trimmed.starts_with('└')
            || trimmed.starts_with('┼')
            || trimmed.chars().all(|c| c == '─')
    };
    let mut kept: Vec<&str> = vec![];
    for (index, line) in lines.iter().enumerate() {
        let stripped = headlamp_core::format::stacks::strip_ansi_simple(line);
        if !stripped.trim().is_empty() {
            kept.push(line);
            continue;
        }
        let prev_is_table = (0..index).rev().find_map(|i| {
            let s = headlamp_core::format::stacks::strip_ansi_simple(lines[i]);
            (!s.trim().is_empty()).then_some(is_table_line(lines[i]))
        });
        let next_is_table = (index + 1..lines.len()).find_map(|i| {
            let s = headlamp_core::format::stacks::strip_ansi_simple(lines[i]);
            (!s.trim().is_empty()).then_some(is_table_line(lines[i]))
        });
        if prev_is_table == Some(true) && next_is_table == Some(true) {
            continue;
        }
        kept.push(line);
    }
    kept.join("\n")
}

fn drop_nondeterministic_lines(text: &str) -> String {
    text.lines()
        .filter(|line| should_keep_line(line))
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn should_keep_line(line: &str) -> bool {
    // Drop live progress frames; keep the final rendered "RUN" line.
    if line.contains("\u{1b}[2K") && line.contains("RUN [") {
        return false;
    }
    if line.contains("waiting for Jest") || line.contains("+<N>s]") {
        return false;
    }

    if line.contains("/headlamp-original/") {
        return false;
    }
    if line.contains("/dist/cli.cjs") {
        return false;
    }
    if line.contains("node:internal/") {
        return false;
    }
    if line.contains("(node:") || line.contains("node:events:") {
        return false;
    }
    if line.contains("internal/process/") {
        return false;
    }

    let discovery_prefixes = [
        "Selection classify",
        "Discovering →",
        "Discovering (",
        "rg related →",
        "rg candidates →",
        "http augmented candidates →",
        "fallback refine",
        "No matching tests were discovered",
        "Jest args:",
        "Tip:",
        "Selected files →",
        "Discovery results →",
        "Discovery →",
        "Run plan →",
        "Starting Jest",
        " - ",
    ];
    !discovery_prefixes
        .iter()
        .any(|prefix| line.starts_with(prefix))
}

fn strip_terminal_sequences(text: &str) -> String {
    let no_osc8 = strip_osc8_sequences(text);
    regex_replace(&no_osc8, "\u{1b}\\[[0-9;]*m", "")
}

fn strip_osc8_sequences(text: &str) -> String {
    let no_osc8 = regex_replace(text, "\u{1b}\\]8;;[^\\u{7}]*\\u{7}", "");
    regex_replace(&no_osc8, "\u{1b}\\]8;;\\u{7}", "")
}

fn pick_final_render_block(text: &str) -> String {
    let needle = " RUN  /private<ROOT>";
    if let Some(idx) = text.rfind(needle) {
        return text[idx + 1..].to_string();
    }
    if let Some(block) = pick_last_test_files_block(text) {
        return block;
    }
    text.to_string()
}

fn pick_last_test_files_block(text: &str) -> Option<String> {
    let lines = text.lines().collect::<Vec<_>>();
    let last_test_files = lines
        .iter()
        .rposition(|line| line.starts_with("Test Files "))?;

    let last_failed_tests = (0..=last_test_files)
        .rev()
        .find(|&i| lines[i].contains("Failed Tests"));
    let start = last_failed_tests
        .and_then(|failed_i| find_render_block_start(&lines, failed_i))
        .unwrap_or(0);
    Some(lines[start..].join("\n"))
}

fn find_render_block_start(lines: &[&str], failed_i: usize) -> Option<usize> {
    (0..=failed_i).rev().find(|&i| {
        let ln = lines[i].trim_start();
        ln.starts_with(" RUN  ") || ln.starts_with("FAIL  ") || ln.starts_with("PASS  ")
    })
}

fn normalize_render_block(block: &str) -> String {
    let mut state = RenderNormalizerState::new();
    block.lines().for_each(|line| {
        state.push_line(line);
    });
    state.finish()
}

impl RenderNormalizerState {
    fn new() -> Self {
        Self {
            out: vec![],
            skip_until_sep: false,
            capturing_logs: false,
            capturing_http: false,
            capturing_stack: false,
            kept_project_stack_lines: 0,
            pending_blank_after_logs: false,
        }
    }

    fn push_line(&mut self, raw: &str) {
        if self.handle_fail_header(raw) {
            return;
        }
        if self.skip_until_sep && self.handle_skip_region(raw) {
            return;
        }
        if self.handle_blank_after_logs(raw) {
            return;
        }
        self.out.push(normalize_time_line(raw));
    }

    fn handle_fail_header(&mut self, raw: &str) -> bool {
        let trimmed = raw.trim_start();
        if trimmed.starts_with("FAIL ") || raw.starts_with("× ") {
            self.out.push(raw.to_string());
            self.skip_until_sep = true;
            return true;
        }
        false
    }

    fn handle_skip_region(&mut self, raw: &str) -> bool {
        if self.capturing_logs {
            return self.capture_logs_line(raw);
        }
        if self.capturing_http {
            return self.capture_http_line(raw);
        }
        if self.capturing_stack {
            return self.capture_stack_line(raw);
        }
        if raw.trim_start().starts_with("Logs:") {
            self.capturing_logs = true;
            self.out.push(raw.to_string());
            return true;
        }
        if raw.trim_start().starts_with("HTTP:") {
            self.capturing_http = true;
            self.out.push(raw.to_string());
            return true;
        }
        if raw.trim_start().starts_with("Stack:") {
            self.capturing_stack = true;
            self.kept_project_stack_lines = 0;
            return true;
        }
        if raw.starts_with('─') || raw.starts_with("────────────────")
        {
            self.skip_until_sep = false;
            self.out.push(raw.to_string());
            return true;
        }
        true
    }

    fn capture_logs_line(&mut self, raw: &str) -> bool {
        self.out.push(raw.to_string());
        if self.out.last().is_some_and(|last| last.trim().is_empty()) {
            self.capturing_logs = false;
            self.pending_blank_after_logs = true;
        }
        true
    }

    fn capture_http_line(&mut self, raw: &str) -> bool {
        self.out.push(raw.to_string());
        if raw.trim().is_empty() {
            self.capturing_http = false;
        }
        true
    }

    fn capture_stack_line(&mut self, raw: &str) -> bool {
        if raw.trim().is_empty() {
            self.finish_stack_capture();
            self.out.push(raw.to_string());
            return true;
        }

        if is_project_stack_frame(raw) {
            self.out.push(raw.to_string());
            self.kept_project_stack_lines += 1;
        }
        if self.kept_project_stack_lines >= 2 {
            self.finish_stack_capture();
            self.out.push(String::new());
        }
        true
    }

    fn finish_stack_capture(&mut self) {
        if self.kept_project_stack_lines > 0 {
            let insert_at = self.out.len().saturating_sub(self.kept_project_stack_lines);
            self.out.insert(insert_at, "    Stack:".to_string());
        }
        self.capturing_stack = false;
        self.kept_project_stack_lines = 0;
    }

    fn handle_blank_after_logs(&mut self, raw: &str) -> bool {
        if !self.pending_blank_after_logs {
            return false;
        }
        if raw.trim().is_empty() {
            self.out.push(raw.to_string());
        }
        self.pending_blank_after_logs = false;
        true
    }

    fn finish(self) -> String {
        let collapsed = self.out.join("\n").trim().replace("\n\n\n", "\n\n");
        regex_replace(&collapsed, r"(\n FAIL[^\n]*\n)\n(─{10,})", "$1$2")
    }
}

fn is_project_stack_frame(line: &str) -> bool {
    let normalized = line.replace('\\', "/");
    normalized.contains("/tests/") && !normalized.contains("/node_modules/")
}

fn normalize_time_line(raw: &str) -> String {
    if raw.starts_with("Time      ") {
        return "Time      0ms (in thread 0ms, 0.00%)".to_string();
    }
    raw.to_string()
}

fn regex_replace(text: &str, pat: &str, repl: &str) -> String {
    let re = regex::Regex::new(pat).unwrap();
    re.replace_all(text, repl).to_string()
}
