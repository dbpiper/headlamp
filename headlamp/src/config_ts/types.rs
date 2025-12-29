use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(super) struct ModuleCacheEntry {
    pub(super) exports: BTreeMap<String, serde_json::Value>,
    pub(super) default_export: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub(super) enum ImportBinding {
    Named { from: PathBuf, export: String },
    Default { from: PathBuf },
}
