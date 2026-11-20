//! A small persistent thread pool for splitting heavy pixel work.
//!
//! Starting threads per work item is cheap enough for the occasional huge brush
//! dab, but painting a stroke can issue several such dabs per frame, and every
//! spawn costs far more than an ordinary dab. This pool starts its workers once
//! and hands them one boxed task each for the duration of a call.
//!
//! [`TaskPool::run`] blocks until every task has finished. Callers that hand a
//! worker a raw pointer into a borrowed buffer rely on that: the borrow outlives
//! every task, because the caller does not return until the pool is idle again.

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe, resume_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

/// One unit of work for the pool.
pub(crate) type Task = Box<dyn FnOnce() + Send + 'static>;

/// Upper bound on pool size. Painting becomes memory-bandwidth bound well
/// before this on consumer machines, and idle threads are cheap but not free.
const MAX_WORKERS: usize = 8;

pub(crate) struct TaskPool {
    senders: Vec<Mutex<Sender<Task>>>,
    next: AtomicUsize,
}

impl TaskPool {
    /// The process-wide pool, or `None` on a single-core machine where a pool
    /// could not help.
    pub(crate) fn global() -> Option<&'static TaskPool> {
        static POOL: OnceLock<Option<TaskPool>> = OnceLock::new();
        POOL.get_or_init(|| {
            let workers = std::thread::available_parallelism()
                .map(|count| count.get())
                .unwrap_or(1)
                .clamp(1, MAX_WORKERS);
            (workers > 1).then(|| TaskPool::new(workers))
        })
        .as_ref()
    }

    fn new(workers: usize) -> Self {
        let mut senders = Vec::with_capacity(workers);
        for index in 0..workers {
            let (sender, receiver) = mpsc::channel::<Task>();
            let spawned = std::thread::Builder::new()
                .name(format!("inkforge-stamp-{index}"))
                .spawn(move || {
                    while let Ok(task) = receiver.recv() {
                        task();
                    }
                });
            // A pool with fewer workers than requested is still useful; a
            // failed send falls back to running the task on the caller.
            if spawned.is_ok() {
                senders.push(Mutex::new(sender));
            }
        }
        Self {
            senders,
            next: AtomicUsize::new(0),
        }
    }

    /// Runs every task and returns only when all of them have finished.
    ///
    /// One task is kept for the calling thread, so a call uses one more worker
    /// than the pool holds. A panic inside a task is re-raised here rather than
    /// silently dropping part of the work.
    pub(crate) fn run(&self, mut tasks: Vec<Task>) {
        if tasks.is_empty() {
            return;
        }

        let local = tasks.pop();
        let state = Arc::new(RunState::new(tasks.len()));

        for task in tasks {
            let done = state.clone();
            let wrapped: Task = Box::new(move || {
                if let Err(payload) = catch_unwind(AssertUnwindSafe(task)) {
                    done.record_panic(payload);
                }
                done.done();
            });

            let index = self.next.fetch_add(1, Ordering::Relaxed) % self.senders.len();
            let sender = lock(&self.senders[index]);
            // The receiver only disappears if a worker panicked earlier, in
            // which case the task comes back and runs here instead.
            if let Err(mpsc::SendError(returned)) = sender.send(wrapped) {
                drop(sender);
                returned();
            }
        }

        if let Some(task) = local {
            if let Err(payload) = catch_unwind(AssertUnwindSafe(task)) {
                state.record_panic(payload);
            }
        }

        state.wait();
    }
}

/// A mutex guard that survives poisoning: the counters stay meaningful even if
/// a task panicked while they were held.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Shared completion state for one [`TaskPool::run`] call.
struct RunState {
    pending: Mutex<usize>,
    wake: Condvar,
    panic: Mutex<Option<Box<dyn Any + Send>>>,
}

impl RunState {
    fn new(pending: usize) -> Self {
        Self {
            pending: Mutex::new(pending),
            wake: Condvar::new(),
            panic: Mutex::new(None),
        }
    }

    fn done(&self) {
        let mut pending = lock(&self.pending);
        *pending = pending.saturating_sub(1);
        if *pending == 0 {
            self.wake.notify_all();
        }
    }

    fn record_panic(&self, payload: Box<dyn Any + Send>) {
        *lock(&self.panic) = Some(payload);
    }

    fn wait(&self) {
        let mut pending = lock(&self.pending);
        while *pending > 0 {
            pending = self
                .wake
                .wait(pending)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        drop(pending);

        if let Some(payload) = lock(&self.panic).take() {
            resume_unwind(payload);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Dispatch overhead is the reason this pool exists: a stroke can issue
    /// several large dabs per frame, and spawning a thread per band would cost
    /// more than the band itself for medium brushes.
    #[test]
    fn a_run_reuses_worker_threads_instead_of_spawning_them() {
        let Some(pool) = TaskPool::global() else {
            return; // single-core machine: nothing to compare
        };
        let repeats = 100;
        let workers = 8;

        let mut pooled = std::time::Duration::ZERO;
        let mut spawned = std::time::Duration::ZERO;

        for _ in 0..repeats {
            let tasks: Vec<Task> = (0..workers).map(|_| Box::new(|| {}) as Task).collect();
            let start = Instant::now();
            pool.run(tasks);
            pooled += start.elapsed();

            let start = Instant::now();
            std::thread::scope(|scope| {
                for _ in 0..workers {
                    scope.spawn(|| {});
                }
            });
            spawned += start.elapsed();
        }

        eprintln!(
            "{workers} empty tasks: pool {:?} per run, fresh threads {:?} per run",
            pooled / repeats,
            spawned / repeats
        );
        assert!(
            pooled < spawned,
            "pooled dispatch ({:?}) should beat spawning ({:?})",
            pooled / repeats,
            spawned / repeats
        );
    }
}
