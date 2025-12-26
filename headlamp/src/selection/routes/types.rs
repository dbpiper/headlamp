use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RouteFrameworkId {
    Express,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalRoute {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MountEdge {
    pub base_path: String,
    pub target_abs_posix: String,
}

#[derive(Debug, Clone, Default)]
pub struct FileRouteFacts {
    pub abs_path_posix: String,

    pub has_root_container: bool,
    pub exports_router: bool,

    pub root_routes: Vec<LocalRoute>,
    pub router_routes: Vec<LocalRoute>,

    pub root_mounts: Vec<MountEdge>,
    pub router_mounts: Vec<MountEdge>,
}

impl FileRouteFacts {
    pub fn is_empty(&self) -> bool {
        !self.has_root_container
            && !self.exports_router
            && self.root_routes.is_empty()
            && self.router_routes.is_empty()
            && self.root_mounts.is_empty()
            && self.router_mounts.is_empty()
    }

    pub fn referenced_router_files(&self) -> BTreeSet<String> {
        self.root_mounts
            .iter()
            .chain(self.router_mounts.iter())
            .map(|edge| edge.target_abs_posix.clone())
            .collect::<BTreeSet<_>>()
    }
}
