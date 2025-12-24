use std::sync::{Arc, Mutex};

pub fn run_parallel_stride<T, R, E, F>(items: &[T], concurrency: usize, run: F) -> Result<Vec<R>, E>
where
    T: Sync,
    R: Send,
    E: Send,
    F: Fn(&T, usize) -> Result<R, E> + Sync,
{
    let total = items.len();
    if total == 0 {
        return Ok(vec![]);
    }
    let worker_count = std::cmp::max(1, std::cmp::min(concurrency, total));

    let results_by_index: Arc<Vec<Mutex<Option<R>>>> =
        Arc::new((0..total).map(|_| Mutex::new(None)).collect());
    let first_error: Arc<Mutex<Option<E>>> = Arc::new(Mutex::new(None));

    std::thread::scope(|scope| {
        let run_ref = &run;
        let items_ref = items;
        for start_index in 0..worker_count {
            let results_by_index = Arc::clone(&results_by_index);
            let first_error = Arc::clone(&first_error);
            scope.spawn(move || {
                let mut index = start_index;
                while index < total {
                    if first_error.lock().ok().is_some_and(|g| g.is_some()) {
                        return;
                    }
                    match run_ref(&items_ref[index], index) {
                        Ok(value) => {
                            if let Ok(mut slot) = results_by_index[index].lock() {
                                *slot = Some(value);
                            }
                        }
                        Err(err) => {
                            if let Ok(mut slot) = first_error.lock()
                                && slot.is_none()
                            {
                                *slot = Some(err);
                            };
                            return;
                        }
                    }
                    index += worker_count;
                }
            });
        }
    });

    if let Some(err) = first_error.lock().ok().and_then(|mut g| g.take()) {
        return Err(err);
    }

    let mut out: Vec<R> = Vec::with_capacity(total);
    for index in 0..total {
        let maybe = results_by_index[index]
            .lock()
            .ok()
            .and_then(|mut g| g.take());
        if let Some(value) = maybe {
            out.push(value);
        }
    }
    Ok(out)
}
