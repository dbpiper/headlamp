use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::run::RunError;

#[derive(Debug)]
pub struct RunSession {
    root: PathBuf,
    _temp_dir: Option<TempDir>,
}

impl RunSession {
    pub fn new(keep_artifacts: bool) -> Result<Self, RunError> {
        if keep_artifacts {
            let root = std::env::temp_dir().join("headlamp");
            std::fs::create_dir_all(&root).map_err(RunError::Io)?;
            return Ok(Self {
                root,
                _temp_dir: None,
            });
        }
        let temp_dir = tempfile::Builder::new()
            .prefix("headlamp-run-")
            .tempdir()
            .map_err(RunError::Io)?;
        Ok(Self {
            root: temp_dir.path().to_path_buf(),
            _temp_dir: Some(temp_dir),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn subdir(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}
