use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use oxc_ast::ast::{
    Argument, ArrayExpression, ArrayExpressionElement, Expression, ObjectExpression,
    ObjectPropertyKind, PropertyKey, UnaryOperator,
};

use crate::error::HeadlampError;

use super::evaluator::ModuleEvaluator;
use super::types::{ImportBinding, ModuleCacheEntry};

impl<'a> ModuleEvaluator<'a> {
    pub(super) fn eval_identifier_value(
        &mut self,
        name: &str,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        if let Some(it) = self.const_inits.get(name) {
            return self.eval_expression_to_value(it, cache, stack);
        }

        if let Some(binding) = self.imports.get(name).cloned() {
            let entry = super::load_module_exports(
                match &binding {
                    ImportBinding::Named { from, .. } => from,
                    ImportBinding::Default { from } => from,
                },
                cache,
                stack,
            )?;
            return match binding {
                ImportBinding::Named { export, .. } => entry
                    .exports
                    .get(export.as_str())
                    .cloned()
                    .ok_or_else(|| self.unsupported("imported named export not found")),
                ImportBinding::Default { .. } => entry
                    .default_export
                    .clone()
                    .ok_or_else(|| self.unsupported("imported default export not found")),
            };
        }

        Err(self.unsupported("unknown identifier"))
    }

    pub(super) fn eval_call_expression(
        &mut self,
        callee: &'a Expression<'a>,
        args: &'a oxc_allocator::Vec<'a, Argument<'a>>,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        let Expression::Identifier(id) = callee else {
            return Err(self.unsupported("unsupported call callee"));
        };
        let name = id.name.as_str();
        if name != "defineConfig" {
            return Err(self.unsupported("unsupported call expression"));
        }
        let first = args
            .first()
            .ok_or_else(|| self.unsupported("defineConfig requires arg"))?;
        self.eval_argument_to_value(first, cache, stack)
    }

    pub(super) fn eval_argument_to_value(
        &mut self,
        arg: &'a Argument<'a>,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        match arg {
            Argument::SpreadElement(_) => Err(self.unsupported("unsupported spread argument")),
            _ => arg
                .as_expression()
                .map(|expr| self.eval_expression_to_value(expr, cache, stack))
                .unwrap_or_else(|| Err(self.unsupported("unsupported argument"))),
        }
    }

    pub(super) fn eval_expression_to_value(
        &mut self,
        expr: &'a Expression<'a>,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        match expr {
            Expression::ObjectExpression(obj) => self.eval_object_expression(obj, cache, stack),
            Expression::ArrayExpression(arr) => self.eval_array_expression(arr, cache, stack),
            Expression::Identifier(ident) => {
                self.eval_identifier_value(ident.name.as_str(), cache, stack)
            }
            Expression::StringLiteral(lit) => Ok(serde_json::Value::String(lit.value.to_string())),
            Expression::NumericLiteral(lit) => Ok(super::utils::json_number_from_f64(lit.value)),
            Expression::BooleanLiteral(lit) => Ok(serde_json::Value::Bool(lit.value)),
            Expression::NullLiteral(_) => Ok(serde_json::Value::Null),
            Expression::UnaryExpression(unary) => {
                self.eval_unary_expression(unary.operator, &unary.argument, cache, stack)
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
            Expression::CallExpression(call) => {
                self.eval_call_expression(&call.callee, &call.arguments, cache, stack)
            }
            _ => Err(self.unsupported("unsupported expression")),
        }
    }

    pub(super) fn eval_unary_expression(
        &mut self,
        op: UnaryOperator,
        arg: &'a Expression<'a>,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        match op {
            UnaryOperator::UnaryNegation => self
                .eval_expression_to_value(arg, cache, stack)
                .and_then(|v| {
                    v.as_f64()
                        .map(|n| super::utils::json_number_from_f64(-n))
                        .ok_or_else(|| self.unsupported("unary minus expects number"))
                }),
            UnaryOperator::UnaryPlus => self.eval_expression_to_value(arg, cache, stack),
            _ => Err(self.unsupported("unsupported unary operator")),
        }
    }

    pub(super) fn eval_object_expression(
        &mut self,
        obj: &'a ObjectExpression<'a>,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        let mut out = serde_json::Map::new();
        for prop in &obj.properties {
            match prop {
                ObjectPropertyKind::ObjectProperty(it) => {
                    let key = match &it.key {
                        PropertyKey::StaticIdentifier(id) => id.name.as_str().to_string(),
                        PropertyKey::StringLiteral(lit) => lit.value.to_string(),
                        _ => return Err(self.unsupported("unsupported object key")),
                    };
                    let value = self.eval_expression_to_value(&it.value, cache, stack)?;
                    out.insert(key, value);
                }
                ObjectPropertyKind::SpreadProperty(it) => {
                    let value = self.eval_expression_to_value(&it.argument, cache, stack)?;
                    let Some(obj) = value.as_object() else {
                        return Err(self.unsupported("spread expects object"));
                    };
                    obj.iter().for_each(|(k, v)| {
                        out.insert(k.clone(), v.clone());
                    });
                }
            }
        }
        Ok(serde_json::Value::Object(out))
    }

    pub(super) fn eval_array_expression(
        &mut self,
        arr: &'a ArrayExpression<'a>,
        cache: &mut HashMap<PathBuf, Arc<ModuleCacheEntry>>,
        stack: &mut Vec<PathBuf>,
    ) -> Result<serde_json::Value, HeadlampError> {
        let mut out = Vec::new();
        for el in &arr.elements {
            match el {
                ArrayExpressionElement::SpreadElement(_) => {
                    return Err(self.unsupported("unsupported array spread"));
                }
                ArrayExpressionElement::Elision(_) => out.push(serde_json::Value::Null),
                _ => {
                    let Some(expr) = el.as_expression() else {
                        return Err(self.unsupported("unsupported array element"));
                    };
                    out.push(self.eval_expression_to_value(expr, cache, stack)?);
                }
            }
        }
        Ok(serde_json::Value::Array(out))
    }
}
