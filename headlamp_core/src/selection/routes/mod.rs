pub mod prefilter_rg;
pub mod ts_js;
pub mod types;

use std::path::{Path, PathBuf};

use crate::selection::deps::ts_js_resolver::TsJsResolveCache;
use crate::selection::routes::types::FileRouteFacts;

#[derive(Debug, Default)]
pub struct RouteExtractorCaches {
    pub ts_js: TsJsResolveCache,
}

pub trait RouteExtractor {
    fn framework_id(&self) -> types::RouteFrameworkId;

    fn candidate_file_globs(&self) -> &'static [&'static str];

    fn rg_fixed_string_tokens(&self) -> &'static [&'static str];

    fn extract_file_facts(
        &self,
        repo_root: &Path,
        abs_path: &Path,
        source_text: &str,
        caches: &mut RouteExtractorCaches,
    ) -> Option<FileRouteFacts>;

    fn discover_candidate_files(&self, repo_root: &Path) -> Vec<PathBuf> {
        prefilter_rg::discover_candidate_files(
            repo_root,
            self.candidate_file_globs(),
            self.rg_fixed_string_tokens(),
        )
    }
}

pub fn built_in_extractors() -> Vec<Box<dyn RouteExtractor>> {
    vec![Box::new(ts_js::express::ExpressRouteExtractor)]
}
