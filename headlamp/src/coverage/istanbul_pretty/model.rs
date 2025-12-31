use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct IstanbulLoc {
    #[serde(default)]
    pub(super) line: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct IstanbulLocRange {
    #[serde(default)]
    pub(super) start: Option<IstanbulLoc>,
    #[serde(default)]
    pub(super) end: Option<IstanbulLoc>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct IstanbulFnMeta {
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) line: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct IstanbulBranchMeta {
    #[serde(default)]
    pub(super) line: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct IstanbulFileRecord {
    #[serde(default)]
    pub(super) path: Option<String>,

    #[serde(default)]
    pub(super) l: Option<BTreeMap<String, u64>>,

    #[serde(default)]
    pub(super) s: Option<BTreeMap<String, u64>>,

    #[serde(default)]
    #[serde(rename = "statementMap")]
    pub(super) statement_map: Option<BTreeMap<String, IstanbulLocRange>>,

    #[serde(default)]
    pub(super) f: Option<BTreeMap<String, u64>>,

    #[serde(default)]
    #[serde(rename = "fnMap")]
    pub(super) fn_map: Option<BTreeMap<String, IstanbulFnMeta>>,

    #[serde(default)]
    pub(super) b: Option<BTreeMap<String, Vec<u64>>>,

    #[serde(default)]
    #[serde(rename = "branchMap")]
    pub(super) branch_map: Option<BTreeMap<String, IstanbulBranchMeta>>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Counts {
    pub(super) covered: u32,
    pub(super) total: u32,
}

impl Counts {
    pub(super) fn pct(self) -> f64 {
        if self.total == 0 {
            100.0
        } else {
            (self.covered as f64 / self.total as f64) * 100.0
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct FileSummary {
    pub(super) statements: Counts,
    pub(super) branches: Counts,
    pub(super) functions: Counts,
    pub(super) lines: Counts,
}

#[derive(Debug, Clone)]
pub(super) struct MissedFunction {
    pub(super) name: String,
    pub(super) line: u32,
}

#[derive(Debug, Clone)]
pub(super) struct MissedBranch {
    pub(super) id: String,
    pub(super) line: u32,
    pub(super) zero_paths: Vec<u32>,
}

#[derive(Debug, Clone)]
pub(super) struct UncoveredRange {
    pub(super) start: u32,
    pub(super) end: u32,
}

#[derive(Debug, Clone)]
pub(super) struct FullFileCoverage {
    #[allow(dead_code)]
    pub(super) abs_path: String,
    pub(super) rel_path: String,
    pub(super) statement_hits: BTreeMap<u64, u32>,
    pub(super) statement_map: BTreeMap<u64, (u32, u32)>,
    pub(super) function_hits: BTreeMap<String, u32>,
    pub(super) function_map: BTreeMap<String, (String, u32)>,
    pub(super) branch_hits: BTreeMap<String, Vec<u32>>,
    pub(super) branch_map: BTreeMap<String, u32>,
    pub(super) line_hits: BTreeMap<u32, u32>,
}
