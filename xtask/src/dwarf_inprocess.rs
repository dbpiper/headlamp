use std::path::{Path, PathBuf};

use crate::dwarf_addr2line::ResolvedLocation;

pub fn resolve_locations_inprocess(
    binary_path: &Path,
    addresses: &[u64],
    raw_symbol_names: &[String],
) -> anyhow::Result<Vec<ResolvedLocation>> {
    if addresses.is_empty() {
        return Ok(Vec::new());
    }
    if addresses.len() != raw_symbol_names.len() {
        anyhow::bail!(
            "addresses ({}) and raw_symbol_names ({}) length mismatch",
            addresses.len(),
            raw_symbol_names.len()
        );
    }

    let addr2line_path =
        dwarf_path_for_binary(binary_path).unwrap_or_else(|| binary_path.to_path_buf());
    resolve_locations_parallel_with_loaders(&addr2line_path, addresses, raw_symbol_names)
}

pub fn default_headlamp_binary_path(repo_root: &Path) -> PathBuf {
    repo_root.join("target").join("release").join("headlamp")
}

fn dwarf_path_for_binary(binary_path: &Path) -> Option<PathBuf> {
    let dsym_dir = binary_path.with_extension("dSYM");
    let dwarf_dir = dsym_dir.join("Contents").join("Resources").join("DWARF");
    if !dwarf_dir.exists() {
        return None;
    }
    let mut candidates = std::fs::read_dir(&dwarf_dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    candidates.sort();
    if candidates.len() == 1 {
        return Some(candidates.remove(0));
    }
    let filename = binary_path.file_name()?;
    let direct = dwarf_dir.join(filename);
    direct.exists().then_some(direct)
}

fn demangle_symbol(raw_symbol_name: &str) -> String {
    let trimmed = raw_symbol_name.trim_start_matches('_');
    if let Ok(demangled) = rustc_demangle::try_demangle(trimmed) {
        return format!("{demangled:#}");
    }
    raw_symbol_name.to_string()
}

fn resolve_locations_parallel_with_loaders(
    binary_path: &Path,
    addresses: &[u64],
    raw_symbol_names: &[String],
) -> anyhow::Result<Vec<ResolvedLocation>> {
    let worker_count = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .max(1);
    let chunk_size = addresses.len().div_ceil(worker_count).max(1);

    let mut chunk_starts = (0..addresses.len()).step_by(chunk_size).collect::<Vec<_>>();
    if chunk_starts.last().copied() != Some(addresses.len()) {
        chunk_starts.push(addresses.len());
    }

    let binary_path = binary_path.to_path_buf();
    let mut results_by_chunk: Vec<(usize, Vec<ResolvedLocation>)> = Vec::new();

    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for window in chunk_starts.windows(2) {
            let start_index = window[0];
            let end_index = window[1];
            let binary_path = binary_path.clone();

            handles.push(scope.spawn(move || {
                let loader = addr2line::Loader::new(&binary_path).map_err(|error| {
                    anyhow::anyhow!("failed to create addr2line loader: {error}")
                })?;

                let mut chunk_results = Vec::with_capacity(end_index - start_index);
                for index in start_index..end_index {
                    let address = addresses[index];
                    let location = loader.find_location(address).map_err(|error| {
                        anyhow::anyhow!("addr2line find_location failed: {error}")
                    })?;

                    let (file_path, line_number) = match location {
                        None => (None, None),
                        Some(location) => {
                            (location.file.map(|path| path.to_string()), location.line)
                        }
                    };

                    chunk_results.push(ResolvedLocation {
                        function_name: demangle_symbol(&raw_symbol_names[index]),
                        file_path,
                        line_number,
                    });
                }

                Ok::<_, anyhow::Error>((start_index, chunk_results))
            }));
        }

        for handle in handles {
            results_by_chunk.push(handle.join().expect("worker thread panicked")?);
        }

        Ok::<_, anyhow::Error>(())
    })?;

    results_by_chunk.sort_by_key(|(start_index, _)| *start_index);
    let mut resolved = Vec::with_capacity(addresses.len());
    for (_, chunk) in results_by_chunk {
        resolved.extend(chunk);
    }
    Ok(resolved)
}
