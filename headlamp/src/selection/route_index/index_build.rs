use std::collections::{BTreeMap, BTreeSet};

use crate::selection::route_index::RouteIndex;
use crate::selection::route_index::normalize;
use crate::selection::routes::types::FileRouteFacts;

pub fn build_route_index(facts_by_file: &BTreeMap<String, FileRouteFacts>) -> RouteIndex {
    let router_files = router_files(facts_by_file);
    let mut index = RouteIndex::default();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut queue = initial_queue(facts_by_file);

    while let Some(task) = queue.pop() {
        process_queue_task(ProcessQueueTaskArgs {
            facts_by_file,
            router_files: &router_files,
            visited: &mut visited,
            queue: &mut queue,
            index: &mut index,
            task,
        });
    }

    sort_and_dedupe_index(&mut index);
    index
}

#[derive(Debug)]
struct QueueTask {
    file_path: String,
    base_path: String,
    is_root: bool,
}

#[derive(Debug)]
struct ProcessQueueTaskArgs<'a> {
    facts_by_file: &'a BTreeMap<String, FileRouteFacts>,
    router_files: &'a BTreeSet<String>,
    visited: &'a mut BTreeSet<String>,
    queue: &'a mut Vec<QueueTask>,
    index: &'a mut RouteIndex,
    task: QueueTask,
}

fn router_files(facts_by_file: &BTreeMap<String, FileRouteFacts>) -> BTreeSet<String> {
    facts_by_file
        .values()
        .filter(|facts| facts.exports_router)
        .map(|facts| facts.abs_path_posix.clone())
        .collect::<BTreeSet<_>>()
}

fn initial_queue(facts_by_file: &BTreeMap<String, FileRouteFacts>) -> Vec<QueueTask> {
    let root_tasks = facts_by_file.values().filter_map(|facts| {
        facts.has_root_container.then_some(QueueTask {
            file_path: facts.abs_path_posix.clone(),
            base_path: "/".to_string(),
            is_root: true,
        })
    });
    let router_tasks = facts_by_file.values().filter_map(|facts| {
        facts.exports_router.then_some(QueueTask {
            file_path: facts.abs_path_posix.clone(),
            base_path: "/".to_string(),
            is_root: false,
        })
    });
    root_tasks.chain(router_tasks).collect()
}

fn process_queue_task(args: ProcessQueueTaskArgs<'_>) {
    let ProcessQueueTaskArgs {
        facts_by_file,
        router_files,
        visited,
        queue,
        index,
        task,
    } = args;

    let QueueTask {
        file_path,
        base_path,
        is_root,
    } = task;

    if !should_visit(visited, &file_path, &base_path, is_root) {
        return;
    }
    let Some(facts) = facts_by_file.get(&file_path) else {
        return;
    };

    add_routes_to_index(index, facts, &base_path, is_root);
    enqueue_mounts(queue, router_files, facts, &base_path, is_root);
}

fn should_visit(
    visited: &mut BTreeSet<String>,
    file_path: &str,
    base_path: &str,
    is_root: bool,
) -> bool {
    visited.insert(format!("{file_path}::{base_path}::{is_root}"))
}

fn add_routes_to_index(
    index: &mut RouteIndex,
    facts: &FileRouteFacts,
    base_path: &str,
    is_root: bool,
) {
    let routes = if is_root {
        &facts.root_routes
    } else {
        &facts.router_routes
    };
    routes.iter().for_each(|local_route| {
        let full = normalize::join_http_paths(base_path, &local_route.path);
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
}

fn enqueue_mounts(
    queue: &mut Vec<QueueTask>,
    router_files: &BTreeSet<String>,
    facts: &FileRouteFacts,
    base_path: &str,
    is_root: bool,
) {
    let mounts = if is_root {
        &facts.root_mounts
    } else {
        &facts.router_mounts
    };
    mounts.iter().for_each(|edge| {
        if !router_files.contains(&edge.target_abs_posix) {
            return;
        }
        let next_prefix = normalize::join_http_paths(base_path, &edge.base_path);
        queue.push(QueueTask {
            file_path: edge.target_abs_posix.clone(),
            base_path: next_prefix,
            is_root: false,
        });
    });
}

fn sort_and_dedupe_index(index: &mut RouteIndex) {
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
}
