use std::path::Path;

use path_slash::PathExt;

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
    let normalized_paths = normalize_paths(text, root);
    let filtered = drop_nondeterministic_lines(&normalized_paths);
    let stripped = strip_terminal_sequences(&filtered);
    let final_block = pick_final_render_block(&stripped);
    normalize_render_block(&final_block)
}

fn normalize_paths(mut text: String, root: &Path) -> String {
    let root_s = root.to_slash_lossy().to_string();
    text = text.replace('\\', "/");
    text = text.replace(&root_s, "<ROOT>");
    text = regex_replace(&text, r"jest-bridge-[0-9]+\.json", "jest-bridge-<PID>.json");
    text = regex_replace(&text, r"\+[0-9]+s\]", "+<N>s]");
    regex_replace(&text, r"\b[0-9]{1,5}ms\b", "<N>ms")
}

fn drop_nondeterministic_lines(text: &str) -> String {
    text.lines()
        .filter(|line| should_keep_line(line))
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn should_keep_line(line: &str) -> bool {
    if line.contains("\u{1b}[2K\rRUN ") {
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
    let no_osc8 = regex_replace(text, "\u{1b}\\]8;;[^\\u{7}]*\\u{7}", "");
    let no_osc8 = regex_replace(&no_osc8, "\u{1b}\\]8;;\\u{7}", "");
    regex_replace(&no_osc8, "\u{1b}\\[[0-9;]*m", "")
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
