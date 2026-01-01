use std::path::PathBuf;

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
        headlamp_bin: env_path("HEADLAMP_PARITY_HEADLAMP_BIN")
            .filter(|p| p.exists())
            .or_else(|| {
                std::env::var("CARGO_BIN_EXE_headlamp")
                    .ok()
                    .map(PathBuf::from)
                    .filter(|p| p.exists())
            })
            .or_else(headlamp_bin_from_target_dir_if_present)
            .unwrap_or_else(|| {
                panic!("headlamp parity requires an existing headlamp binary. Set HEADLAMP_PARITY_HEADLAMP_BIN to an executable path (recommended) or build `target/debug/headlamp` before running parity tests.")
            }),
    }
}

fn headlamp_bin_from_target_dir_if_present() -> Option<PathBuf> {
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
    bin_path.exists().then_some(bin_path)
}
