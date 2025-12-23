use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub enum NormalizerKind {
    NonTty,
    TtyUi,
}

#[derive(Debug, Clone, Serialize)]
pub struct NormalizationStageStats {
    pub stage: &'static str,
    pub bytes: usize,
    pub lines: usize,
    pub markers: BTreeMap<&'static str, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NormalizationMeta {
    pub normalizer: NormalizerKind,
    pub used_fallback: bool,
    pub last_failed_tests_line: Option<usize>,
    pub last_test_files_line: Option<usize>,
    pub last_box_table_top_line: Option<usize>,
    pub stages: Vec<NormalizationStageStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParitySideMeta {
    pub raw_bytes: usize,
    pub raw_lines: usize,
    pub normalized_bytes: usize,
    pub normalized_lines: usize,
    pub normalization: NormalizationMeta,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParityCompareMeta {
    pub ts: ParitySideMeta,
    pub rs: ParitySideMeta,
}

#[derive(Debug, Clone)]
pub struct ParityCompareInput {
    pub raw_ts: String,
    pub raw_rs: String,
    pub normalized_ts: String,
    pub normalized_rs: String,
    pub meta: ParityCompareMeta,
}
