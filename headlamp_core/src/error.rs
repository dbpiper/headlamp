use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HeadlampError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse config at {path}: {message}")]
    ConfigParse { path: PathBuf, message: String },

    #[error("node is required to load {path}")]
    NodeMissing { path: PathBuf },

    #[error("node failed to load {path}: {stderr}")]
    NodeLoadFailed { path: PathBuf, stderr: String },
}
