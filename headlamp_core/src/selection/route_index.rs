use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use duct::cmd as duct_cmd;
use fancy_regex::Regex as FancyRegex;
use path_slash::PathExt;
use regex::Regex;
use which::which;

use super::import_resolve::resolve_import_with_root;

const CANDIDATE_FILE_GLOBS: [&str; 1] = ["**/*.{ts,tsx,js,jsx,mjs,cjs}"];
const ROUTE_EXCLUDE_GLOBS: [&str; 4] = [
    "**/node_modules/**",
    "**/dist/**",
    "**/build/**",
    "**/.next/**",
];

#[derive(Debug, Clone)]
struct RouteUseEdge {
    container: String,
    base_path: String,
    target_abs_posix: String,
}

#[derive(Debug, Clone, Default)]
struct FileRouteInfo {
    abs_path_posix: String,
    app_containers: BTreeSet<String>,
    router_containers: BTreeSet<String>,
    app_http_routes: Vec<String>,
    router_http_routes: Vec<String>,
    app_uses: Vec<RouteUseEdge>,
    router_uses: Vec<RouteUseEdge>,
    exports_router: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RouteIndex {
    sources_by_http_route: BTreeMap<String, Vec<String>>,
    http_routes_by_source: BTreeMap<String, Vec<String>>,
}

impl RouteIndex {
    pub fn sources_for_http_route(&self, http_path: &str) -> Vec<String> {
        self.sources_by_http_route
            .get(&normalize_http_path(http_path))
            .cloned()
            .unwrap_or_default()
    }

    pub fn http_routes_for_source(&self, source_path: &str) -> Vec<String> {
        self.http_routes_by_source
            .get(&normalize_fs_path(source_path))
            .cloned()
            .unwrap_or_default()
    }
}

pub fn get_route_index(repo_root: &Path) -> RouteIndex {
    let Ok(rg) = which("rg") else {
        return RouteIndex::default();
    };
    let target = repo_root.to_string_lossy().to_string();

    let mut args: Vec<String> = vec![
        "--no-messages".to_string(),
        "--color".to_string(),
        "never".to_string(),
        "--files-with-matches".to_string(),
    ];
    for glob in CANDIDATE_FILE_GLOBS {
        args.push("-g".to_string());
        args.push(glob.to_string());
    }
    for exclude in ROUTE_EXCLUDE_GLOBS {
        args.push("-g".to_string());
        args.push(format!("!{exclude}"));
    }
    args.push("-e".to_string());
    args.push(
        "express\\.Router\\(|\\.use\\(|\\.get\\(|\\.post\\(|\\.put\\(|\\.delete\\(|\\.patch\\("
            .to_string(),
    );
    args.push(target.clone());

    let Ok(out) = duct_cmd(rg, args)
        .dir(repo_root)
        .env("CI", "1")
        .stderr_capture()
        .stdout_capture()
        .unchecked()
        .run()
    else {
        return RouteIndex::default();
    };

    let files = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|rel_or_abs| repo_root.join(rel_or_abs))
        .collect::<Vec<_>>();

    build_index_from_files(repo_root, &files)
}

pub fn discover_tests_for_http_paths(
    repo_root: &Path,
    http_paths: &[String],
    exclude_globs: &[String],
) -> Vec<String> {
    let Ok(rg) = which("rg") else {
        return vec![];
    };
    if http_paths.is_empty() {
        return vec![];
    }
    let tokens = http_paths
        .iter()
        .flat_map(|p| expand_http_search_tokens(p))
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return vec![];
    }

    let mut args: Vec<String> = vec![
        "--no-messages".to_string(),
        "--line-number".to_string(),
        "--color".to_string(),
        "never".to_string(),
        "--files-with-matches".to_string(),
        "-F".to_string(),
        "-S".to_string(),
    ];
    for g in [
        "**/*.{test,spec}.{ts,tsx,js,jsx}",
        "tests/**/*.{ts,tsx,js,jsx}",
    ] {
        args.push("-g".to_string());
        args.push(g.to_string());
    }
    for ex in exclude_globs {
        args.push("-g".to_string());
        args.push(format!("!{ex}"));
    }
    for token in tokens {
        args.push("-e".to_string());
        args.push(token);
    }
    args.push(repo_root.to_string_lossy().to_string());

    let Ok(out) = duct_cmd(rg, args)
        .dir(repo_root)
        .env("CI", "1")
        .stderr_capture()
        .stdout_capture()
        .unchecked()
        .run()
    else {
        return vec![];
    };

    let raw = String::from_utf8_lossy(&out.stdout);
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|rel_or_abs| repo_root.join(rel_or_abs).to_slash_lossy().to_string())
        .collect()
}

fn extract_http_routes_from_source_text(
    source_text: &str,
    allowed_containers: &BTreeSet<String>,
    app_containers: &BTreeSet<String>,
) -> (Vec<String>, Vec<String>) {
    let Ok(re) = FancyRegex::new(
        r#"\b([A-Za-z_$][A-Za-z0-9_$]*)\.(?:get|post|put|delete|patch|options|head|all)\(\s*(['"`])([^'"`]+)\2"#,
    ) else {
        return (vec![], vec![]);
    };
    let mut app_routes: Vec<String> = vec![];
    let mut router_routes: Vec<String> = vec![];

    for cap in re.captures_iter(source_text).filter_map(|cap| cap.ok()) {
        let container = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let path_text = cap
            .get(3)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if container.trim().is_empty() || path_text.trim().is_empty() {
            continue;
        }
        if !allowed_containers.contains(&container) {
            continue;
        }
        let normalized = normalize_http_path(&path_text);
        if normalized.is_empty() {
            continue;
        }
        if app_containers.contains(&container) {
            app_routes.push(normalized);
        } else {
            router_routes.push(normalized);
        }
    }

    (app_routes, router_routes)
}

fn build_index_from_files(repo_root: &Path, files: &[PathBuf]) -> RouteIndex {
    let mut infos_by_source: BTreeMap<String, FileRouteInfo> = BTreeMap::new();

    for path in files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let source_key = normalize_fs_path(&path.to_string_lossy());
        let info = parse_file_route_info(repo_root, path, &text, &source_key);
        if info.app_containers.is_empty()
            && info.router_containers.is_empty()
            && info.app_http_routes.is_empty()
            && info.router_http_routes.is_empty()
            && info.app_uses.is_empty()
            && info.router_uses.is_empty()
        {
            continue;
        }
        infos_by_source.insert(source_key.clone(), info);
    }

    let router_files = infos_by_source
        .values()
        .filter(|info| info.exports_router)
        .map(|info| info.abs_path_posix.clone())
        .collect::<BTreeSet<_>>();

    let mut index = RouteIndex::default();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut queue: Vec<(String, String, bool)> = infos_by_source
        .values()
        .filter(|info| !info.app_containers.is_empty())
        .map(|info| (info.abs_path_posix.clone(), "/".to_string(), true))
        .chain(
            infos_by_source
                .values()
                .filter(|info| info.exports_router)
                .map(|info| (info.abs_path_posix.clone(), "/".to_string(), false)),
        )
        .collect();

    while let Some((file_path, base_path, is_app)) = queue.pop() {
        let visit_key = format!("{file_path}::{base_path}::{is_app}");
        if !visited.insert(visit_key) {
            continue;
        }
        let Some(info) = infos_by_source.get(&file_path) else {
            continue;
        };

        let routes = if is_app {
            &info.app_http_routes
        } else {
            &info.router_http_routes
        };
        for local_route in routes {
            let full = join_http_paths(&base_path, local_route);
            index
                .sources_by_http_route
                .entry(full.clone())
                .or_default()
                .push(info.abs_path_posix.clone());
            index
                .http_routes_by_source
                .entry(info.abs_path_posix.clone())
                .or_default()
                .push(full);
        }

        let uses = if is_app {
            &info.app_uses
        } else {
            &info.router_uses
        };
        for edge in uses {
            if !router_files.contains(&edge.target_abs_posix) {
                continue;
            }
            let next_prefix = join_http_paths(&base_path, &edge.base_path);
            queue.push((edge.target_abs_posix.clone(), next_prefix, false));
        }
    }

    index
        .sources_by_http_route
        .iter_mut()
        .for_each(|(_, sources)| {
            sources.sort();
            sources.dedup();
        });
    index
        .http_routes_by_source
        .iter_mut()
        .for_each(|(_, routes)| {
            routes.sort();
            routes.dedup();
        });
    index
}

fn parse_file_route_info(
    repo_root: &Path,
    file_path: &Path,
    source_text: &str,
    abs_path_posix: &str,
) -> FileRouteInfo {
    let (app_containers, router_containers) = extract_container_sets(source_text);
    let allowed_containers = app_containers
        .iter()
        .chain(router_containers.iter())
        .cloned()
        .collect::<BTreeSet<_>>();

    let imports_by_local = extract_import_map(repo_root, file_path, source_text);
    let exports_router = detect_exports_router(source_text, &router_containers);

    let (app_http_routes, router_http_routes) =
        extract_http_routes_from_source_text(source_text, &allowed_containers, &app_containers);

    let (app_uses, router_uses) = extract_use_edges(
        repo_root,
        file_path,
        source_text,
        &allowed_containers,
        &app_containers,
        &imports_by_local,
    );

    FileRouteInfo {
        abs_path_posix: abs_path_posix.to_string(),
        app_containers,
        router_containers,
        app_http_routes,
        router_http_routes,
        app_uses,
        router_uses,
        exports_router,
    }
}

fn extract_container_sets(source_text: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut app: BTreeSet<String> = BTreeSet::new();
    let mut router: BTreeSet<String> = BTreeSet::new();

    let Ok(re_app) =
        Regex::new(r#"\b(?:const|let|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*=\s*express\(\s*\)"#)
    else {
        return (app, router);
    };
    let Ok(re_router_express) = Regex::new(
        r#"\b(?:const|let|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*=\s*express\.Router\(\s*\)"#,
    ) else {
        return (app, router);
    };
    let Ok(re_router_ident) =
        Regex::new(r#"\b(?:const|let|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*=\s*Router\(\s*\)"#)
    else {
        return (app, router);
    };

    for cap in re_app.captures_iter(source_text) {
        if let Some(m) = cap.get(1) {
            app.insert(m.as_str().to_string());
        }
    }
    for cap in re_router_express.captures_iter(source_text) {
        if let Some(m) = cap.get(1) {
            router.insert(m.as_str().to_string());
        }
    }
    for cap in re_router_ident.captures_iter(source_text) {
        if let Some(m) = cap.get(1) {
            router.insert(m.as_str().to_string());
        }
    }

    (app, router)
}

fn detect_exports_router(source_text: &str, router_containers: &BTreeSet<String>) -> bool {
    if router_containers.is_empty() {
        return false;
    }
    let Ok(re_cjs) = Regex::new(r#"\bmodule\.exports\s*=\s*([A-Za-z_$][A-Za-z0-9_$]*)\b"#) else {
        return false;
    };
    let Ok(re_cjs_prop) = Regex::new(
        r#"\bmodule\.exports\.[A-Za-z_$][A-Za-z0-9_$]*\s*=\s*([A-Za-z_$][A-Za-z0-9_$]*)\b"#,
    ) else {
        return false;
    };
    let Ok(re_exports_prop) =
        Regex::new(r#"\bexports\.[A-Za-z_$][A-Za-z0-9_$]*\s*=\s*([A-Za-z_$][A-Za-z0-9_$]*)\b"#)
    else {
        return false;
    };
    let Ok(re_cjs_arrow) =
        Regex::new(r#"\bmodule\.exports\s*=\s*\(\s*\)\s*=>\s*([A-Za-z_$][A-Za-z0-9_$]*)\b"#)
    else {
        return false;
    };
    let Ok(re_cjs_fn) = Regex::new(
        r#"\bmodule\.exports\s*=\s*function\b[\s\S]*?\breturn\s+([A-Za-z_$][A-Za-z0-9_$]*)\b"#,
    ) else {
        return false;
    };
    let Ok(re_esm) = Regex::new(r#"\bexport\s+default\s+([A-Za-z_$][A-Za-z0-9_$]*)\b"#) else {
        return false;
    };
    let mut candidates = re_cjs
        .captures_iter(source_text)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .chain(
            re_cjs_prop
                .captures_iter(source_text)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string())),
        )
        .chain(
            re_exports_prop
                .captures_iter(source_text)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string())),
        )
        .chain(
            re_cjs_arrow
                .captures_iter(source_text)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string())),
        )
        .chain(
            re_cjs_fn
                .captures_iter(source_text)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string())),
        )
        .chain(
            re_esm
                .captures_iter(source_text)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string())),
        );

    candidates.any(|name| router_containers.contains(&name))
}

fn extract_import_map(
    repo_root: &Path,
    file_path: &Path,
    source_text: &str,
) -> BTreeMap<String, String> {
    let requires = extract_require_descriptors(source_text);
    let imports = extract_import_descriptors(source_text);
    requires
        .into_iter()
        .chain(imports.into_iter())
        .filter_map(|(local, spec)| {
            let resolved = resolve_import_with_root(file_path, &spec, repo_root)?;
            Some((local, normalize_fs_path(&resolved.to_slash_lossy())))
        })
        .collect::<BTreeMap<_, _>>()
}

fn extract_use_edges(
    repo_root: &Path,
    file_path: &Path,
    source_text: &str,
    allowed_containers: &BTreeSet<String>,
    app_containers: &BTreeSet<String>,
    imports_by_local: &BTreeMap<String, String>,
) -> (Vec<RouteUseEdge>, Vec<RouteUseEdge>) {
    let mut app_edges: Vec<RouteUseEdge> = vec![];
    let mut router_edges: Vec<RouteUseEdge> = vec![];
    let mut seen: BTreeSet<(String, String, String)> = BTreeSet::new();

    let Ok(re_use_ident) = FancyRegex::new(
        r#"\b([A-Za-z_$][A-Za-z0-9_$]*)\.use\(\s*(['"`])([^'"`]+)\2\s*,\s*([A-Za-z_$][A-Za-z0-9_$]*)"#,
    ) else {
        return (app_edges, router_edges);
    };
    let Ok(re_use_ident_call) = FancyRegex::new(
        r#"\b([A-Za-z_$][A-Za-z0-9_$]*)\.use\(\s*(['"`])([^'"`]+)\2\s*,\s*([A-Za-z_$][A-Za-z0-9_$]*)\s*\("#,
    ) else {
        return (app_edges, router_edges);
    };
    let Ok(re_use_require) = FancyRegex::new(
        r#"\b([A-Za-z_$][A-Za-z0-9_$]*)\.use\(\s*(['"`])([^'"`]+)\2\s*,\s*require\(\s*['"]([^'"]+)['"]\s*\)\s*\)"#,
    ) else {
        return (app_edges, router_edges);
    };
    let Ok(re_use_require_prefix) = FancyRegex::new(
        r#"\b([A-Za-z_$][A-Za-z0-9_$]*)\.use\(\s*(['"`])([^'"`]+)\2\s*,\s*require\(\s*['"]([^'"]+)['"]\s*\)"#,
    ) else {
        return (app_edges, router_edges);
    };
    let Ok(re_use_ident_no_path) = FancyRegex::new(
        r#"\b([A-Za-z_$][A-Za-z0-9_$]*)\.use\(\s*([A-Za-z_$][A-Za-z0-9_$]*)\s*[,\)]"#,
    ) else {
        return (app_edges, router_edges);
    };
    let Ok(re_use_ident_no_path_call) =
        FancyRegex::new(r#"\b([A-Za-z_$][A-Za-z0-9_$]*)\.use\(\s*([A-Za-z_$][A-Za-z0-9_$]*)\s*\("#)
    else {
        return (app_edges, router_edges);
    };
    let Ok(re_use_require_no_path) = FancyRegex::new(
        r#"\b([A-Za-z_$][A-Za-z0-9_$]*)\.use\(\s*require\(\s*['"]([^'"]+)['"]\s*\)\s*\)"#,
    ) else {
        return (app_edges, router_edges);
    };
    let Ok(re_use_require_no_path_prefix) = FancyRegex::new(
        r#"\b([A-Za-z_$][A-Za-z0-9_$]*)\.use\(\s*require\(\s*['"]([^'"]+)['"]\s*\)"#,
    ) else {
        return (app_edges, router_edges);
    };

    let mut push_edge = |edge: RouteUseEdge| {
        let key = (
            edge.container.clone(),
            edge.base_path.clone(),
            edge.target_abs_posix.clone(),
        );
        if !seen.insert(key) {
            return;
        }
        if app_containers.contains(&edge.container) {
            app_edges.push(edge);
        } else {
            router_edges.push(edge);
        }
    };

    for cap in re_use_ident
        .captures_iter(source_text)
        .filter_map(|c| c.ok())
    {
        let container = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let base_path = cap
            .get(3)
            .map(|m| normalize_http_path(m.as_str()))
            .unwrap_or_else(|| "/".to_string());
        let local = cap
            .get(4)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if container.trim().is_empty()
            || base_path.trim().is_empty()
            || local.trim().is_empty()
            || !allowed_containers.contains(&container)
        {
            continue;
        }
        let Some(target_abs_posix) = imports_by_local.get(&local).cloned() else {
            continue;
        };
        push_edge(RouteUseEdge {
            container,
            base_path,
            target_abs_posix,
        });
    }

    for cap in re_use_ident_call
        .captures_iter(source_text)
        .filter_map(|c| c.ok())
    {
        let container = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let base_path = cap
            .get(3)
            .map(|m| normalize_http_path(m.as_str()))
            .unwrap_or_else(|| "/".to_string());
        let local = cap
            .get(4)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if container.trim().is_empty()
            || base_path.trim().is_empty()
            || local.trim().is_empty()
            || !allowed_containers.contains(&container)
        {
            continue;
        }
        let Some(target_abs_posix) = imports_by_local.get(&local).cloned() else {
            continue;
        };
        push_edge(RouteUseEdge {
            container,
            base_path,
            target_abs_posix,
        });
    }

    for cap in re_use_require
        .captures_iter(source_text)
        .filter_map(|c| c.ok())
    {
        let container = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let base_path = cap
            .get(3)
            .map(|m| normalize_http_path(m.as_str()))
            .unwrap_or_else(|| "/".to_string());
        let spec = cap
            .get(4)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if container.trim().is_empty()
            || base_path.trim().is_empty()
            || spec.trim().is_empty()
            || !allowed_containers.contains(&container)
        {
            continue;
        }
        let Some(resolved) = resolve_import_with_root(file_path, &spec, repo_root) else {
            continue;
        };
        push_edge(RouteUseEdge {
            container,
            base_path,
            target_abs_posix: normalize_fs_path(&resolved.to_slash_lossy()),
        });
    }

    for cap in re_use_require_prefix
        .captures_iter(source_text)
        .filter_map(|c| c.ok())
    {
        let container = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let base_path = cap
            .get(3)
            .map(|m| normalize_http_path(m.as_str()))
            .unwrap_or_else(|| "/".to_string());
        let spec = cap
            .get(4)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if container.trim().is_empty()
            || base_path.trim().is_empty()
            || spec.trim().is_empty()
            || !allowed_containers.contains(&container)
        {
            continue;
        }
        let Some(resolved) = resolve_import_with_root(file_path, &spec, repo_root) else {
            continue;
        };
        push_edge(RouteUseEdge {
            container,
            base_path,
            target_abs_posix: normalize_fs_path(&resolved.to_slash_lossy()),
        });
    }

    for cap in re_use_ident_no_path
        .captures_iter(source_text)
        .filter_map(|c| c.ok())
    {
        let container = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let local = cap
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if container.trim().is_empty()
            || local.trim().is_empty()
            || !allowed_containers.contains(&container)
        {
            continue;
        }
        let Some(target_abs_posix) = imports_by_local.get(&local).cloned() else {
            continue;
        };
        push_edge(RouteUseEdge {
            container,
            base_path: "/".to_string(),
            target_abs_posix,
        });
    }

    for cap in re_use_ident_no_path_call
        .captures_iter(source_text)
        .filter_map(|c| c.ok())
    {
        let container = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let local = cap
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if container.trim().is_empty()
            || local.trim().is_empty()
            || !allowed_containers.contains(&container)
        {
            continue;
        }
        let Some(target_abs_posix) = imports_by_local.get(&local).cloned() else {
            continue;
        };
        push_edge(RouteUseEdge {
            container,
            base_path: "/".to_string(),
            target_abs_posix,
        });
    }

    for cap in re_use_require_no_path
        .captures_iter(source_text)
        .filter_map(|c| c.ok())
    {
        let container = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let spec = cap
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if container.trim().is_empty()
            || spec.trim().is_empty()
            || !allowed_containers.contains(&container)
        {
            continue;
        }
        let Some(resolved) = resolve_import_with_root(file_path, &spec, repo_root) else {
            continue;
        };
        push_edge(RouteUseEdge {
            container,
            base_path: "/".to_string(),
            target_abs_posix: normalize_fs_path(&resolved.to_slash_lossy()),
        });
    }

    for cap in re_use_require_no_path_prefix
        .captures_iter(source_text)
        .filter_map(|c| c.ok())
    {
        let container = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let spec = cap
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if container.trim().is_empty()
            || spec.trim().is_empty()
            || !allowed_containers.contains(&container)
        {
            continue;
        }
        let Some(resolved) = resolve_import_with_root(file_path, &spec, repo_root) else {
            continue;
        };
        push_edge(RouteUseEdge {
            container,
            base_path: "/".to_string(),
            target_abs_posix: normalize_fs_path(&resolved.to_slash_lossy()),
        });
    }

    (app_edges, router_edges)
}

fn extract_require_descriptors(source_text: &str) -> Vec<(String, String)> {
    let Ok(re_ident) = Regex::new(
        r#"(?m)^\s*(?:const|let|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*=\s*require\(\s*['"]([^'"]+)['"]\s*\)"#,
    ) else {
        return vec![];
    };
    let Ok(re_destructure) = Regex::new(
        r#"(?m)^\s*(?:const|let|var)\s*\{([^}]+)\}\s*=\s*require\(\s*['"]([^'"]+)['"]\s*\)"#,
    ) else {
        return vec![];
    };
    let Ok(re_ident_only) = Regex::new(r#"^[A-Za-z_$][A-Za-z0-9_$]*$"#) else {
        return vec![];
    };

    let ident_assigns = re_ident
        .captures_iter(source_text)
        .filter_map(|cap| {
            Some((
                cap.get(1)?.as_str().to_string(),
                cap.get(2)?.as_str().to_string(),
            ))
        })
        .collect::<Vec<_>>();

    let destructured = re_destructure
        .captures_iter(source_text)
        .filter_map(|cap| {
            let bindings = cap.get(1)?.as_str().to_string();
            let spec = cap.get(2)?.as_str().to_string();
            Some((bindings, spec))
        })
        .flat_map(|(bindings, spec)| {
            bindings
                .split(',')
                .map(str::trim)
                .filter(|raw| !raw.is_empty())
                .filter(|raw| !raw.starts_with("..."))
                .filter_map(|raw| {
                    let before_default = raw.split('=').next().unwrap_or(raw).trim();
                    let local = before_default
                        .split(':')
                        .nth(1)
                        .unwrap_or(before_default)
                        .trim();
                    if local.is_empty() || !re_ident_only.is_match(local) {
                        return None;
                    }
                    Some((local.to_string(), spec.clone()))
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    ident_assigns
        .into_iter()
        .chain(destructured.into_iter())
        .collect()
}

fn extract_import_descriptors(source_text: &str) -> Vec<(String, String)> {
    let Ok(re) = Regex::new(
        r#"(?m)^\s*import\s+(?:([A-Za-z_$][A-Za-z0-9_$]*)|\*\s+as\s+([A-Za-z_$][A-Za-z0-9_$]*))\s+from\s+['"]([^'"]+)['"]"#,
    ) else {
        return vec![];
    };
    re.captures_iter(source_text)
        .filter_map(|cap| {
            let local = cap
                .get(1)
                .or_else(|| cap.get(2))
                .map(|m| m.as_str().to_string())?;
            let spec = cap.get(3)?.as_str().to_string();
            Some((local, spec))
        })
        .collect()
}

fn join_http_paths(left: &str, right: &str) -> String {
    let l = normalize_http_path(left);
    let r = normalize_http_path(right);
    if l == "/" {
        return r;
    }
    let joined = format!("{}/{}", l.trim_end_matches('/'), r.trim_start_matches('/'));
    normalize_http_path(&joined)
}

fn normalize_fs_path(value: &str) -> String {
    dunce::canonicalize(Path::new(value))
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| value.to_string())
        .replace('\\', "/")
}

fn collapse_slashes(input: &str) -> String {
    let Ok(re) = Regex::new(r"/+?") else {
        return input.to_string();
    };
    re.replace_all(input, "/").to_string()
}

fn normalize_http_path(value: &str) -> String {
    let no_query = value.split('?').next().unwrap_or(value);
    let no_hash = no_query.split('#').next().unwrap_or(no_query);
    let without_origin = Regex::new(r"^https?://[^/]+")
        .ok()
        .map(|re| re.replace(no_hash, "").to_string())
        .unwrap_or_else(|| no_hash.to_string());
    let ensure_leading = if without_origin.starts_with('/') {
        without_origin
    } else {
        format!("/{without_origin}")
    };
    let collapsed = collapse_slashes(&ensure_leading);
    if collapsed.is_empty() {
        "/".to_string()
    } else {
        collapsed
    }
}

fn expand_http_search_tokens(http_path: &str) -> Vec<String> {
    let normalized = normalize_http_path(http_path);
    let mut tokens = vec![normalized.clone()];
    let without_params = Regex::new(r":[^/]+")
        .ok()
        .map(|re| re.replace_all(&normalized, "/").to_string())
        .unwrap_or_else(|| normalized.clone());
    tokens.push(without_params.clone());
    tokens.push(without_params.trim_end_matches('/').to_string());

    if let Some(last_slash) = normalized.rfind('/') {
        if last_slash > 0 {
            let base = normalized[..last_slash].to_string();
            tokens.push(base.clone());
            tokens.push(format!("{base}/"));
        }
    }

    tokens.into_iter().filter(|t| !t.is_empty()).collect()
}
