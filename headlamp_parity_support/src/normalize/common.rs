use crate::parity_meta::NormalizationStageStats;

pub(super) fn trim_leading_blank_lines(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(lines.len());
    lines[start..].join("\n")
}

pub(super) fn stage_stats(stage: &'static str, text: &str) -> NormalizationStageStats {
    let stripped = headlamp::format::stacks::strip_ansi_simple(text);
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
        bytes: text.len(),
        lines: text.lines().count(),
        markers,
    }
}

pub(super) fn compute_render_indices(text: &str) -> (Option<usize>, Option<usize>, Option<usize>) {
    let stripped = headlamp::format::stacks::strip_ansi_simple(text);
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
