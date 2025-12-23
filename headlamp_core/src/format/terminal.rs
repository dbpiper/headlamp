use std::io::IsTerminal;

use terminal_size::{Height, Width, terminal_size_of};

pub fn is_output_terminal() -> bool {
    std::io::stdout().is_terminal() || std::io::stderr().is_terminal()
}

pub fn detect_terminal_size_cols_rows() -> Option<(usize, usize)> {
    let stdout = std::io::stdout();
    if stdout.is_terminal() {
        return terminal_size_of(stdout).map(|(Width(w), Height(h))| (w as usize, h as usize));
    }

    let stderr = std::io::stderr();
    stderr
        .is_terminal()
        .then(|| terminal_size_of(stderr).map(|(Width(w), Height(h))| (w as usize, h as usize)))
        .flatten()
}


