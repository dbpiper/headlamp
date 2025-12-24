use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxc_resolver::{
    ResolveContext, ResolveOptions, Resolver, TsConfig, TsconfigDiscovery, TsconfigOptions,
    TsconfigReferences,
};

#[derive(Debug, Default)]
pub struct TsJsResolveCache {
    by_dir: HashMap<PathBuf, CachedResolver>,
}

#[derive(Debug)]
struct CachedResolver {
    tsconfig: Option<LoadedTsConfig>,
    resolver: Resolver,
}

#[derive(Debug, Clone)]
struct LoadedTsConfig {
    tsconfig: Arc<TsConfig>,
    paths_patterns: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpecifierKind {
    Relative,
    RepoRootAbsolute,
    TsPathAlias,
    BareExternal,
}

pub struct TsJsImportResolver<'a> {
    repo_root: &'a Path,
}

impl<'a> TsJsImportResolver<'a> {
    pub fn new(repo_root: &'a Path) -> Self {
        Self { repo_root }
    }

    pub fn resolve_import(
        &self,
        from_file: &Path,
        specifier: &str,
        cache: &mut TsJsResolveCache,
    ) -> Option<PathBuf> {
        let raw_specifier = specifier.trim();
        if raw_specifier.is_empty() {
            return None;
        }

        let from_dir = from_file.parent().unwrap_or(self.repo_root);
        let cached_resolver = cache.get_or_load(from_dir, self.repo_root)?;
        let kind = classify_specifier(raw_specifier, cached_resolver.tsconfig.as_ref());
        match kind {
            SpecifierKind::BareExternal => None,
            SpecifierKind::RepoRootAbsolute => {
                resolve_repo_root_absolute(raw_specifier, self.repo_root)
            }
            SpecifierKind::Relative => resolve_relative_with_root_dirs_fallback(
                raw_specifier,
                from_dir,
                self.repo_root,
                cached_resolver,
            ),
            SpecifierKind::TsPathAlias => resolve_with_oxc(
                raw_specifier,
                from_dir,
                self.repo_root,
                &cached_resolver.resolver,
                cached_resolver.tsconfig.as_ref(),
            ),
        }
    }
}

impl TsJsResolveCache {
    fn get_or_load(&mut self, from_dir: &Path, repo_root: &Path) -> Option<&CachedResolver> {
        if self.by_dir.contains_key(from_dir) {
            return self.by_dir.get(from_dir);
        }
        let created = build_cached_resolver(from_dir, repo_root)?;
        self.by_dir.insert(from_dir.to_path_buf(), created);
        self.by_dir.get(from_dir)
    }
}

fn build_cached_resolver(from_dir: &Path, repo_root: &Path) -> Option<CachedResolver> {
    let tsconfig_path = find_tsconfig_json(from_dir, repo_root);
    let tsconfig = tsconfig_path.as_deref().and_then(load_tsconfig);
    let resolver = build_oxc_resolver(tsconfig_path.as_deref());
    Some(CachedResolver { tsconfig, resolver })
}

fn build_oxc_resolver(tsconfig_path: Option<&Path>) -> Resolver {
    let extensions = [
        ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts", ".json",
    ]
    .into_iter()
    .map(|ext| ext.to_string())
    .collect::<Vec<_>>();
    let tsconfig = tsconfig_path.map(|path| {
        TsconfigDiscovery::Manual(TsconfigOptions {
            config_file: path.to_path_buf(),
            references: TsconfigReferences::Disabled,
        })
    });
    Resolver::new(ResolveOptions {
        extensions,
        tsconfig,
        ..Default::default()
    })
}

fn classify_specifier(specifier: &str, tsconfig: Option<&LoadedTsConfig>) -> SpecifierKind {
    if specifier.starts_with("./") || specifier.starts_with("../") {
        return SpecifierKind::Relative;
    }
    if specifier.starts_with('/') {
        return SpecifierKind::RepoRootAbsolute;
    }
    if matches_tsconfig_paths(specifier, tsconfig) {
        SpecifierKind::TsPathAlias
    } else {
        SpecifierKind::BareExternal
    }
}

fn matches_tsconfig_paths(specifier: &str, tsconfig: Option<&LoadedTsConfig>) -> bool {
    tsconfig.is_some_and(|cfg| {
        cfg.paths_patterns
            .iter()
            .any(|pattern| matches_ts_path_pattern(pattern, specifier))
    })
}

fn matches_ts_path_pattern(pattern: &str, specifier: &str) -> bool {
    let Some((prefix, suffix)) = pattern.split_once('*') else {
        return pattern == specifier;
    };
    specifier.starts_with(prefix) && specifier.ends_with(suffix)
}

fn resolve_repo_root_absolute(specifier: &str, repo_root: &Path) -> Option<PathBuf> {
    let rel = specifier.trim_start_matches('/');
    crate::selection::deps::ts_js::try_resolve_file(&repo_root.join(rel))
}

fn resolve_relative_with_root_dirs_fallback(
    specifier: &str,
    from_dir: &Path,
    repo_root: &Path,
    cached_resolver: &CachedResolver,
) -> Option<PathBuf> {
    resolve_with_oxc(
        specifier,
        from_dir,
        repo_root,
        &cached_resolver.resolver,
        cached_resolver.tsconfig.as_ref(),
    )
}

fn resolve_with_oxc(
    specifier: &str,
    from_dir: &Path,
    repo_root: &Path,
    resolver: &Resolver,
    tsconfig: Option<&LoadedTsConfig>,
) -> Option<PathBuf> {
    let mut resolve_context = ResolveContext::default();
    let resolution = resolver
        .resolve_with_context(
            from_dir,
            specifier,
            tsconfig.map(|cfg| cfg.tsconfig.as_ref()),
            &mut resolve_context,
        )
        .ok()?;
    let path = resolution.into_path_buf();
    repo_local_guard(repo_root, &path).then_some(path)
}

fn repo_local_guard(repo_root: &Path, resolved: &Path) -> bool {
    let normalized_repo_root =
        dunce::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let normalized_resolved =
        dunce::canonicalize(resolved).unwrap_or_else(|_| resolved.to_path_buf());

    let Ok(rel) = normalized_resolved.strip_prefix(&normalized_repo_root) else {
        return false;
    };
    !rel.components()
        .any(|component| component.as_os_str() == "node_modules")
}

fn find_tsconfig_json(from_dir: &Path, repo_root: &Path) -> Option<PathBuf> {
    std::iter::successors(Some(from_dir), |dir| dir.parent())
        .take_while(|dir| dir.starts_with(repo_root))
        .find_map(|dir| {
            let candidate = dir.join("tsconfig.json");
            candidate.exists().then_some(candidate)
        })
}

fn load_tsconfig(tsconfig_path: &Path) -> Option<LoadedTsConfig> {
    let raw = std::fs::read_to_string(tsconfig_path).ok()?;
    let value = json5::from_str::<serde_json::Value>(&raw).ok()?;
    let paths_patterns = value
        .get("compilerOptions")
        .and_then(|v| v.get("paths"))
        .and_then(|v| v.as_object())
        .map(|obj| obj.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    let normalized_json = serde_json::to_string(&value).ok()?;
    let mut tsconfig = TsConfig::parse(true, tsconfig_path, normalized_json).ok()?;
    normalize_tsconfig_paths_and_root_dirs(&mut tsconfig);
    Some(LoadedTsConfig {
        tsconfig: Arc::new(tsconfig),
        paths_patterns,
    })
}

fn normalize_tsconfig_paths_and_root_dirs(tsconfig: &mut TsConfig) {
    let tsconfig_dir = tsconfig.directory().to_path_buf();
    let base_dir = tsconfig
        .compiler_options
        .base_url
        .as_ref()
        .map(|base_url| {
            if base_url.is_absolute() {
                base_url.to_path_buf()
            } else {
                tsconfig_dir.join(base_url)
            }
        })
        .unwrap_or_else(|| tsconfig_dir.clone());

    if let Some(base_url) = tsconfig.compiler_options.base_url.as_mut()
        && !base_url.is_absolute()
    {
        *base_url = tsconfig_dir.join(&*base_url);
    };

    if let Some(root_dirs) = tsconfig.compiler_options.root_dirs.as_mut() {
        root_dirs
            .iter_mut()
            .filter(|p| !p.is_absolute())
            .for_each(|p| {
                *p = tsconfig_dir.join(&*p);
            });
    }

    if let Some(paths) = tsconfig.compiler_options.paths.as_mut() {
        paths.values_mut().for_each(|targets| {
            targets
                .iter_mut()
                .filter(|p| !p.is_absolute())
                .for_each(|p| {
                    *p = base_dir.join(&*p);
                });
        });
    }
}
