use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::parity_meta::ParitySideLabel;

#[derive(Debug, Clone)]
pub struct ParityRunSpec {
    pub cwd: PathBuf,
    pub program: PathBuf,
    pub side_label: ParitySideLabel,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub tty_columns: Option<usize>,
    pub stdout_piped: bool,
    /// How the command was executed/captured when tty_columns is Some(..).
    /// Populated by the parity harness; `None` means unknown/not-applicable.
    pub exec_backend: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParityRunGroup {
    pub sides: Vec<ParityRunSpec>,
}
