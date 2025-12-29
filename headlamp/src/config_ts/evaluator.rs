use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxc_ast::ast::{
    BindingPatternKind, Declaration, ExportDefaultDeclaration, ExportDefaultDeclarationKind,
    ExportNamedDeclaration, ImportDeclaration, ImportDeclarationSpecifier, Program, Statement,
};
use oxc_resolver::Resolver;

use crate::error::HeadlampError;

use super::types::{ImportBinding, ModuleCacheEntry};
use super::utils::module_export_name_as_str;

#[derive(Debug)]
pub(super) struct ModuleEvaluator<'a> {
    pub(super) path: &'a Path,
    pub(super) program: &'a Program<'a>,
    pub(super) resolver: Resolver,
    pub(super) const_inits: HashMap<String, &'a oxc_ast::ast::Expression<'a>>,
    pub(super) imports: HashMap<String, ImportBinding>,
}

impl<'a> ModuleEvaluator<'a> {
    pub(super) fn new(path: &'a Path, program: &'a Program<'a>, resolver: Resolver) -> Self {
        Self {
            path,
            program,
            resolver,
            const_inits: HashMap::new(),
            imports: HashMap::new(),
        }
    }

    pub(super) fn eval_module(
        &mut self,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<Arc<ModuleCacheEntry>, HeadlampError> {
        self.collect_bindings()?;

        let mut exports: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        let mut default_export: Option<serde_json::Value> = None;

        for statement in &self.program.body {
            match statement {
                Statement::ExportDefaultDeclaration(it) => {
                    default_export = Some(self.eval_export_default(it, cache, stack)?);
                }
                Statement::ExportNamedDeclaration(it) => {
                    self.eval_export_named(it, &mut exports, cache, stack)?;
                }
                _ => {}
            }
        }

        Ok(Arc::new(ModuleCacheEntry {
            exports,
            default_export,
        }))
    }

    fn collect_bindings(&mut self) -> Result<(), HeadlampError> {
        for statement in &self.program.body {
            match statement {
                Statement::ImportDeclaration(it) => self.collect_import(it)?,
                Statement::VariableDeclaration(var) => self.collect_var_decl(var),
                Statement::ExportNamedDeclaration(it) => {
                    if let Some(Declaration::VariableDeclaration(var)) = &it.declaration {
                        self.collect_var_decl(var);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn collect_var_decl(&mut self, var: &'a oxc_ast::ast::VariableDeclaration<'a>) {
        for declarator in &var.declarations {
            let Some(init) = declarator.init.as_ref() else {
                continue;
            };
            let BindingPatternKind::BindingIdentifier(ident) = &declarator.id.kind else {
                continue;
            };
            self.const_inits
                .insert(ident.name.as_str().to_string(), init);
        }
    }

    fn collect_import(&mut self, it: &'a ImportDeclaration<'a>) -> Result<(), HeadlampError> {
        let spec = it.source.value.as_str();
        if !(spec.starts_with("./") || spec.starts_with("../")) {
            return Err(
                self.unsupported("only relative imports are supported in headlamp.config.ts")
            );
        }

        let from_dir = self.path.parent().unwrap_or_else(|| Path::new("."));
        let resolved = self
            .resolver
            .resolve(from_dir, spec)
            .map_err(|e| HeadlampError::ConfigParse {
                path: self.path.to_path_buf(),
                message: format!("failed to resolve import {spec:?}: {e}"),
            })?
            .full_path();

        let Some(specifiers) = it.specifiers.as_ref() else {
            return Ok(());
        };

        for s in specifiers {
            match s {
                ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
                    let local = specifier.local.name.as_str().to_string();
                    let imported = specifier.imported.name().to_string();
                    self.imports.insert(
                        local,
                        ImportBinding::Named {
                            from: resolved.clone(),
                            export: imported,
                        },
                    );
                }
                ImportDeclarationSpecifier::ImportDefaultSpecifier(specifier) => {
                    let local = specifier.local.name.as_str().to_string();
                    self.imports.insert(
                        local,
                        ImportBinding::Default {
                            from: resolved.clone(),
                        },
                    );
                }
                _ => {
                    return Err(
                        self.unsupported("unsupported import specifier in headlamp.config.ts")
                    );
                }
            }
        }

        Ok(())
    }

    fn eval_export_default(
        &mut self,
        it: &'a ExportDefaultDeclaration<'a>,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        self.eval_export_default_kind(&it.declaration, cache, stack)
    }

    fn eval_export_default_kind(
        &mut self,
        kind: &'a ExportDefaultDeclarationKind<'a>,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        match kind {
            ExportDefaultDeclarationKind::FunctionDeclaration(_)
            | ExportDefaultDeclarationKind::ClassDeclaration(_)
            | ExportDefaultDeclarationKind::TSInterfaceDeclaration(_) => {
                Err(self.unsupported("unsupported export default declaration kind"))
            }
            ExportDefaultDeclarationKind::ObjectExpression(obj) => {
                self.eval_object_expression(obj, cache, stack)
            }
            ExportDefaultDeclarationKind::ArrayExpression(arr) => {
                self.eval_array_expression(arr, cache, stack)
            }
            ExportDefaultDeclarationKind::Identifier(ident) => {
                self.eval_identifier_value(ident.name.as_str(), cache, stack)
            }
            ExportDefaultDeclarationKind::CallExpression(call) => {
                self.eval_call_expression(&call.callee, &call.arguments, cache, stack)
            }
            ExportDefaultDeclarationKind::TSAsExpression(it) => {
                self.eval_expression_to_value(&it.expression, cache, stack)
            }
            ExportDefaultDeclarationKind::TSSatisfiesExpression(it) => {
                self.eval_expression_to_value(&it.expression, cache, stack)
            }
            ExportDefaultDeclarationKind::ParenthesizedExpression(it) => {
                self.eval_expression_to_value(&it.expression, cache, stack)
            }
            ExportDefaultDeclarationKind::StringLiteral(lit) => {
                Ok(serde_json::Value::String(lit.value.to_string()))
            }
            ExportDefaultDeclarationKind::NumericLiteral(lit) => {
                Ok(super::utils::json_number_from_f64(lit.value))
            }
            ExportDefaultDeclarationKind::BooleanLiteral(lit) => {
                Ok(serde_json::Value::Bool(lit.value))
            }
            ExportDefaultDeclarationKind::NullLiteral(_) => Ok(serde_json::Value::Null),
            ExportDefaultDeclarationKind::UnaryExpression(unary) => {
                self.eval_unary_expression(unary.operator, &unary.argument, cache, stack)
            }
            _ => Err(self.unsupported("unsupported export default expression")),
        }
    }

    fn eval_export_named(
        &mut self,
        it: &'a ExportNamedDeclaration<'a>,
        exports: &mut BTreeMap<String, serde_json::Value>,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<(), HeadlampError> {
        if let Some(decl) = &it.declaration {
            match decl {
                Declaration::VariableDeclaration(var) => {
                    for declarator in &var.declarations {
                        let Some(init) = declarator.init.as_ref() else {
                            continue;
                        };
                        let BindingPatternKind::BindingIdentifier(ident) = &declarator.id.kind
                        else {
                            return Err(self.unsupported("unsupported export const pattern"));
                        };
                        let name = ident.name.as_str().to_string();
                        let value = self.eval_expression_to_value(init, cache, stack)?;
                        exports.insert(name, value);
                    }
                    return Ok(());
                }
                _ => return Err(self.unsupported("unsupported export named declaration")),
            }
        }

        for spec in &it.specifiers {
            let Some(local) = module_export_name_as_str(&spec.local) else {
                return Err(self.unsupported("unsupported export specifier local name"));
            };
            let Some(exported) = module_export_name_as_str(&spec.exported) else {
                return Err(self.unsupported("unsupported export specifier exported name"));
            };
            let value = self.eval_identifier_value(local, cache, stack)?;
            exports.insert(exported.to_string(), value);
        }

        Ok(())
    }

    pub(super) fn unsupported(&self, message: &str) -> HeadlampError {
        HeadlampError::ConfigParse {
            path: self.path.to_path_buf(),
            message: format!("headlamp.config.ts: {message}"),
        }
    }
}
