use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use duct::cmd as duct_cmd;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use which::which;

use crate::config_ts::load_headlamp_config_ts_oxc;
use crate::error::HeadlampError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangedMode {
    All,
    Staged,
    Unstaged,
    Branch,
    LastCommit,
    LastRelease,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoverageUi {
    Jest,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoverageMode {
    Compact,
    Full,
    Auto,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CoverageSection {
    pub abort_on_failure: Option<bool>,
    pub mode: Option<CoverageMode>,
    pub page_fit: Option<bool>,
    pub thresholds: Option<CoverageThresholds>,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CoverageThresholds {
    pub lines: Option<f64>,
    pub functions: Option<f64>,
    pub branches: Option<f64>,
    pub statements: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChangedSection {
    pub depth: Option<u32>,

    #[serde(flatten)]
    pub per_mode: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CoverageConfig {
    Bool(bool),
    Obj(CoverageSection),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ChangedConfig {
    Mode(ChangedMode),
    Obj(ChangedSection),
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HeadlampConfig {
    pub bootstrap_command: Option<String>,
    pub jest_args: Option<Vec<String>>,
    pub vitest_args: Option<Vec<String>>,
    pub sequential: Option<bool>,

    pub watch: Option<bool>,
    pub ci: Option<bool>,
    pub verbose: Option<bool>,
    pub no_cache: Option<bool>,

    pub coverage: Option<CoverageConfig>,
    pub coverage_ui: Option<CoverageUi>,
    pub coverage_abort_on_failure: Option<bool>,
    pub only_failures: Option<bool>,
    pub show_logs: Option<bool>,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub editor_cmd: Option<String>,
    pub workspace_root: Option<String>,
    pub coverage_detail: Option<serde_json::Value>,
    pub coverage_show_code: Option<bool>,
    pub coverage_mode: Option<CoverageMode>,
    pub coverage_max_files: Option<u32>,
    pub coverage_max_hotspots: Option<u32>,
    pub coverage_page_fit: Option<bool>,

    pub changed: Option<ChangedConfig>,

    pub coverage_section: Option<CoverageSection>,
    pub changed_section: Option<ChangedSection>,
}

pub fn find_repo_root(start: &Path) -> PathBuf {
    git2::Repository::discover(start)
        .ok()
        .and_then(|repo| repo.workdir().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| start.to_path_buf())
}

pub fn discover_config_path(repo_root: &Path) -> Option<PathBuf> {
    let names = [
        "headlamp.toml",
        "headlamp.config.ts",
        "headlamp.config.js",
        "headlamp.config.mjs",
        "headlamp.config.cjs",
        "headlamp.config.json",
        "headlamp.config.json5",
        "headlamp.config.jsonc",
        "headlamp.config.yaml",
        "headlamp.config.yml",
        ".headlamprc",
        ".headlamprc.json",
        ".headlamprc.json5",
        ".headlamprc.jsonc",
        ".headlamprc.yaml",
        ".headlamprc.yml",
        ".headlamprc.js",
        ".headlamprc.cjs",
        ".headlamprc.mjs",
        ".headlamprc.ts",
    ];
    names
        .into_iter()
        .map(|name| repo_root.join(name))
        .find(|p| p.exists())
}

pub fn load_headlamp_config(repo_root: &Path) -> Result<HeadlampConfig, HeadlampError> {
    if let Some(path) = discover_config_path(repo_root) {
        return load_headlamp_config_from_path(&path);
    }

    load_embedded_toml_config(repo_root)
}

pub fn load_headlamp_config_from_path(path: &Path) -> Result<HeadlampConfig, HeadlampError> {
    let ext = path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "json" | "json5" | "jsonc" => load_json_config(path),
        "yaml" | "yml" => load_yaml_config(path),
        "toml" => load_toml_config(path, TomlConfigSource::FullFile).map(|v| v.unwrap_or_default()),
        "ts" => load_ts_config_oxc(path),
        "js" | "mjs" | "cjs" => load_js_config(path),
        _ => Ok(HeadlampConfig::default()),
    }
}

fn load_json_config(path: &Path) -> Result<HeadlampConfig, HeadlampError> {
    let raw = std::fs::read_to_string(path).map_err(|source| HeadlampError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    json5::from_str::<HeadlampConfig>(&raw)
        .or_else(|_| serde_json::from_str::<HeadlampConfig>(&raw))
        .map_err(|err| HeadlampError::ConfigParse {
            path: path.to_path_buf(),
            message: err.to_string(),
        })
}

fn load_yaml_config(path: &Path) -> Result<HeadlampConfig, HeadlampError> {
    let raw = std::fs::read_to_string(path).map_err(|source| HeadlampError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_yaml::from_str::<HeadlampConfig>(&raw).map_err(|err| HeadlampError::ConfigParse {
        path: path.to_path_buf(),
        message: err.to_string(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TomlConfigSource {
    FullFile,
    PyProjectToolHeadlamp,
    CargoPackageMetadataHeadlamp,
}

fn load_embedded_toml_config(repo_root: &Path) -> Result<HeadlampConfig, HeadlampError> {
    let pyproject_path = repo_root.join("pyproject.toml");
    if pyproject_path.exists() {
        if let Some(config) =
            load_toml_config(&pyproject_path, TomlConfigSource::PyProjectToolHeadlamp)?
        {
            return Ok(config);
        }
    }

    let cargo_toml_path = repo_root.join("Cargo.toml");
    if cargo_toml_path.exists() {
        if let Some(config) = load_toml_config(
            &cargo_toml_path,
            TomlConfigSource::CargoPackageMetadataHeadlamp,
        )? {
            return Ok(config);
        }
    }

    Ok(HeadlampConfig::default())
}

fn load_toml_config(
    path: &Path,
    source: TomlConfigSource,
) -> Result<Option<HeadlampConfig>, HeadlampError> {
    let raw = std::fs::read_to_string(path).map_err(|source| HeadlampError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let toml_value =
        toml::from_str::<toml::Value>(&raw).map_err(|err| HeadlampError::ConfigParse {
            path: path.to_path_buf(),
            message: err.to_string(),
        })?;

    let maybe_table_value = match source {
        TomlConfigSource::FullFile => Some(toml_value),
        TomlConfigSource::PyProjectToolHeadlamp => extract_pyproject_tool_headlamp(&toml_value),
        TomlConfigSource::CargoPackageMetadataHeadlamp => {
            extract_cargo_package_metadata_headlamp(&toml_value)
        }
    };

    let Some(table_value) = maybe_table_value else {
        return Ok(None);
    };

    let json_value =
        serde_json::to_value(table_value).map_err(|err| HeadlampError::ConfigParse {
            path: path.to_path_buf(),
            message: err.to_string(),
        })?;
    let normalized = normalize_toml_keys_to_camel_case(&json_value);

    serde_json::from_value::<HeadlampConfig>(normalized)
        .map(Some)
        .map_err(|err| HeadlampError::ConfigParse {
            path: path.to_path_buf(),
            message: err.to_string(),
        })
}

fn extract_pyproject_tool_headlamp(value: &toml::Value) -> Option<toml::Value> {
    value
        .as_table()
        .and_then(|root| root.get("tool"))
        .and_then(|tool| tool.as_table())
        .and_then(|tool_table| tool_table.get("headlamp"))
        .cloned()
}

fn extract_cargo_package_metadata_headlamp(value: &toml::Value) -> Option<toml::Value> {
    value
        .as_table()
        .and_then(|root| root.get("package"))
        .and_then(|package| package.as_table())
        .and_then(|package_table| package_table.get("metadata"))
        .and_then(|metadata| metadata.as_table())
        .and_then(|metadata_table| metadata_table.get("headlamp"))
        .cloned()
}

fn normalize_toml_keys_to_camel_case(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Object(object) => {
            let normalized_entries = object.iter().map(|(key, value)| {
                let normalized_key = normalize_toml_key(key);
                let normalized_value = normalize_toml_keys_to_camel_case(value);
                (normalized_key, normalized_value)
            });
            JsonValue::Object(normalized_entries.collect())
        }
        JsonValue::Array(array) => JsonValue::Array(
            array
                .iter()
                .map(normalize_toml_keys_to_camel_case)
                .collect(),
        ),
        other => other.clone(),
    }
}

fn normalize_toml_key(key: &str) -> String {
    if !key.contains('_') {
        return key.to_string();
    }

    let mut segments = key.split('_').filter(|segment| !segment.is_empty());
    let Some(first_segment) = segments.next() else {
        return key.to_string();
    };

    let mut normalized_key = String::from(first_segment);
    for segment in segments {
        let mut chars = segment.chars();
        let Some(first_char) = chars.next() else {
            continue;
        };
        normalized_key.push(first_char.to_ascii_uppercase());
        normalized_key.extend(chars);
    }
    normalized_key
}

fn load_js_config(path: &Path) -> Result<HeadlampConfig, HeadlampError> {
    let node = which_node().ok_or_else(|| HeadlampError::NodeMissing {
        path: path.to_path_buf(),
    })?;

    let script = r#"
import { pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';

const p = process.argv[1];
const url = pathToFileURL(p).href;

let mod;
try {
  mod = await import(url);
} catch (e) {
  const require = createRequire(import.meta.url);
  // Best-effort TS support (matches c12/jiti behavior when ts-node is present).
  if (String(p).endsWith('.ts')) {
    try { require('ts-node/register/transpile-only'); } catch {}
    try { require('ts-node/register'); } catch {}
    try { require('tsx/require'); } catch {}
  }
  mod = require(p);
}

const cfg = mod && (mod.default ?? mod);
process.stdout.write(JSON.stringify(cfg ?? {}));
"#;

    let out = duct_cmd(
        &node,
        ["--input-type=module", "-e", script, &path.to_string_lossy()],
    )
    .stderr_capture()
    .stdout_capture()
    .unchecked()
    .run()
    .map_err(|e| HeadlampError::ConfigParse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    if !out.status.success() {
        let mut stderr = String::from_utf8_lossy(&out.stderr).to_string();
        if stderr.trim().is_empty() {
            stderr = format!("exit_code={:?}", out.status.code());
        }
        return Err(HeadlampError::NodeLoadFailed {
            path: path.to_path_buf(),
            stderr,
        });
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str::<HeadlampConfig>(&stdout).map_err(|err| HeadlampError::ConfigParse {
        path: path.to_path_buf(),
        message: err.to_string(),
    })
}

fn load_ts_config_oxc(path: &Path) -> Result<HeadlampConfig, HeadlampError> {
    let value = load_headlamp_config_ts_oxc(path)?;
    serde_json::from_value::<HeadlampConfig>(value.clone()).map_err(|err| {
        HeadlampError::ConfigParse {
            path: path.to_path_buf(),
            message: format!(
                "{err} (ts_config_json={})",
                serde_json::to_string(&value).unwrap_or_default()
            ),
        }
    })
}

fn which_node() -> Option<PathBuf> {
    which("node").ok()
}
