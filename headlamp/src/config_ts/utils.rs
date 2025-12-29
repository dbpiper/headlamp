use oxc_ast::ast::ModuleExportName;

pub(super) fn module_export_name_as_str<'a>(name: &ModuleExportName<'a>) -> Option<&'a str> {
    match name {
        ModuleExportName::IdentifierName(it) => Some(it.name.as_str()),
        ModuleExportName::StringLiteral(it) => Some(it.value.as_str()),
        _ => None,
    }
}

pub(super) fn json_number_from_f64(value: f64) -> serde_json::Value {
    if !value.is_finite() {
        return serde_json::Value::Null;
    }
    if value.trunc() == value {
        if value >= 0.0 && value <= (u64::MAX as f64) {
            return serde_json::Value::Number(serde_json::Number::from(value as u64));
        }
        if value >= (i64::MIN as f64) && value <= (i64::MAX as f64) {
            return serde_json::Value::Number(serde_json::Number::from(value as i64));
        }
    }
    serde_json::Number::from_f64(value)
        .map(serde_json::Value::Number)
        .unwrap_or(serde_json::Value::Null)
}
