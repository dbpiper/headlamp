use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    CallExpression, ExportAllDeclaration, ExportNamedDeclaration, ImportDeclaration,
    ImportExpression,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::SourceType;

const FILE_EXTS: [&str; 10] = [
    "", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts", ".json",
];

pub fn extract_import_specs(abs_path: &Path) -> Vec<String> {
    if !abs_path.exists() {
        return vec![];
    }
    let Ok(source_text) = std::fs::read_to_string(abs_path) else {
        return vec![];
    };
    let source_type = SourceType::from_path(abs_path).unwrap_or_default();
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, &source_text, source_type).parse();

    let program = ret.program;
    let mut collector = ImportSpecCollector::default();
    collector.visit_program(&program);
    collector.into_sorted_vec()
}

pub fn resolve_import_with_root(from_file: &Path, spec: &str, root_dir: &Path) -> Option<PathBuf> {
    let mut cache = crate::selection::deps::ts_js_resolver::TsJsResolveCache::default();
    crate::selection::deps::ts_js_resolver::TsJsImportResolver::new(root_dir)
        .resolve_import(from_file, spec, &mut cache)
}

pub fn looks_like_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            matches!(
                ext,
                "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts"
            )
        })
        .unwrap_or(false)
}

pub fn build_seed_terms(
    repo_root: &Path,
    production_selection_paths_abs: &[String],
) -> Vec<String> {
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    production_selection_paths_abs.iter().for_each(|abs| {
        let abs_path = PathBuf::from(abs);
        let Ok(rel) = abs_path.strip_prefix(repo_root) else {
            return;
        };
        let Some(rel_text) = rel.to_str().map(|s| s.replace('\\', "/")) else {
            return;
        };
        let without_ext = strip_ts_like_extension(&rel_text);
        if without_ext.is_empty() {
            return;
        }
        let base = Path::new(&without_ext)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let last_two = last_two_segments(&without_ext);
        [without_ext, base, last_two]
            .into_iter()
            .filter(|s| !s.is_empty())
            .for_each(|s| {
                out.insert(s);
            });
    });
    out.into_iter().collect()
}

fn strip_ts_like_extension(input: &str) -> String {
    let lowered = input.to_lowercase();
    for ext in [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts"] {
        if lowered.ends_with(ext) {
            return input[..input.len().saturating_sub(ext.len())].to_string();
        }
    }
    input.to_string()
}

fn last_two_segments(path_text: &str) -> String {
    let segs = path_text
        .split('/')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if segs.len() < 2 {
        return String::new();
    }
    format!("{}/{}", segs[segs.len() - 2], segs[segs.len() - 1])
}

pub fn try_resolve_file(candidate_base: &Path) -> Option<PathBuf> {
    FILE_EXTS
        .iter()
        .filter_map(|ext| {
            let full = if ext.is_empty() {
                candidate_base.to_path_buf()
            } else {
                PathBuf::from(format!("{}{}", candidate_base.to_string_lossy(), ext))
            };
            full.exists()
                .then(|| std::fs::metadata(&full).ok().map(|m| (full, m)))
                .flatten()
                .and_then(|(path, meta)| meta.is_file().then_some(path))
        })
        .find_map(|full| dunce::canonicalize(&full).ok().or(Some(full)))
        .or_else(|| try_resolve_index_file(candidate_base))
}

fn try_resolve_index_file(candidate_base: &Path) -> Option<PathBuf> {
    FILE_EXTS
        .iter()
        .filter_map(|ext| {
            let index_name = if ext.is_empty() {
                "index".to_string()
            } else {
                format!("index{ext}")
            };
            let full = candidate_base.join(index_name);
            full.exists()
                .then(|| std::fs::metadata(&full).ok().map(|m| (full, m)))
                .flatten()
                .and_then(|(path, meta)| meta.is_file().then_some(path))
        })
        .find_map(|full| dunce::canonicalize(&full).ok().or(Some(full)))
}

#[derive(Debug, Default)]
struct ImportSpecCollector {
    specs: std::collections::BTreeSet<String>,
}

impl ImportSpecCollector {
    fn into_sorted_vec(self) -> Vec<String> {
        self.specs.into_iter().collect()
    }

    fn push_string_literal(&mut self, lit: &oxc_ast::ast::StringLiteral<'_>) {
        self.specs.insert(lit.value.to_string());
    }
}

impl<'a> Visit<'a> for ImportSpecCollector {
    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        self.push_string_literal(&it.source);
        walk::walk_import_declaration(self, it);
    }

    fn visit_export_all_declaration(&mut self, it: &ExportAllDeclaration<'a>) {
        self.push_string_literal(&it.source);
        walk::walk_export_all_declaration(self, it);
    }

    fn visit_export_named_declaration(&mut self, it: &ExportNamedDeclaration<'a>) {
        if let Some(source) = it.source.as_ref() {
            self.push_string_literal(source);
        }
        walk::walk_export_named_declaration(self, it);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        if let oxc_ast::ast::Expression::StringLiteral(lit) = &it.source {
            self.push_string_literal(lit);
        }
        walk::walk_import_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        let is_require = matches!(
            &it.callee,
            oxc_ast::ast::Expression::Identifier(ident) if ident.name.as_str() == "require"
        );
        if is_require
            && let Some(first) = it.arguments.first()
            && let oxc_ast::ast::Argument::StringLiteral(lit) = first
        {
            self.push_string_literal(lit);
        };
        walk::walk_call_expression(self, it);
    }
}
