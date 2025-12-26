use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::project::classify::FileKind;

#[derive(Debug, Clone)]
pub struct TsJsManifestClassifier {
    test_match: Option<GlobSet>,
    test_regex: Vec<regex::Regex>,
    ignore_regex: Vec<regex::Regex>,
    project_root: PathBuf,
}

impl TsJsManifestClassifier {
    pub fn read_from(project_root: PathBuf) -> Option<Self> {
        let package_json = project_root.join("package.json");
        let raw = std::fs::read_to_string(&package_json).ok()?;
        let value = serde_json::from_str::<serde_json::Value>(&raw).ok()?;
        let jest = value.get("jest")?;

        let test_match = jest
            .get("testMatch")
            .and_then(as_string_vec)
            .and_then(build_globset);
        let test_regex = jest
            .get("testRegex")
            .and_then(as_string_or_string_vec)
            .into_iter()
            .flatten()
            .filter_map(|pat| regex::Regex::new(&pat).ok())
            .collect::<Vec<_>>();
        let ignore_regex = jest
            .get("testPathIgnorePatterns")
            .and_then(as_string_or_string_vec)
            .into_iter()
            .flatten()
            .filter_map(|pat| regex::Regex::new(&pat).ok())
            .collect::<Vec<_>>();

        let any_configured =
            test_match.is_some() || !test_regex.is_empty() || !ignore_regex.is_empty();
        any_configured.then_some(Self {
            test_match,
            test_regex,
            ignore_regex,
            project_root,
        })
    }

    pub fn classify(&self, abs_path: &Path) -> Option<FileKind> {
        let rel = abs_path
            .strip_prefix(&self.project_root)
            .ok()
            .and_then(|p| p.to_str())
            .map(|s| s.replace('\\', "/"))?;

        if self.ignore_regex.iter().any(|re| re.is_match(&rel)) {
            return Some(FileKind::Production);
        }
        if self
            .test_match
            .as_ref()
            .is_some_and(|gs| gs.is_match(Path::new(&rel)))
        {
            return Some(FileKind::Test);
        }
        self.test_regex
            .iter()
            .any(|re| re.is_match(&rel))
            .then_some(FileKind::Test)
    }
}

fn as_string_vec(value: &serde_json::Value) -> Option<Vec<String>> {
    value.as_array().map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>()
    })
}

fn as_string_or_string_vec(value: &serde_json::Value) -> Option<Vec<String>> {
    value
        .as_str()
        .map(|s| vec![s.to_string()])
        .or_else(|| as_string_vec(value))
}

fn build_globset(patterns: Vec<String>) -> Option<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    patterns
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .filter_map(|s| Glob::new(&s).ok())
        .for_each(|g| {
            builder.add(g);
        });
    builder.build().ok()
}
