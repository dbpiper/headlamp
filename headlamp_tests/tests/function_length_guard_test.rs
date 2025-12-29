use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use headlamp_tests::guards::workspace_scan::list_workspace_rust_files;
use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::visit::Visit;

#[derive(Debug, Clone)]
struct FunctionLocation {
    file_path: PathBuf,
    function_label: String,
    start_line: usize,
    end_line: usize,
    body_physical_lines_excluding_signature_line: usize,
}

#[derive(Debug, Clone, Copy)]
struct GuardConfig {
    max_body_lines: usize,
}

struct FileFunctionCollector<'a> {
    file_path: &'a Path,
    file_contents: &'a str,
    current_scope: Vec<String>,
    functions_by_label: BTreeMap<String, FunctionLocation>,
}

impl<'a> FileFunctionCollector<'a> {
    fn scoped_label(&self, function_name: &str) -> String {
        let mut parts = self.current_scope.clone();
        parts.push(function_name.to_string());
        parts.join("::")
    }

    fn record_function(&mut self, function_name: &str, block_span: Span) {
        let Some((start_line, end_line, body_lines)) =
            compute_body_physical_lines_excluding_signature(self.file_contents, block_span)
        else {
            return;
        };
        let label = self.scoped_label(function_name);
        self.functions_by_label.insert(
            label.clone(),
            FunctionLocation {
                file_path: self.file_path.to_path_buf(),
                function_label: label,
                start_line,
                end_line,
                body_physical_lines_excluding_signature_line: body_lines,
            },
        );
    }
}

impl<'a, 'ast> Visit<'ast> for FileFunctionCollector<'a> {
    fn visit_item_mod(&mut self, module_item: &'ast syn::ItemMod) {
        let module_name = module_item.ident.to_string();
        self.current_scope.push(module_name);
        syn::visit::visit_item_mod(self, module_item);
        self.current_scope.pop();
    }

    fn visit_item_impl(&mut self, impl_item: &'ast syn::ItemImpl) {
        let impl_label = impl_label_for(impl_item.self_ty.as_ref());
        self.current_scope.push(impl_label);
        syn::visit::visit_item_impl(self, impl_item);
        self.current_scope.pop();
    }

    fn visit_item_fn(&mut self, function_item: &'ast syn::ItemFn) {
        let function_name = function_item.sig.ident.to_string();
        self.record_function(&function_name, function_item.block.span());
        syn::visit::visit_item_fn(self, function_item);
    }

    fn visit_impl_item_fn(&mut self, method_item: &'ast syn::ImplItemFn) {
        let function_name = method_item.sig.ident.to_string();
        self.record_function(&function_name, method_item.block.span());
        syn::visit::visit_impl_item_fn(self, method_item);
    }

    fn visit_trait_item_fn(&mut self, trait_function_item: &'ast syn::TraitItemFn) {
        let function_name = trait_function_item.sig.ident.to_string();
        if let Some(block) = &trait_function_item.default {
            self.record_function(&function_name, block.span());
        }
        syn::visit::visit_trait_item_fn(self, trait_function_item);
    }
}

fn read_file_to_string(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|err| panic!("failed reading {path:?}: {err}"))
}

fn span_start_end_lines(span: Span) -> Option<(usize, usize)> {
    let start = span.start();
    let end = span.end();
    if start.line == 0 || end.line == 0 {
        return None;
    }
    Some((start.line, end.line))
}

fn compute_body_physical_lines_excluding_signature(
    file_contents: &str,
    block_span: Span,
) -> Option<(usize, usize, usize)> {
    let (start_line, end_line) = span_start_end_lines(block_span)?;
    if end_line < start_line {
        return None;
    }
    let total_block_lines_including_braces = end_line - start_line + 1;
    let start_line_text = file_contents
        .lines()
        .nth(start_line.saturating_sub(1))
        .unwrap_or("");
    let signature_is_on_block_start_line = start_line_text.contains("fn ")
        || start_line_text.contains("async fn ")
        || start_line_text.contains("const fn ");
    let excluded_lines = usize::from(signature_is_on_block_start_line);
    let body_lines = total_block_lines_including_braces.saturating_sub(excluded_lines);
    Some((start_line, end_line, body_lines))
}

fn impl_label_for(self_ty: &syn::Type) -> String {
    match self_ty {
        syn::Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|seg| seg.ident.to_string())
            .unwrap_or_else(|| "impl".to_string()),
        _ => "impl".to_string(),
    }
}

fn collect_functions_in_file(file_path: &Path) -> Vec<FunctionLocation> {
    let file_contents = read_file_to_string(file_path);
    let syntax = syn::parse_file(&file_contents)
        .unwrap_or_else(|err| panic!("failed parsing {file_path:?} as Rust: {err}"));

    let mut visitor = FileFunctionCollector {
        file_path,
        file_contents: &file_contents,
        current_scope: vec![],
        functions_by_label: BTreeMap::new(),
    };
    visitor.visit_file(&syntax);
    visitor.functions_by_label.into_values().collect()
}

fn find_function_length_violations(cfg: GuardConfig) -> Vec<FunctionLocation> {
    let candidate_files = list_workspace_rust_files();
    let mut all_functions = candidate_files
        .iter()
        .flat_map(|file_path| collect_functions_in_file(file_path))
        .collect::<Vec<_>>();

    all_functions.sort_by(|left, right| {
        right
            .body_physical_lines_excluding_signature_line
            .cmp(&left.body_physical_lines_excluding_signature_line)
            .then_with(|| left.file_path.cmp(&right.file_path))
            .then_with(|| left.function_label.cmp(&right.function_label))
    });

    all_functions
        .into_iter()
        .filter(|loc| loc.body_physical_lines_excluding_signature_line > cfg.max_body_lines)
        .collect()
}

fn format_violation(location: &FunctionLocation) -> String {
    format!(
        "{} -> {} lines -> {}:{}-{}",
        location.function_label,
        location.body_physical_lines_excluding_signature_line,
        location.file_path.display(),
        location.start_line,
        location.end_line
    )
}

#[test]
fn rust_functions_do_not_exceed_max_physical_lines() {
    let cfg = GuardConfig { max_body_lines: 70 };
    let violations = find_function_length_violations(cfg);
    let rendered = violations.iter().map(format_violation).collect::<Vec<_>>();
    assert!(
        rendered.is_empty(),
        "found {} functions over limit ({}):\n{}",
        rendered.len(),
        cfg.max_body_lines,
        rendered.join("\n")
    );
}
