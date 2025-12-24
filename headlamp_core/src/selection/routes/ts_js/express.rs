use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, AssignmentExpression, AssignmentTarget, BindingPattern, BindingPatternKind,
    BindingProperty, CallExpression, ExportDefaultDeclaration, ExportNamedDeclaration, Expression,
    ImportDeclaration, ImportDeclarationSpecifier, ModuleExportName, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::selection::deps::ts_js_resolver::{TsJsImportResolver, TsJsResolveCache};
use crate::selection::routes::prefilter_rg;
use crate::selection::routes::types::{FileRouteFacts, LocalRoute, MountEdge, RouteFrameworkId};
use crate::selection::routes::{RouteExtractor, RouteExtractorCaches};

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

        let mut collector = ExpressFactsCollector::default();
        collector.visit_program(&program);

        let exports_router = collector
            .exported_identifiers
            .iter()
            .any(|name| collector.router_containers.contains(name));

        let mut root_mounts = vec![];
        let mut router_mounts = vec![];
        resolve_mount_descriptors(
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

#[derive(Debug, Clone)]
enum MountTarget {
    LocalIdentifier(String),
    RequireSpecifier(String),
}

#[derive(Debug, Clone)]
struct MountDescriptor {
    is_app: bool,
    base_path: String,
    target: MountTarget,
}

#[derive(Debug, Default)]
struct ExpressFactsCollector {
    app_containers: BTreeSet<String>,
    router_containers: BTreeSet<String>,
    import_bindings: BTreeMap<String, String>,
    exported_identifiers: BTreeSet<String>,

    app_routes: Vec<String>,
    router_routes: Vec<String>,
    mounts: Vec<MountDescriptor>,
}

impl ExpressFactsCollector {
    fn record_exported_ident(&mut self, exported_name: &str) {
        if exported_name.trim().is_empty() {
            return;
        }
        self.exported_identifiers.insert(exported_name.to_string());
    }

    fn record_import_binding(&mut self, local_name: &str, specifier: &str) {
        if local_name.trim().is_empty() || specifier.trim().is_empty() {
            return;
        }
        self.import_bindings
            .insert(local_name.to_string(), specifier.to_string());
    }

    fn record_route(&mut self, is_app: bool, raw_path: &str) {
        let normalized = normalize_http_path(raw_path);
        if normalized.is_empty() {
            return;
        }
        if is_app {
            self.app_routes.push(normalized);
        } else {
            self.router_routes.push(normalized);
        }
    }

    fn record_mount(&mut self, is_app: bool, base_path: &str, target: MountTarget) {
        let normalized_base_path = normalize_http_path(base_path);
        let base = if normalized_base_path.is_empty() {
            "/".to_string()
        } else {
            normalized_base_path
        };
        self.mounts.push(MountDescriptor {
            is_app,
            base_path: base,
            target,
        });
    }
}

impl<'a> Visit<'a> for ExpressFactsCollector {
    fn visit_variable_declarator(&mut self, it: &VariableDeclarator<'a>) {
        let Some(init) = it.init.as_ref() else {
            return;
        };

        if let Some(name) = declared_ident_name(&it.id) {
            if is_express_app_initializer(init) {
                self.app_containers.insert(name.to_string());
            }
            if is_express_router_initializer(init) {
                self.router_containers.insert(name.to_string());
            }

            if let Some(require_spec) = require_string_specifier(init) {
                self.record_import_binding(name, require_spec);
            }
        }

        if let Some((require_spec, locals)) = require_destructure_bindings(&it.id, init) {
            locals.iter().for_each(|local| {
                self.record_import_binding(local, &require_spec);
            });
        }

        walk::walk_variable_declarator(self, it);
    }

    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        let specifier = it.source.value.as_str();
        if let Some(specifiers) = it.specifiers.as_ref() {
            specifiers.iter().for_each(|spec| match spec {
                ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                    self.record_import_binding(s.local.name.as_str(), specifier);
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                    self.record_import_binding(s.local.name.as_str(), specifier);
                }
                ImportDeclarationSpecifier::ImportSpecifier(s) => {
                    self.record_import_binding(s.local.name.as_str(), specifier);
                }
            });
        }
        walk::walk_import_declaration(self, it);
    }

    fn visit_export_named_declaration(&mut self, it: &ExportNamedDeclaration<'a>) {
        it.specifiers.iter().for_each(|spec| {
            module_export_name_identifier(&spec.local)
                .iter()
                .for_each(|name| {
                    self.record_exported_ident(name);
                });
        });
        walk::walk_export_named_declaration(self, it);
    }

    fn visit_export_default_declaration(&mut self, it: &ExportDefaultDeclaration<'a>) {
        if let oxc_ast::ast::ExportDefaultDeclarationKind::Identifier(ident) = &it.declaration {
            self.record_exported_ident(ident.name.as_str());
        }
        walk::walk_export_default_declaration(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if let Some(exported_ident) = cjs_exported_identifier(it) {
            self.record_exported_ident(exported_ident);
        }
        walk::walk_assignment_expression(self, it);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Some(route) = extract_route_call(it) {
            let is_app = self.app_containers.contains(route.container);
            let is_router = self.router_containers.contains(route.container);
            if is_app || is_router {
                self.record_route(is_app, route.path);
            }
        }

        if let Some(mount) = extract_use_call(it) {
            let is_app = self.app_containers.contains(mount.container);
            let is_router = self.router_containers.contains(mount.container);
            if is_app || is_router {
                self.record_mount(is_app, mount.base_path, mount.target.clone());
            }
        }

        walk::walk_call_expression(self, it);
    }
}

struct RouteCall<'a> {
    container: &'a str,
    path: &'a str,
}

fn extract_route_call<'a>(call: &'a CallExpression<'a>) -> Option<RouteCall<'a>> {
    let method = member_property_name(&call.callee)?;
    if !is_http_method(method) {
        return None;
    }

    if let Some((container, path)) =
        extract_direct_container_and_path(&call.callee, &call.arguments)
    {
        return Some(RouteCall { container, path });
    }
    extract_route_chain_container_and_path(&call.callee)
        .map(|(container, path)| RouteCall { container, path })
}

fn extract_direct_container_and_path<'a>(
    callee: &'a Expression<'a>,
    arguments: &'a [Argument<'a>],
) -> Option<(&'a str, &'a str)> {
    let (container, method) = direct_member_container_and_method(callee)?;
    if !is_http_method(method) {
        return None;
    }
    let path = first_string_literal(arguments)?;
    Some((container, path))
}

fn extract_route_chain_container_and_path<'a>(
    callee: &'a Expression<'a>,
) -> Option<(&'a str, &'a str)> {
    let member = match callee {
        Expression::StaticMemberExpression(member) => member,
        _ => return None,
    };
    let method = member.property.name.as_str();
    if !is_http_method(method) {
        return None;
    }
    let inner_call = match &member.object {
        Expression::CallExpression(call) => call,
        _ => return None,
    };
    let (container, inner_method) = direct_member_container_and_method(&inner_call.callee)?;
    if inner_method != "route" {
        return None;
    }
    let path = first_string_literal(&inner_call.arguments)?;
    Some((container, path))
}

struct UseCall<'a> {
    container: &'a str,
    base_path: &'a str,
    target: MountTarget,
}

fn extract_use_call<'a>(call: &'a CallExpression<'a>) -> Option<UseCall<'a>> {
    let (container, method) = direct_member_container_and_method(&call.callee)?;
    if method != "use" {
        return None;
    }
    let (base_path, handler_arg) = split_use_args(&call.arguments)?;
    let target = extract_mount_target(handler_arg)?;
    Some(UseCall {
        container,
        base_path,
        target,
    })
}

fn split_use_args<'a>(arguments: &'a [Argument<'a>]) -> Option<(&'a str, &'a Argument<'a>)> {
    match arguments {
        [first, second, ..] => {
            if let Some(base) = argument_string_literal(first) {
                Some((base, second))
            } else {
                Some(("/", first))
            }
        }
        [only] => Some(("/", only)),
        _ => None,
    }
}

fn extract_mount_target<'a>(argument: &'a Argument<'a>) -> Option<MountTarget> {
    match argument {
        Argument::Identifier(ident) => Some(MountTarget::LocalIdentifier(
            ident.name.as_str().to_string(),
        )),
        Argument::CallExpression(call) => {
            if let Some(require_spec) = require_string_specifier_from_call(call) {
                return Some(MountTarget::RequireSpecifier(require_spec.to_string()));
            }
            let callee_ident = match &call.callee {
                Expression::Identifier(ident) => ident.name.as_str(),
                _ => return None,
            };
            Some(MountTarget::LocalIdentifier(callee_ident.to_string()))
        }
        _ => None,
    }
}

fn resolve_mount_descriptors(
    repo_root: &Path,
    from_file: &Path,
    bindings_by_local: &BTreeMap<String, String>,
    mounts: &[MountDescriptor],
    cache: &mut TsJsResolveCache,
    root_mounts: &mut Vec<MountEdge>,
    router_mounts: &mut Vec<MountEdge>,
) {
    let resolver = TsJsImportResolver::new(repo_root);
    for mount in mounts {
        let resolved_abs = match &mount.target {
            MountTarget::RequireSpecifier(spec) => resolver.resolve_import(from_file, spec, cache),
            MountTarget::LocalIdentifier(local) => bindings_by_local
                .get(local)
                .and_then(|spec| resolver.resolve_import(from_file, spec, cache)),
        };
        let Some(target_abs) = resolved_abs else {
            continue;
        };
        let edge = MountEdge {
            base_path: mount.base_path.clone(),
            target_abs_posix: prefilter_rg::normalize_abs_posix(&target_abs),
        };
        if mount.is_app {
            root_mounts.push(edge);
        } else {
            router_mounts.push(edge);
        }
    }
}

fn declared_ident_name<'a>(pattern: &'a BindingPattern<'a>) -> Option<&'a str> {
    match &pattern.kind {
        BindingPatternKind::BindingIdentifier(ident) => Some(ident.name.as_str()),
        _ => None,
    }
}

fn require_destructure_bindings<'a>(
    pattern: &'a BindingPattern<'a>,
    init: &'a Expression<'a>,
) -> Option<(String, Vec<String>)> {
    let spec = require_string_specifier(init)?.to_string();
    let BindingPatternKind::ObjectPattern(obj) = &pattern.kind else {
        return None;
    };
    let locals = obj
        .properties
        .iter()
        .filter_map(binding_property_local_identifier)
        .collect::<Vec<_>>();
    (!locals.is_empty()).then_some((spec, locals))
}

fn binding_property_local_identifier(binding_property: &BindingProperty<'_>) -> Option<String> {
    match &binding_property.value.kind {
        BindingPatternKind::BindingIdentifier(ident) => Some(ident.name.as_str().to_string()),
        BindingPatternKind::AssignmentPattern(assign) => match &assign.left.kind {
            BindingPatternKind::BindingIdentifier(ident) => Some(ident.name.as_str().to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn is_express_app_initializer<'a>(expr: &'a Expression<'a>) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    matches!(&call.callee, Expression::Identifier(ident) if ident.name.as_str() == "express")
}

fn is_express_router_initializer<'a>(expr: &'a Expression<'a>) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    if matches!(&call.callee, Expression::Identifier(ident) if ident.name.as_str() == "Router") {
        return true;
    }
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let is_express =
        matches!(&member.object, Expression::Identifier(ident) if ident.name.as_str() == "express");
    is_express && member.property.name.as_str() == "Router"
}

fn require_string_specifier<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    let Expression::CallExpression(call) = expr else {
        return None;
    };
    require_string_specifier_from_call(call)
}

fn require_string_specifier_from_call<'a>(call: &'a CallExpression<'a>) -> Option<&'a str> {
    let is_require =
        matches!(&call.callee, Expression::Identifier(ident) if ident.name.as_str() == "require");
    is_require.then_some(())?;
    first_string_literal(&call.arguments)
}

fn cjs_exported_identifier<'a>(it: &'a AssignmentExpression<'a>) -> Option<&'a str> {
    let left = &it.left;
    let right_ident = match &it.right {
        Expression::Identifier(ident) => ident.name.as_str(),
        _ => return None,
    };

    let AssignmentTarget::StaticMemberExpression(member) = left else {
        return None;
    };
    let object_is_exports =
        matches!(&member.object, Expression::Identifier(ident) if ident.name.as_str() == "exports");

    let object_is_module_exports_direct = matches!(
        &member.object,
        Expression::Identifier(ident) if ident.name.as_str() == "module"
    ) && member.property.name.as_str() == "exports";

    let object_is_module_exports_nested = match &member.object {
        Expression::StaticMemberExpression(inner) => {
            matches!(&inner.object, Expression::Identifier(ident) if ident.name.as_str() == "module")
                && inner.property.name.as_str() == "exports"
        }
        _ => false,
    };

    (object_is_exports || object_is_module_exports_direct || object_is_module_exports_nested)
        .then_some(right_ident)
}

fn module_export_name_identifier<'a>(name: &'a ModuleExportName<'a>) -> Option<&'a str> {
    match name {
        ModuleExportName::IdentifierName(ident) => Some(ident.name.as_str()),
        ModuleExportName::IdentifierReference(ident) => Some(ident.name.as_str()),
        ModuleExportName::StringLiteral(_) => None,
    }
}

fn direct_member_container_and_method<'a>(
    callee: &'a Expression<'a>,
) -> Option<(&'a str, &'a str)> {
    let member = match callee {
        Expression::StaticMemberExpression(member) => member,
        _ => return None,
    };
    let container = match &member.object {
        Expression::Identifier(ident) => ident.name.as_str(),
        _ => return None,
    };
    Some((container, member.property.name.as_str()))
}

fn member_property_name<'a>(callee: &'a Expression<'a>) -> Option<&'a str> {
    match callee {
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        _ => None,
    }
}

fn first_string_literal<'a>(arguments: &'a [Argument<'a>]) -> Option<&'a str> {
    arguments.first().and_then(argument_string_literal)
}

fn argument_string_literal<'a>(argument: &'a Argument<'a>) -> Option<&'a str> {
    match argument {
        Argument::StringLiteral(lit) => Some(lit.value.as_str()),
        _ => None,
    }
}

fn is_http_method(name: &str) -> bool {
    matches!(
        name,
        "get" | "post" | "put" | "delete" | "patch" | "options" | "head" | "all"
    )
}

fn normalize_http_path(value: &str) -> String {
    let without_query = value.split('?').next().unwrap_or(value);
    let without_hash = without_query.split('#').next().unwrap_or(without_query);
    let without_origin = strip_http_origin(without_hash);

    let with_leading = if without_origin.starts_with('/') {
        without_origin.to_string()
    } else {
        format!("/{without_origin}")
    };
    collapse_slashes(&with_leading)
}

fn strip_http_origin(input: &str) -> &str {
    let is_http = input.starts_with("http://") || input.starts_with("https://");
    if !is_http {
        return input;
    }
    let Some(after_scheme) = input.find("://").map(|idx| idx + 3) else {
        return input;
    };
    let rest = &input[after_scheme..];
    let Some(first_slash) = rest.find('/') else {
        return "/";
    };
    &rest[first_slash..]
}

fn collapse_slashes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_was_slash = false;
    for ch in input.chars() {
        if ch == '/' {
            if last_was_slash {
                continue;
            }
            last_was_slash = true;
            out.push('/');
            continue;
        }
        last_was_slash = false;
        out.push(ch);
    }
    if out.is_empty() { "/".to_string() } else { out }
}
