use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::selection::routes::prefilter_rg;
use crate::selection::routes::types::{FileRouteFacts, LocalRoute, RouteFrameworkId};
use crate::selection::routes::{RouteExtractor, RouteExtractorCaches};

mod collector;

const CANDIDATE_FILE_GLOBS: [&str; 1] = ["**/*.{ts,tsx,js,jsx,mjs,cjs}"];

const RG_FIXED_STRING_TOKENS: [&str; 10] = [
    "express(",
    "express.Router(",
    "Router(",
    ".use(",
    ".route(",
    ".get(",
    ".post(",
    ".put(",
    ".delete(",
    ".patch(",
];

#[derive(Debug, Default)]
pub struct ExpressRouteExtractor;

impl RouteExtractor for ExpressRouteExtractor {
    fn framework_id(&self) -> RouteFrameworkId {
        RouteFrameworkId::Express
    }

    fn candidate_file_globs(&self) -> &'static [&'static str] {
        &CANDIDATE_FILE_GLOBS
    }

    fn rg_fixed_string_tokens(&self) -> &'static [&'static str] {
        &RG_FIXED_STRING_TOKENS
    }

    fn extract_file_facts(
        &self,
        repo_root: &Path,
        abs_path: &Path,
        source_text: &str,
        caches: &mut RouteExtractorCaches,
    ) -> Option<FileRouteFacts> {
        let source_type = SourceType::from_path(abs_path).unwrap_or_default();
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source_text, source_type).parse();
        let program = parsed.program;

        let mut collector = collector::ExpressFactsCollector::default();
        collector.visit_program(&program);

        let exports_router = collector
            .exported_identifiers
            .iter()
            .any(|name| collector.router_containers.contains(name));

        let mut root_mounts = vec![];
        let mut router_mounts = vec![];
        collector::resolve_mount_descriptors(
            repo_root,
            abs_path,
            &collector.import_bindings,
            &collector.mounts,
            &mut caches.ts_js,
            &mut root_mounts,
            &mut router_mounts,
        );

        let abs_path_posix = prefilter_rg::normalize_abs_posix(abs_path);
        let facts = FileRouteFacts {
            abs_path_posix,
            has_root_container: !collector.app_containers.is_empty(),
            exports_router,
            root_routes: collector
                .app_routes
                .into_iter()
                .map(|path| LocalRoute { path })
                .collect(),
            router_routes: collector
                .router_routes
                .into_iter()
                .map(|path| LocalRoute { path })
                .collect(),
            root_mounts,
            router_mounts,
        };
        (!facts.is_empty()).then_some(facts)
    }
}
