use std::collections::BTreeMap;
use std::path::Path;

mod index_build;
mod normalize;
mod scan;

#[derive(Debug, Clone, Default)]
pub struct RouteIndex {
    sources_by_http_route: BTreeMap<String, Vec<String>>,
    http_routes_by_source: BTreeMap<String, Vec<String>>,
}

impl RouteIndex {
    pub fn sources_for_http_route(&self, http_path: &str) -> Vec<String> {
        self.sources_by_http_route
            .get(&normalize::normalize_http_path(http_path))
            .cloned()
            .unwrap_or_default()
    }

    pub fn http_routes_for_source(&self, source_path: &str) -> Vec<String> {
        self.http_routes_by_source
            .get(&normalize::normalize_fs_path(source_path))
            .cloned()
            .unwrap_or_default()
    }
}

pub fn get_route_index(repo_root: &Path) -> RouteIndex {
    let mut caches = crate::selection::routes::RouteExtractorCaches::default();
    let extractors = crate::selection::routes::built_in_extractors();

    let mut facts_by_file: BTreeMap<String, crate::selection::routes::types::FileRouteFacts> =
        BTreeMap::new();
    extractors.iter().for_each(|extractor| {
        extractor
            .discover_candidate_files(repo_root)
            .into_iter()
            .for_each(|abs_path| {
                let Ok(source_text) = std::fs::read_to_string(&abs_path) else {
                    return;
                };
                let Some(facts) =
                    extractor.extract_file_facts(repo_root, &abs_path, &source_text, &mut caches)
                else {
                    return;
                };
                facts_by_file.insert(facts.abs_path_posix.clone(), facts);
            });
    });
    index_build::build_route_index(&facts_by_file)
}

pub fn discover_tests_for_http_paths(
    repo_root: &Path,
    http_paths: &[String],
    exclude_globs: &[String],
) -> Vec<String> {
    scan::discover_tests_for_http_paths(repo_root, http_paths, exclude_globs)
}
