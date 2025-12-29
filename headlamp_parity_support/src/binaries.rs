use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct ParityBinaries {
    pub ts_cli: PathBuf,
    pub rust_bin: PathBuf,
    pub node_modules: PathBuf,
}

#[derive(Debug, Clone)]
pub struct RunnerParityBinaries {
    pub headlamp_bin: PathBuf,
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var(var)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

pub fn parity_binaries() -> Option<ParityBinaries> {
    if std::env::var("HEADLAMP_RUN_PARITY").ok().as_deref() != Some("1") {
        return None;
    }

    let ts_cli = env_path("HEADLAMP_PARITY_TS_CLI")?;
    let rust_bin = env_path("HEADLAMP_PARITY_RS_BIN")?;
    let node_modules = env_path("HEADLAMP_PARITY_NODE_MODULES")?;

    if !(ts_cli.exists() && rust_bin.exists() && node_modules.exists()) {
        return None;
    }

    Some(ParityBinaries {
        ts_cli,
        rust_bin,
        node_modules,
    })
}

pub fn runner_parity_binaries() -> RunnerParityBinaries {
    RunnerParityBinaries {
        headlamp_bin: std::env::var("CARGO_BIN_EXE_headlamp")
            .ok()
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .unwrap_or_else(ensure_headlamp_bin_from_target_dir),
    }
}

fn ensure_headlamp_bin_from_target_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .unwrap_or_else(|| panic!("expected {} to have a parent dir", manifest_dir.display()))
        .to_path_buf();

    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("target"));

    let exe_name = if cfg!(windows) {
        "headlamp.exe"
    } else {
        "headlamp"
    };
    let bin_path = target_dir.join("debug").join(exe_name);
    let status = Command::new("cargo")
        .current_dir(&workspace_root)
        .args(["build", "-q", "-p", "headlamp"])
        .status()
        .unwrap_or_else(|e| panic!("failed to run cargo build: {e}"));
    if !status.success() {
        panic!(
            "failed to build headlamp binary (status={:?})",
            status.code()
        );
    }
    if !bin_path.exists() {
        panic!("headlamp binary missing at {}", bin_path.display());
    }
    bin_path
}
