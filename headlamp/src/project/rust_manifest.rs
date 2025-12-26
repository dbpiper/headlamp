use std::path::{Path, PathBuf};

use crate::project::classify::FileKind;

#[derive(Debug, Clone)]
pub struct RustManifestPaths {
    test_files_abs: Vec<PathBuf>,
    production_files_abs: Vec<PathBuf>,
}

impl RustManifestPaths {
    pub fn read_from(cargo_toml_path: PathBuf) -> Result<Self, std::io::Error> {
        let Ok(raw) = std::fs::read_to_string(&cargo_toml_path) else {
            return Ok(Self {
                test_files_abs: vec![],
                production_files_abs: vec![],
            });
        };
        let Ok(value) = raw.parse::<toml::Value>() else {
            return Ok(Self {
                test_files_abs: vec![],
                production_files_abs: vec![],
            });
        };

        let crate_root = cargo_toml_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let lib_paths = parse_single_target_path(&value, "lib")
            .into_iter()
            .map(|p| crate_root.join(p))
            .collect::<Vec<_>>();
        let bin_paths = parse_array_target_paths(&value, "bin")
            .into_iter()
            .map(|p| crate_root.join(p))
            .collect::<Vec<_>>();
        let test_paths = parse_array_target_paths(&value, "test")
            .into_iter()
            .map(|p| crate_root.join(p))
            .collect::<Vec<_>>();

        Ok(Self {
            test_files_abs: canonicalize_all(test_paths),
            production_files_abs: canonicalize_all(
                lib_paths.into_iter().chain(bin_paths).collect(),
            ),
        })
    }

    pub fn classify(&self, abs_path: &Path) -> Option<FileKind> {
        let canonical = dunce::canonicalize(abs_path)
            .ok()
            .unwrap_or_else(|| abs_path.to_path_buf());
        if self.test_files_abs.iter().any(|p| p == &canonical) {
            return Some(FileKind::Test);
        }
        if self.production_files_abs.iter().any(|p| p == &canonical) {
            return Some(FileKind::Production);
        }
        None
    }
}

fn parse_single_target_path(value: &toml::Value, table_key: &str) -> Option<String> {
    value
        .get(table_key)
        .and_then(|v| v.as_table())
        .and_then(|t| t.get("path"))
        .and_then(|p| p.as_str())
        .map(|s| s.to_string())
}

fn parse_array_target_paths(value: &toml::Value, array_key: &str) -> Vec<String> {
    value
        .get(array_key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_table())
                .filter_map(|t| t.get("path").and_then(|p| p.as_str()))
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn canonicalize_all(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .filter_map(|p| dunce::canonicalize(&p).ok().or(Some(p)))
        .collect::<Vec<_>>()
}
