use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;

use headlamp::parallel_stride::run_parallel_stride;

#[test]
fn run_parallel_stride_runs_all_items_once() {
    let items = (0usize..17).collect::<Vec<_>>();
    let seen = Arc::new(
        (0..items.len())
            .map(|_| AtomicUsize::new(0))
            .collect::<Vec<_>>(),
    );

    let out = run_parallel_stride(&items, 3, |value, _index| {
        let slot = &seen[*value];
        slot.fetch_add(1, Ordering::SeqCst);
        Ok::<_, ()>(*value)
    })
    .unwrap();

    assert_eq!(out.len(), items.len());
    for slot in &*seen {
        assert_eq!(slot.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn run_parallel_stride_respects_concurrency_upper_bound() {
    let items = (0usize..9).collect::<Vec<_>>();
    let worker_count = 3usize;

    let barrier = Arc::new(Barrier::new(worker_count));
    let in_flight = Arc::new(AtomicUsize::new(0));
    let max_in_flight = Arc::new(AtomicUsize::new(0));

    let _ = run_parallel_stride(&items, worker_count, |_value, index| {
        if index < worker_count {
            barrier.wait();
        }
        let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        max_in_flight.fetch_max(now, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(30));
        in_flight.fetch_sub(1, Ordering::SeqCst);
        Ok::<_, ()>(())
    })
    .unwrap();

    let observed = max_in_flight.load(Ordering::SeqCst);
    assert!(
        observed <= worker_count,
        "observed={observed} worker_count={worker_count}"
    );
}
