use std::process::ExitCode;

use headlamp_tests::guards::max_file_lines::{
    MaxFileLinesGuardConfig, find_files_over_max_physical_lines, format_violation,
};

fn main() -> ExitCode {
    let cfg = MaxFileLinesGuardConfig {
        max_physical_lines: 500,
    };
    let violations = find_files_over_max_physical_lines(cfg);
    if violations.is_empty() {
        return ExitCode::SUCCESS;
    }

    let rendered = violations.iter().map(format_violation).collect::<Vec<_>>();
    eprintln!(
        "found {} files over limit ({}):\n{}",
        rendered.len(),
        cfg.max_physical_lines,
        rendered.join("\n")
    );
    ExitCode::FAILURE
}
