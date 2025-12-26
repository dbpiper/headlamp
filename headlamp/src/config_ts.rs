use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, ArrayExpression, ArrayExpressionElement, BindingPatternKind, Declaration,
    ExportDefaultDeclaration, ExportDefaultDeclarationKind, ExportNamedDeclaration, Expression,
    ImportDeclaration, ImportDeclarationSpecifier, ModuleExportName, ObjectExpression,
    ObjectPropertyKind, Program, PropertyKey, Statement, UnaryOperator,
};
use oxc_parser::Parser;
use oxc_resolver::{ResolveOptions, Resolver};
use oxc_span::SourceType;

use crate::error::HeadlampError;

#[derive(Debug, Clone)]
struct ModuleCacheEntry {
    exports: BTreeMap<String, serde_json::Value>,
    default_export: Option<serde_json::Value>,
}

pub fn load_headlamp_config_ts_oxc(path: &Path) -> Result<serde_json::Value, HeadlampError> {
    let mut cache: HashMap<PathBuf, Arc<ModuleCacheEntry>> = HashMap::new();
    let mut stack: Vec<PathBuf> = vec![];
    let entry = load_module_exports(path, &mut cache, &mut stack)?;
    entry
        .default_export
        .clone()
        .ok_or_else(|| HeadlampError::ConfigParse {
            path: path.to_path_buf(),
            message: "headlamp.config.ts: missing default export".to_string(),
        })
}

fn load_module_exports(
    path: &Path,
    cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
    stack: &mut Vec<PathBuf>,
) -> Result<Arc<ModuleCacheEntry>, HeadlampError> {
    let canonical = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(hit) = cache.get(&canonical) {
        return Ok(hit.clone());
    }
    if stack.iter().any(|p| *p == canonical) {
        return Err(HeadlampError::ConfigParse {
            path: canonical,
            message: "headlamp.config.ts: cyclic import in config evaluation".to_string(),
        });
    }
    stack.push(canonical.clone());

    let raw = std::fs::read_to_string(&canonical).map_err(|source| HeadlampError::Io {
        path: canonical.clone(),
        source,
    })?;

    let allocator = Allocator::default();
    let source_type = SourceType::from_path(&canonical).unwrap_or(SourceType::ts());
    let parsed = Parser::new(&allocator, &raw, source_type).parse();
    if parsed.panicked || !parsed.errors.is_empty() {
        let message = parsed
            .errors
            .iter()
            .map(|e| format!("{e:?}"))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(HeadlampError::ConfigParse {
            path: canonical.clone(),
            message: format!("headlamp.config.ts: parse errors\n{message}"),
        });
    }

    let resolver = build_resolver();
    let mut module_eval = ModuleEvaluator::new(&canonical, &parsed.program, resolver);
    let entry = module_eval.eval_module(cache, stack)?;
    cache.insert(canonical.clone(), entry.clone());

    let _ = stack.pop();
    Ok(entry)
}

fn build_resolver() -> Resolver {
    let extensions = [
        ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts", ".json",
    ]
    .into_iter()
    .map(|ext| ext.to_string())
    .collect::<Vec<_>>();
    Resolver::new(ResolveOptions {
        extensions,
        ..Default::default()
    })
}

#[derive(Debug)]
struct ModuleEvaluator<'a> {
    path: &'a Path,
    program: &'a Program<'a>,
    resolver: Resolver,
    const_inits: HashMap<String, &'a Expression<'a>>,
    imports: HashMap<String, ImportBinding>,
}

#[derive(Debug, Clone)]
enum ImportBinding {
    Named { from: PathBuf, export: String },
    Default { from: PathBuf },
}

impl<'a> ModuleEvaluator<'a> {
    fn new(path: &'a Path, program: &'a Program<'a>, resolver: Resolver) -> Self {
        Self {
            path,
            program,
            resolver,
            const_inits: HashMap::new(),
            imports: HashMap::new(),
        }
    }

    fn eval_module(
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
                Ok(json_number_from_f64(lit.value))
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

    fn eval_identifier_value(
        &mut self,
        name: &str,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        if let Some(expr) = self.const_inits.get(name) {
            return self.eval_expression_to_value(expr, cache, stack);
        }
        if let Some(binding) = self.imports.get(name).cloned() {
            let entry = load_module_exports(&binding_path(&binding), cache, stack)?;
            let value = match binding {
                ImportBinding::Named { export, .. } => {
                    entry.exports.get(&export).cloned().ok_or_else(|| {
                        self.unsupported(&format!("missing imported export {export:?}"))
                    })?
                }
                ImportBinding::Default { .. } => entry.default_export.clone().ok_or_else(|| {
                    self.unsupported("missing default export from imported module")
                })?,
            };
            return Ok(value);
        }
        Err(self.unsupported(&format!("unknown identifier: {name}")))
    }

    fn eval_call_expression(
        &mut self,
        callee: &'a Expression<'a>,
        arguments: &'a [Argument<'a>],
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        let Expression::Identifier(callee_ident) = callee else {
            return Err(self.unsupported("unsupported call expression"));
        };
        if callee_ident.name.as_str() != "defineConfig" {
            return Err(self.unsupported("unsupported call expression"));
        }
        let Some(first) = arguments.first() else {
            return Err(self.unsupported("defineConfig requires one argument"));
        };
        self.eval_argument_to_value(first, cache, stack)
    }

    fn eval_argument_to_value(
        &mut self,
        arg: &'a Argument<'a>,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        match arg {
            Argument::SpreadElement(_) => Err(self.unsupported("unsupported spread argument")),
            Argument::ObjectExpression(obj) => self.eval_object_expression(obj, cache, stack),
            Argument::ArrayExpression(arr) => self.eval_array_expression(arr, cache, stack),
            Argument::Identifier(ident) => {
                self.eval_identifier_value(ident.name.as_str(), cache, stack)
            }
            Argument::TSAsExpression(it) => {
                self.eval_expression_to_value(&it.expression, cache, stack)
            }
            Argument::TSSatisfiesExpression(it) => {
                self.eval_expression_to_value(&it.expression, cache, stack)
            }
            Argument::ParenthesizedExpression(it) => {
                self.eval_expression_to_value(&it.expression, cache, stack)
            }
            Argument::StringLiteral(lit) => Ok(serde_json::Value::String(lit.value.to_string())),
            Argument::NumericLiteral(lit) => Ok(json_number_from_f64(lit.value)),
            Argument::BooleanLiteral(lit) => Ok(serde_json::Value::Bool(lit.value)),
            Argument::NullLiteral(_) => Ok(serde_json::Value::Null),
            _ => Err(self.unsupported("unsupported defineConfig argument")),
        }
    }

    fn eval_expression_to_value(
        &mut self,
        expr: &'a Expression<'a>,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        match expr {
            Expression::ObjectExpression(obj) => self.eval_object_expression(obj, cache, stack),
            Expression::ArrayExpression(arr) => self.eval_array_expression(arr, cache, stack),
            Expression::StringLiteral(lit) => Ok(serde_json::Value::String(lit.value.to_string())),
            Expression::NumericLiteral(lit) => Ok(json_number_from_f64(lit.value)),
            Expression::BooleanLiteral(lit) => Ok(serde_json::Value::Bool(lit.value)),
            Expression::NullLiteral(_) => Ok(serde_json::Value::Null),
            Expression::UnaryExpression(unary) => {
                self.eval_unary_expression(unary.operator, &unary.argument, cache, stack)
            }
            Expression::Identifier(ident) => {
                self.eval_identifier_value(ident.name.as_str(), cache, stack)
            }
            Expression::CallExpression(call) => {
                self.eval_call_expression(&call.callee, &call.arguments, cache, stack)
            }
            Expression::TSAsExpression(it) => {
                self.eval_expression_to_value(&it.expression, cache, stack)
            }
            Expression::TSSatisfiesExpression(it) => {
                self.eval_expression_to_value(&it.expression, cache, stack)
            }
            Expression::ParenthesizedExpression(it) => {
                self.eval_expression_to_value(&it.expression, cache, stack)
            }
            _ => Err(self.unsupported("unsupported expression in headlamp.config.ts")),
        }
    }

    fn eval_unary_expression(
        &mut self,
        operator: UnaryOperator,
        argument: &'a Expression<'a>,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        if operator != UnaryOperator::UnaryNegation {
            return Err(self.unsupported("unsupported unary operator"));
        }
        let inner = self.eval_expression_to_value(argument, cache, stack)?;
        let Some(n) = inner.as_f64() else {
            return Err(self.unsupported("unary '-' requires numeric literal"));
        };
        Ok(serde_json::json!(-n))
    }

    fn eval_object_expression(
        &mut self,
        obj: &'a ObjectExpression<'a>,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        let mut map = serde_json::Map::new();
        for prop in &obj.properties {
            match prop {
                ObjectPropertyKind::ObjectProperty(p) => {
                    let key = match &p.key {
                        PropertyKey::StaticIdentifier(ident) => ident.name.as_str().to_string(),
                        PropertyKey::StringLiteral(lit) => lit.value.as_str().to_string(),
                        PropertyKey::NumericLiteral(lit) => lit.value.to_string(),
                        _ => return Err(self.unsupported("unsupported object key")),
                    };
                    let value = self.eval_expression_to_value(&p.value, cache, stack)?;
                    map.insert(key, value);
                }
                ObjectPropertyKind::SpreadProperty(spread) => {
                    let spread_value =
                        self.eval_expression_to_value(&spread.argument, cache, stack)?;
                    let serde_json::Value::Object(obj) = spread_value else {
                        return Err(self.unsupported("object spread must be an object"));
                    };
                    obj.into_iter().for_each(|(k, v)| {
                        map.insert(k, v);
                    });
                }
            }
        }
        Ok(serde_json::Value::Object(map))
    }

    fn eval_array_expression(
        &mut self,
        arr: &'a ArrayExpression<'a>,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        let mut out: Vec<serde_json::Value> = vec![];
        for el in &arr.elements {
            match el {
                ArrayExpressionElement::Elision(_) => {}
                ArrayExpressionElement::SpreadElement(_) => {
                    return Err(self.unsupported("unsupported array spread element"));
                }
                ArrayExpressionElement::ObjectExpression(obj) => {
                    out.push(self.eval_object_expression(obj, cache, stack)?);
                }
                ArrayExpressionElement::ArrayExpression(arr) => {
                    out.push(self.eval_array_expression(arr, cache, stack)?);
                }
                ArrayExpressionElement::Identifier(ident) => {
                    out.push(self.eval_identifier_value(ident.name.as_str(), cache, stack)?);
                }
                ArrayExpressionElement::StringLiteral(lit) => {
                    out.push(serde_json::Value::String(lit.value.to_string()));
                }
                ArrayExpressionElement::NumericLiteral(lit) => {
                    out.push(json_number_from_f64(lit.value));
                }
                ArrayExpressionElement::BooleanLiteral(lit) => {
                    out.push(serde_json::Value::Bool(lit.value));
                }
                ArrayExpressionElement::NullLiteral(_) => out.push(serde_json::Value::Null),
                ArrayExpressionElement::TSAsExpression(it) => {
                    out.push(self.eval_expression_to_value(&it.expression, cache, stack)?);
                }
                ArrayExpressionElement::TSSatisfiesExpression(it) => {
                    out.push(self.eval_expression_to_value(&it.expression, cache, stack)?);
                }
                ArrayExpressionElement::ParenthesizedExpression(it) => {
                    out.push(self.eval_expression_to_value(&it.expression, cache, stack)?);
                }
                _ => return Err(self.unsupported("unsupported array element")),
            }
        }
        Ok(serde_json::Value::Array(out))
    }

    fn unsupported(&self, message: &str) -> HeadlampError {
        HeadlampError::ConfigParse {
            path: self.path.to_path_buf(),
            message: format!("unsupported: {message}"),
        }
    }
}

fn binding_path(binding: &ImportBinding) -> PathBuf {
    match binding {
        ImportBinding::Named { from, .. } => from.clone(),
        ImportBinding::Default { from } => from.clone(),
    }
}

fn module_export_name_as_str<'a>(name: &'a ModuleExportName<'a>) -> Option<&'a str> {
    match name {
        ModuleExportName::IdentifierName(ident) => Some(ident.name.as_str()),
        ModuleExportName::IdentifierReference(ident) => Some(ident.name.as_str()),
        ModuleExportName::StringLiteral(lit) => Some(lit.value.as_str()),
    }
}

fn json_number_from_f64(value: f64) -> serde_json::Value {
    if value.is_finite() && value.fract() == 0.0 {
        if value >= i64::MIN as f64 && value <= i64::MAX as f64 {
            return serde_json::Value::Number(serde_json::Number::from(value as i64));
        }
    }
    serde_json::Value::Number(
        serde_json::Number::from_f64(value).unwrap_or_else(|| serde_json::Number::from(0)),
    )
}
