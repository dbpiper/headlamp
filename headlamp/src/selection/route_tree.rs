use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RouteSegmentKind {
    Literal,
    Param,
    Splat,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RouteSegment {
    pub segment: String,
    pub kind: RouteSegmentKind,
    pub param_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RouteTrieNode<A> {
    pub segment: RouteSegment,
    pub handlers: BTreeMap<String, A>,
    pub children: Vec<RouteTrieNode<A>>,
}

#[derive(Debug, Clone)]
pub struct RouteTrie<A> {
    pub root: RouteTrieNode<A>,
}

pub fn empty_route_trie<A>() -> RouteTrie<A> {
    RouteTrie {
        root: RouteTrieNode {
            segment: RouteSegment {
                segment: String::new(),
                kind: RouteSegmentKind::Literal,
                param_name: None,
            },
            handlers: BTreeMap::new(),
            children: vec![],
        },
    }
}

pub fn parse_http_segments(http_path: &str) -> Vec<RouteSegment> {
    http_path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|seg| {
            if seg == "*" {
                RouteSegment {
                    segment: seg.to_string(),
                    kind: RouteSegmentKind::Splat,
                    param_name: None,
                }
            } else if let Some(param) = seg.strip_prefix(':') {
                RouteSegment {
                    segment: seg.to_string(),
                    kind: RouteSegmentKind::Param,
                    param_name: Some(param.to_string()),
                }
            } else {
                RouteSegment {
                    segment: seg.to_string(),
                    kind: RouteSegmentKind::Literal,
                    param_name: None,
                }
            }
        })
        .collect()
}

fn upsert_handler<A: Clone>(
    handlers: &BTreeMap<String, A>,
    method: &str,
    value: &A,
) -> BTreeMap<String, A> {
    let mut next = handlers.clone();
    next.insert(method.to_ascii_uppercase(), value.clone());
    next
}

fn kind_rank(kind: &RouteSegmentKind) -> u8 {
    match kind {
        RouteSegmentKind::Literal => 0,
        RouteSegmentKind::Param => 1,
        RouteSegmentKind::Splat => 2,
    }
}

fn compare_route_nodes<A>(left: &RouteTrieNode<A>, right: &RouteTrieNode<A>) -> std::cmp::Ordering {
    let left_rank = kind_rank(&left.segment.kind);
    let right_rank = kind_rank(&right.segment.kind);
    left_rank
        .cmp(&right_rank)
        .then_with(|| left.segment.segment.cmp(&right.segment.segment))
}

fn node_matches_segment<A>(candidate: &RouteTrieNode<A>, head: &RouteSegment) -> bool {
    match (&candidate.segment.kind, &head.kind) {
        (RouteSegmentKind::Literal, RouteSegmentKind::Literal) => {
            candidate.segment.segment == head.segment
        }
        (RouteSegmentKind::Param, RouteSegmentKind::Param) => {
            candidate.segment.param_name == head.param_name
        }
        (RouteSegmentKind::Splat, RouteSegmentKind::Splat) => true,
        _ => false,
    }
}

fn insert_segments<A: Clone>(
    node: &RouteTrieNode<A>,
    segments: &[RouteSegment],
    method: &str,
    value: &A,
) -> RouteTrieNode<A> {
    let Some((head, tail)) = segments.split_first() else {
        return RouteTrieNode {
            segment: node.segment.clone(),
            handlers: upsert_handler(&node.handlers, method, value),
            children: node.children.clone(),
        };
    };

    let existing_index = node
        .children
        .iter()
        .position(|candidate| node_matches_segment(candidate, head));
    let next_child = match existing_index {
        Some(index) => insert_segments(&node.children[index], tail, method, value),
        None => {
            let fresh = RouteTrieNode {
                segment: head.clone(),
                handlers: BTreeMap::new(),
                children: vec![],
            };
            insert_segments(&fresh, tail, method, value)
        }
    };

    let mut next_children = node.children.clone();
    if let Some(index) = existing_index {
        next_children[index] = next_child;
    } else {
        next_children.push(next_child);
    }
    next_children.sort_by(compare_route_nodes::<A>);

    RouteTrieNode {
        segment: node.segment.clone(),
        handlers: node.handlers.clone(),
        children: next_children,
    }
}

pub fn insert_route<A: Clone>(
    trie: &RouteTrie<A>,
    segments: &[RouteSegment],
    method: &str,
    value: A,
) -> RouteTrie<A> {
    RouteTrie {
        root: insert_segments(&trie.root, segments, method, &value),
    }
}

pub fn collect_route_handlers<A: Clone>(
    trie: &RouteTrie<A>,
    segments: &[String],
    method: &str,
) -> Vec<A> {
    fn collect_handlers<A: Clone>(
        node: &RouteTrieNode<A>,
        segments: &[String],
        method: &str,
        mut accumulated: Vec<A>,
    ) -> (Vec<A>, usize, i32) {
        let method_key = method.to_ascii_uppercase();
        if let Some(value) = node.handlers.get(&method_key) {
            accumulated.push(value.clone());
        } else if let Some(value) = node.handlers.get("*") {
            accumulated.push(value.clone());
        }

        if segments.is_empty() {
            return (accumulated, 0, i32::MAX);
        }
        let head = &segments[0];
        let tail = &segments[1..];

        let mut candidates: Vec<(Vec<A>, usize, i32)> = vec![];

        node.children
            .iter()
            .filter(|child| {
                child.segment.kind == RouteSegmentKind::Literal && child.segment.segment == *head
            })
            .for_each(|child| {
                let (handlers, matched, _priority) =
                    collect_handlers(child, tail, method, accumulated.clone());
                candidates.push((handlers, matched + 1, 0));
            });
        node.children
            .iter()
            .filter(|child| child.segment.kind == RouteSegmentKind::Param)
            .for_each(|child| {
                let (handlers, matched, _priority) =
                    collect_handlers(child, tail, method, accumulated.clone());
                candidates.push((handlers, matched + 1, 1));
            });
        node.children
            .iter()
            .filter(|child| child.segment.kind == RouteSegmentKind::Splat)
            .for_each(|child| {
                let (handlers, matched, _priority) =
                    collect_handlers(child, &[], method, accumulated.clone());
                candidates.push((handlers, matched + segments.len(), 2));
            });

        candidates
            .into_iter()
            .max_by(|(_h1, matched1, priority1), (_h2, matched2, priority2)| {
                matched1
                    .cmp(matched2)
                    .then_with(|| priority2.cmp(priority1))
            })
            .unwrap_or((accumulated, 0, i32::MAX))
    }

    let (handlers, _matched, _priority) = collect_handlers(&trie.root, segments, method, vec![]);
    handlers
}
