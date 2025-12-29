use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::error::HeadlampError;

mod evaluator;
mod expr;
mod resolver;
mod types;
mod utils;

pub fn load_headlamp_config_ts_oxc(path: &Path) -> Result<serde_json::Value, HeadlampError> {
    let mut cache: HashMap<PathBuf, Arc<types::ModuleCacheEntry>> = HashMap::new();
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
    cache: &mut HashMap<PathBuf, Arc<types::ModuleCacheEntry>>,
    stack: &mut Vec<PathBuf>,
) -> Result<Arc<types::ModuleCacheEntry>, HeadlampError> {
    let canonical = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(hit) = cache.get(&canonical) {
        return Ok(hit.clone());
    }
    if stack.contains(&canonical) {
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

    let resolver = resolver::build_resolver();
    let mut module_eval = evaluator::ModuleEvaluator::new(&canonical, &parsed.program, resolver);
    let entry = module_eval.eval_module(cache, stack)?;
    cache.insert(canonical.clone(), entry.clone());

    let _ = stack.pop();
    Ok(entry)
}
