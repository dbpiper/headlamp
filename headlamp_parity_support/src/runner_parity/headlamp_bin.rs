use std::path::PathBuf;

pub fn runner_parity_headlamp_bin() -> PathBuf {
    std::env::var("CARGO_BIN_EXE_headlamp")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .unwrap_or_else(|| crate::binaries::runner_parity_binaries().headlamp_bin)
}
