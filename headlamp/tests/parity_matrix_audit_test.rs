#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum Status {
    Done,
    Partial,
    Missing,
}

#[derive(Debug, Clone)]
struct Item {
    area: &'static str,
    name: &'static str,
    ts_source: &'static str,
    rust_location: &'static str,
    status: Status,
}

#[test]
fn parity_matrix_audit() {
    let items = vec![
        Item {
            area: "args",
            name: "Token parsing for flags + defaults",
            ts_source: "headlamp-original/src/lib/args.ts",
            rust_location: "headlamp_core/src/args.rs",
            status: Status::Partial,
        },
        Item {
            area: "config",
            name: "c12 config resolution + TS/JS configs",
            ts_source: "headlamp-original/src/lib/config.ts",
            rust_location: "headlamp_core/src/config.rs",
            status: Status::Partial,
        },
        Item {
            area: "discovery",
            name: "Per-project jest.config discovery + ownership filtering",
            ts_source: "headlamp-original/src/lib/jest-config.ts + discovery.ts",
            rust_location: "headlamp/src/jest_config.rs + jest_discovery.rs + jest_ownership.rs + jest.rs",
            status: Status::Partial,
        },
        Item {
            area: "discovery",
            name: "Jest --listTests timeout + ripgrep fallback",
            ts_source: "headlamp-original/src/lib/discovery.ts (discoverJestResilient)",
            rust_location: "headlamp/src/jest_discovery.rs + headlamp/src/jest.rs",
            status: Status::Done,
        },
        Item {
            area: "discovery",
            name: "Per-project Jest list caching (git HEAD + status in cache key)",
            ts_source: "headlamp-original/src/lib/discovery.ts (discoverJestCached)",
            rust_location: "headlamp/src/jest_discovery.rs (discover_jest_list_tests_cached_with_timeout) + headlamp/src/jest.rs",
            status: Status::Done,
        },
        Item {
            area: "selection",
            name: "Fast related tests (ripgrep seeds + cache keyed by git HEAD)",
            ts_source: "headlamp-original/src/lib/fast-related.ts",
            rust_location: "headlamp/src/fast_related.rs (rg-based matching + seed-term parity + cache root/key parity)",
            status: Status::Partial,
        },
        Item {
            area: "selection",
            name: "Directness rank + http route augmentation",
            ts_source: "headlamp-original/src/lib/relevance.ts + routeGraph.ts",
            rust_location: "headlamp/src/jest.rs (compute_directness_rank_base) + headlamp_core/src/selection/route_index.rs + headlamp_core/src/selection/relevance.rs",
            status: Status::Partial,
        },
        Item {
            area: "format",
            name: "Vitest-like rendering from Jest bridge JSON",
            ts_source: "headlamp-original/src/lib/formatter/*",
            rust_location: "headlamp_core/src/format/* + headlamp/src/jest.rs (partial)",
            status: Status::Partial,
        },
        Item {
            area: "coverage",
            name: "Coverage filtering + ordering + composite tables/hotspots matching TS UX",
            ts_source: "headlamp-original/src/lib/coverage-*.ts",
            rust_location: "headlamp_core/src/coverage/* (simplified)",
            status: Status::Partial,
        },
        Item {
            area: "exec",
            name: "Project stride concurrency + live progress UI",
            ts_source: "headlamp-original/src/lib/program.ts + parallel.ts",
            rust_location: "headlamp/src/parallel_stride.rs + headlamp/src/live_progress.rs + headlamp/src/jest.rs (stride=3)",
            status: Status::Partial,
        },
    ];

    let missing = items
        .iter()
        .filter(|i| i.status != Status::Done)
        .collect::<Vec<_>>();

    eprintln!("\n=== Parity audit (non-Done items) ===");
    for it in &missing {
        eprintln!(
            "- [{}] {} :: {} (TS: {}) (Rust: {})",
            match it.status {
                Status::Done => "done",
                Status::Partial => "partial",
                Status::Missing => "missing",
            },
            it.area,
            it.name,
            it.ts_source,
            it.rust_location
        );
    }

    // This is an audit-only test: it must not fail builds, but it must surface gaps.
    assert!(true);
}
