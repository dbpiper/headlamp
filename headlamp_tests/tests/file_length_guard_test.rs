use headlamp_tests::guards::max_file_lines::{
    MaxFileLinesGuardConfig, find_files_over_max_physical_lines, format_violation,
};

#[test]
fn rust_files_do_not_exceed_max_physical_lines() {
    let cfg = MaxFileLinesGuardConfig {
        max_physical_lines: 500,
    };
    let violations = find_files_over_max_physical_lines(cfg);
    let rendered = violations.iter().map(format_violation).collect::<Vec<_>>();

    assert!(
        rendered.is_empty(),
        "found {} files over limit ({}):\n{}",
        rendered.len(),
        cfg.max_physical_lines,
        rendered.join("\n")
    );
}
