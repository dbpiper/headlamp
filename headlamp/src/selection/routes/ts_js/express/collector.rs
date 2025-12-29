use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use oxc_ast::ast::{
    Argument, AssignmentExpression, AssignmentTarget, BindingPattern, BindingPatternKind,
    BindingProperty, CallExpression, ExportDefaultDeclaration, ExportNamedDeclaration, Expression,
    ImportDeclaration, ImportDeclarationSpecifier, ModuleExportName, VariableDeclarator,
};
use oxc_ast_visit::{Visit, walk};

use crate::selection::deps::ts_js_resolver::{TsJsImportResolver, TsJsResolveCache};
use crate::selection::routes::prefilter_rg;
use crate::selection::routes::types::MountEdge;

#[derive(Debug, Clone)]
pub(super) enum MountTarget {
    LocalIdentifier(String),
    RequireSpecifier(String),
}

#[derive(Debug, Clone)]
pub(super) struct MountDescriptor {
    pub(super) is_app: bool,
    pub(super) base_path: String,
    pub(super) target: MountTarget,
}

#[derive(Debug, Default)]
pub(super) struct ExpressFactsCollector {
    pub(super) app_containers: BTreeSet<String>,
    pub(super) router_containers: BTreeSet<String>,
    pub(super) import_bindings: BTreeMap<String, String>,
    pub(super) exported_identifiers: BTreeSet<String>,

    pub(super) app_routes: Vec<String>,
    pub(super) router_routes: Vec<String>,
    pub(super) mounts: Vec<MountDescriptor>,
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

pub(super) fn resolve_mount_descriptors(
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
    let callee_name = match &call.callee {
        Expression::Identifier(ident) => ident.name.as_str(),
        _ => return false,
    };
    callee_name == "express"
}

fn is_express_router_initializer<'a>(expr: &'a Expression<'a>) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let member = match &call.callee {
        Expression::StaticMemberExpression(member) => member,
        _ => return false,
    };
    if member.property.name.as_str() != "Router" {
        return false;
    }
    match &member.object {
        Expression::Identifier(ident) => ident.name.as_str() == "express",
        _ => false,
    }
}

fn require_string_specifier<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    let Expression::CallExpression(call) = expr else {
        return None;
    };
    require_string_specifier_from_call(call)
}

fn require_string_specifier_from_call<'a>(call: &'a CallExpression<'a>) -> Option<&'a str> {
    let callee_ident = match &call.callee {
        Expression::Identifier(ident) => ident.name.as_str(),
        _ => return None,
    };
    if callee_ident != "require" {
        return None;
    }
    first_string_literal(&call.arguments)
}

fn direct_member_container_and_method<'a>(expr: &'a Expression<'a>) -> Option<(&'a str, &'a str)> {
    let member = match expr {
        Expression::StaticMemberExpression(member) => member,
        _ => return None,
    };
    let container_ident = match &member.object {
        Expression::Identifier(ident) => ident.name.as_str(),
        _ => return None,
    };
    let method = member.property.name.as_str();
    Some((container_ident, method))
}

fn member_property_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        _ => None,
    }
}

fn first_string_literal<'a>(arguments: &'a [Argument<'a>]) -> Option<&'a str> {
    match arguments.first()? {
        Argument::StringLiteral(lit) => Some(lit.value.as_str()),
        _ => None,
    }
}

fn argument_string_literal<'a>(argument: &'a Argument<'a>) -> Option<&'a str> {
    match argument {
        Argument::StringLiteral(lit) => Some(lit.value.as_str()),
        _ => None,
    }
}

fn is_http_method(method: &str) -> bool {
    matches!(
        method,
        "get" | "post" | "put" | "delete" | "patch" | "head" | "options" | "all"
    )
}

fn normalize_http_path(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let trimmed = trimmed.strip_prefix('/').unwrap_or(trimmed);
    format!("/{}", trimmed.trim_end_matches('/'))
}

fn module_export_name_identifier<'a>(name: &'a ModuleExportName<'a>) -> Option<&'a str> {
    match name {
        ModuleExportName::IdentifierName(it) => Some(it.name.as_str()),
        ModuleExportName::IdentifierReference(it) => Some(it.name.as_str()),
        _ => None,
    }
}

fn cjs_exported_identifier<'a>(it: &'a AssignmentExpression<'a>) -> Option<&'a str> {
    let left = match &it.left {
        AssignmentTarget::StaticMemberExpression(member) => member,
        _ => return None,
    };
    let left_obj = match &left.object {
        Expression::Identifier(id) => id.name.as_str(),
        _ => return None,
    };
    if left_obj != "module" {
        return None;
    }
    if left.property.name.as_str() != "exports" {
        return None;
    }
    let right = match &it.right {
        Expression::Identifier(id) => id.name.as_str(),
        _ => return None,
    };
    Some(right)
}
