use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ParitySideLabel {
    pub binary: String,
    pub runner_stack: String,
}

impl ParitySideLabel {
    pub fn display_label(&self) -> String {
        format!("{}[{}]", self.binary, self.runner_stack)
    }

    pub fn file_safe_label(&self) -> String {
        let label = self.display_label();
        let mut out: String = String::with_capacity(label.len());
        let mut prev_dash = false;
        for ch in label.chars() {
            let lower = ch.to_ascii_lowercase();
            let keep = lower.is_ascii_alphanumeric() || matches!(lower, '-' | '_' | '.');
            let mapped = if keep { lower } else { '-' };
            if mapped == '-' {
                if !prev_dash {
                    out.push('-');
                }
                prev_dash = true;
            } else {
                out.push(mapped);
                prev_dash = false;
            }
        }
        out.trim_matches(['-', '_']).to_string()
    }
}

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
pub struct ParityCompareSideInput {
    pub label: ParitySideLabel,
    pub exit: i32,
    pub raw: String,
    pub normalized: String,
    pub meta: ParitySideMeta,
}

#[derive(Debug, Clone)]
pub struct ParityCompareInput {
    pub sides: Vec<ParityCompareSideInput>,
}
