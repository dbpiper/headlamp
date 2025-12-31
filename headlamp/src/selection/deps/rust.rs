use std::path::{Path, PathBuf};

pub fn extract_import_specs(abs_path: &Path) -> Vec<String> {
    let Ok(body) = std::fs::read_to_string(abs_path) else {
        return vec![];
    };
    crate::rust_parse::extract_import_specs_from_source(&body)
        .into_iter()
        .collect::<Vec<_>>()
}

pub fn resolve_import_with_root(from_file: &Path, spec: &str, root_dir: &Path) -> Option<PathBuf> {
    let raw = spec.trim();
    if raw.is_empty() {
        return None;
    }

    if let Some(path) = raw.strip_prefix("path:") {
        let rel = path.trim_start_matches('/');
        let full = root_dir.join(rel);
        return is_file(&full).then(|| canonicalize_lossy(&full)).flatten();
    }

    let from_dir = from_file.parent().unwrap_or(root_dir);
    let crate_name = find_crate_name_for_file(from_file).unwrap_or_default();
    let from_module_dir = module_dir_for_file(from_file);
    let crate_src_root =
        crate_src_root_for_file(from_file, root_dir).unwrap_or_else(|| root_dir.to_path_buf());

    let segments = parse_rust_path_segments(raw);
    if segments.is_empty() {
        return None;
    }

    let (base_dir, tail_segments) = resolve_base_dir_for_segments(
        &crate_name,
        &crate_src_root,
        &from_module_dir,
        from_dir,
        &segments,
    )?;

    resolve_module_like_reference(&base_dir, &tail_segments)
}

pub fn looks_like_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext == "rs")
}

pub fn build_seed_terms(
    repo_root: &Path,
    production_selection_paths_abs: &[String],
) -> Vec<String> {
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    production_selection_paths_abs.iter().for_each(|abs| {
        let abs_path = PathBuf::from(abs);
        let Ok(rel) = abs_path.strip_prefix(repo_root) else {
            return;
        };
        let Some(rel_text) = rel.to_str().map(|s| s.replace('\\', "/")) else {
            return;
        };
        let without_ext = strip_rs_extension(&rel_text);
        let without_mod = strip_trailing_mod_segment(&without_ext);
        if without_mod.is_empty() {
            return;
        }
        let base = Path::new(&without_mod)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let last_two = last_two_segments(&without_mod);
        [without_mod, base, last_two]
            .into_iter()
            .filter(|s| !s.is_empty())
            .for_each(|s| {
                out.insert(s);
            });
    });
    out.into_iter().collect()
}

fn parse_rust_path_segments(raw: &str) -> Vec<String> {
    raw.trim()
        .trim_start_matches("::")
        .split("::")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches(';'))
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
}

fn resolve_base_dir_for_segments(
    crate_name: &str,
    crate_src_root: &Path,
    from_module_dir: &Path,
    from_dir: &Path,
    segments: &[String],
) -> Option<(PathBuf, Vec<String>)> {
    match segments.first().map(|s| s.as_str()) {
        Some("crate") => Some((
            crate_src_root.to_path_buf(),
            segments.iter().skip(1).cloned().collect(),
        )),
        Some("self") => Some((
            from_module_dir.to_path_buf(),
            segments.iter().skip(1).cloned().collect(),
        )),
        Some("super") => resolve_super_base(from_module_dir, segments),
        Some(first) if !crate_name.is_empty() && first == crate_name => Some((
            crate_src_root.to_path_buf(),
            segments.iter().skip(1).cloned().collect(),
        )),
        _ => Some((from_dir.to_path_buf(), segments.to_vec())),
    }
}

fn resolve_super_base(
    from_module_dir: &Path,
    segments: &[String],
) -> Option<(PathBuf, Vec<String>)> {
    let super_count = segments
        .iter()
        .take_while(|s| s.as_str() == "super")
        .count();
    let tail = segments
        .iter()
        .skip(super_count)
        .cloned()
        .collect::<Vec<_>>();
    let base = (0..super_count).try_fold(from_module_dir.to_path_buf(), |acc, _| {
        acc.parent().map(|p| p.to_path_buf())
    })?;
    Some((base, tail))
}

fn resolve_module_like_reference(base_dir: &Path, segments: &[String]) -> Option<PathBuf> {
    if segments.is_empty() {
        return None;
    }
    resolve_module_file(base_dir, segments).or_else(|| {
        (segments.len() > 1)
            .then(|| resolve_module_file(base_dir, &segments[..segments.len() - 1]))
            .flatten()
    })
}

fn resolve_module_file(base_dir: &Path, segments: &[String]) -> Option<PathBuf> {
    let module_path = segments.join("/");
    let direct = base_dir.join(format!("{module_path}.rs"));
    if is_file(&direct) {
        return canonicalize_lossy(&direct);
    }
    let mod_rs = base_dir.join(module_path).join("mod.rs");
    is_file(&mod_rs)
        .then(|| canonicalize_lossy(&mod_rs))
        .flatten()
}

fn crate_src_root_for_file(from_file: &Path, repo_root: &Path) -> Option<PathBuf> {
    let crate_root = find_nearest_cargo_toml(from_file)
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| repo_root.to_path_buf());
    let src_root = crate_root.join("src");
    is_dir(&src_root).then_some(src_root)
}

fn module_dir_for_file(from_file: &Path) -> PathBuf {
    let Some(parent) = from_file.parent() else {
        return PathBuf::from(from_file);
    };
    parent.to_path_buf()
}

fn find_crate_name_for_file(from_file: &Path) -> Option<String> {
    find_nearest_cargo_toml(from_file).and_then(read_package_name_from_cargo_toml)
}

fn find_nearest_cargo_toml(from_file: &Path) -> Option<PathBuf> {
    let mut cur = from_file.parent()?;
    loop {
        let cand = cur.join("Cargo.toml");
        if is_file(&cand) {
            return Some(cand);
        }
        cur = cur.parent()?;
    }
}

fn read_package_name_from_cargo_toml(path: PathBuf) -> Option<String> {
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return None;
    };
    let Ok(val) = raw.parse::<toml::Value>() else {
        return None;
    };
    val.get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.replace('-', "_"))
}

fn strip_rs_extension(input: &str) -> String {
    input.strip_suffix(".rs").unwrap_or(input).to_string()
}

fn strip_trailing_mod_segment(input: &str) -> String {
    input.strip_suffix("/mod").unwrap_or(input).to_string()
}

fn last_two_segments(path_text: &str) -> String {
    let segs = path_text
        .split('/')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if segs.len() < 2 {
        return String::new();
    }
    format!("{}/{}", segs[segs.len() - 2], segs[segs.len() - 1])
}

fn is_file(path: &Path) -> bool {
    std::fs::metadata(path).ok().is_some_and(|m| m.is_file())
}

fn is_dir(path: &Path) -> bool {
    std::fs::metadata(path).ok().is_some_and(|m| m.is_dir())
}

fn canonicalize_lossy(path: &Path) -> Option<PathBuf> {
    dunce::canonicalize(path)
        .ok()
        .or_else(|| Some(path.to_path_buf()))
}
