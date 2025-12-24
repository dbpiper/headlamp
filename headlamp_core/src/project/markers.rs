use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMarker {
    CargoToml,
    PackageJson,
}

impl ProjectMarker {
    pub fn filename(self) -> &'static str {
        match self {
            Self::CargoToml => "Cargo.toml",
            Self::PackageJson => "package.json",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRoot {
    pub root_dir: PathBuf,
    pub marker: ProjectMarker,
}

pub fn find_project_root(start_path: &Path) -> Option<ProjectRoot> {
    let mut cursor = if start_path.is_dir() {
        start_path.to_path_buf()
    } else {
        start_path.parent()?.to_path_buf()
    };

    loop {
        if is_file(&cursor.join(ProjectMarker::CargoToml.filename())) {
            return Some(ProjectRoot {
                root_dir: cursor,
                marker: ProjectMarker::CargoToml,
            });
        }
        if is_file(&cursor.join(ProjectMarker::PackageJson.filename())) {
            return Some(ProjectRoot {
                root_dir: cursor,
                marker: ProjectMarker::PackageJson,
            });
        }
        cursor = cursor.parent()?.to_path_buf();
    }
}

fn is_file(path: &Path) -> bool {
    std::fs::metadata(path).ok().is_some_and(|m| m.is_file())
}
