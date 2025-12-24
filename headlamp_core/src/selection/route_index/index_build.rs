use std::collections::{BTreeMap, BTreeSet};

use crate::selection::route_index::RouteIndex;
use crate::selection::route_index::normalize;
use crate::selection::routes::types::FileRouteFacts;

pub fn build_route_index(facts_by_file: &BTreeMap<String, FileRouteFacts>) -> RouteIndex {
    let router_files = facts_by_file
        .values()
        .filter(|facts| facts.exports_router)
        .map(|facts| facts.abs_path_posix.clone())
        .collect::<BTreeSet<_>>();

    let mut index = RouteIndex::default();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut queue: Vec<(String, String, bool)> = facts_by_file
        .values()
        .filter(|facts| facts.has_root_container)
        .map(|facts| (facts.abs_path_posix.clone(), "/".to_string(), true))
        .chain(
            facts_by_file
                .values()
                .filter(|facts| facts.exports_router)
                .map(|facts| (facts.abs_path_posix.clone(), "/".to_string(), false)),
        )
        .collect();

    while let Some((file_path, base_path, is_root)) = queue.pop() {
        let visit_key = format!("{file_path}::{base_path}::{is_root}");
        if !visited.insert(visit_key) {
            continue;
        }
        let Some(facts) = facts_by_file.get(&file_path) else {
            continue;
        };

        let routes = if is_root {
            &facts.root_routes
        } else {
            &facts.router_routes
        };
        routes.iter().for_each(|local_route| {
            let full = normalize::join_http_paths(&base_path, &local_route.path);
            index
                .sources_by_http_route
                .entry(full.clone())
                .or_default()
                .push(facts.abs_path_posix.clone());
            index
                .http_routes_by_source
                .entry(facts.abs_path_posix.clone())
                .or_default()
                .push(full);
        });

        let mounts = if is_root {
            &facts.root_mounts
        } else {
            &facts.router_mounts
        };
        mounts.iter().for_each(|edge| {
            if !router_files.contains(&edge.target_abs_posix) {
                return;
            }
            let next_prefix = normalize::join_http_paths(&base_path, &edge.base_path);
            queue.push((edge.target_abs_posix.clone(), next_prefix, false));
        });
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
