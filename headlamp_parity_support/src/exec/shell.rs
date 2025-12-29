use std::process::Command;

pub(crate) fn build_tty_shell_command(cmd: &Command, columns: usize) -> String {
    let exe = shell_escape(&cmd.get_program().to_string_lossy());
    let args = cmd
        .get_args()
        .map(|a| shell_escape(&a.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ");
    format!("stty cols {columns} rows 40 2>/dev/null || true; exec {exe} {args}")
}

pub(crate) fn build_tty_shell_command_stdout_redirect(
    cmd: &Command,
    columns: usize,
    stdout_capture: &std::path::Path,
) -> String {
    let exe = shell_escape(&cmd.get_program().to_string_lossy());
    let args = cmd
        .get_args()
        .map(|a| shell_escape(&a.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ");
    let stdout_capture = shell_escape(&stdout_capture.to_string_lossy());
    format!("stty cols {columns} rows 40 2>/dev/null || true; exec {exe} {args} > {stdout_capture}")
}

fn shell_escape(text: &str) -> String {
    let safe = text.replace('\'', r"'\''");
    format!("'{safe}'")
}
