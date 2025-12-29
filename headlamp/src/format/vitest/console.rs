pub(super) fn filter_console_to_failed_tests(
    file: &crate::test_model::TestSuiteResult,
    console_entries: Vec<crate::format::console::ConsoleEntry>,
) -> Vec<crate::format::console::ConsoleEntry> {
    let failed_names = file
        .test_results
        .iter()
        .filter(|a| a.status == "failed")
        .map(|a| a.full_name.as_str())
        .collect::<Vec<_>>();
    if failed_names.is_empty() {
        return console_entries;
    }
    let matches_failed = |e: &crate::format::console::ConsoleEntry| -> bool {
        e.current_test_name
            .as_deref()
            .is_some_and(|n| failed_names.contains(&n))
    };
    if !console_entries.iter().any(matches_failed) {
        return console_entries;
    }
    console_entries
        .into_iter()
        .filter(matches_failed)
        .collect::<Vec<_>>()
}

pub(super) fn extract_expected_received_values(
    messages_array: &[String],
) -> (Option<String>, Option<String>) {
    let stripped = messages_array
        .iter()
        .map(|line| crate::format::stacks::strip_ansi_simple(line))
        .collect::<Vec<_>>();
    let expected = stripped.iter().find_map(|line| {
        line.strip_prefix("Expected: ")
            .map(|v| v.trim().to_string())
    });
    let received = stripped.iter().find_map(|line| {
        line.strip_prefix("Received: ")
            .map(|v| v.trim().to_string())
    });
    if expected.is_some() || received.is_some() {
        return (expected, received);
    }

    let left = stripped.iter().find_map(|line| {
        line.trim_start()
            .strip_prefix("left: ")
            .map(|v| v.trim().to_string())
    });
    let right = stripped.iter().find_map(|line| {
        line.trim_start()
            .strip_prefix("right: ")
            .map(|v| v.trim().to_string())
    });
    (right, left)
}
