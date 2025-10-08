# Latched State + Callback Map (A)

A practical pattern for jobs that should accept **add/remove of continuations (callbacks) at runtime**, and also **run late-registered continuations immediately** once the job is already finished.

---

## Goals

* Allow dynamic registration/removal of named continuations while a job is running.
* On job completion, **fan out** to all currently registered continuations.
* If a continuation is registered **after** completion, run it **immediately** ("latched" behavior).
* Provide two add APIs:

  * `add_or_run_now(...)` — add when running, or run immediately if already done.
  * `add_or_error_if_already_stopped(...)` — add when running, or return an error if already done.

## High-Level Design

* Maintain a **latched state**: `Running` | `Done(Result<(), Error>)`.
* Maintain a thread-safe **map of continuations** keyed by string IDs.
* When the underlying task completes, set state to `Done(...)` and **drain** the map, spawning each continuation.
* Adding a continuation checks the state:

  * If `Running`: insert into the map.
  * If `Done`: either run immediately (for `add_or_run_now`) or return an error (for `add_or_error_if_already_stopped`).
* Removing a continuation simply deletes it from the map (no-op if not present).

---

## Core Types

```rust
use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};
use tokio::sync::RwLock; // or parking_lot::RwLock
use anyhow::Error;

pub struct LeTaskData; // your job-specific data

/// Type-erased async continuation: (job_task, result) -> Future<Output=()>.
/// We box the future to store heterogeneous async closures in a single map.
pub type ContFn = Arc<
    dyn Fn(Arc<LeTaskData>, Result<(), Error>) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

#[derive(Clone, Debug)]
pub enum JobState {
    Running,
    Done(Result<(), Error>),
}

pub struct Continuations {
    state: RwLock<JobState>,
    map: RwLock<HashMap<String, ContFn>>, // name -> callback
}
```

---

## API Surface

```rust
impl Continuations {
    pub fn new() -> Self;

    /// Helper to box any async closure into `ContFn`.
    pub fn boxed<F, Fut>(f: F) -> ContFn
    where
        F: Fn(Arc<LeTaskData>, Result<(), Error>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static;

    /// Add a continuation if running; otherwise run it immediately.
    pub async fn add_or_run_now(
        &self,
        key: impl Into<String>,
        callback: ContFn,
        job_task: Arc<LeTaskData>,
    );

    /// Add a continuation only if running; otherwise return an error with the final result.
    pub async fn add_or_error_if_already_stopped(
        &self,
        key: impl Into<String>,
        callback: ContFn,
    ) -> Result<(), AlreadyStopped>;

    /// Remove a continuation by key. Returns true if present.
    pub async fn remove(&self, key: &str) -> bool;

    /// Mark complete and fan out to all registered continuations.
    pub async fn complete(&self, result: Result<(), Error>, job_task: Arc<LeTaskData>);

    /// Convenience: check if done and obtain the latched result.
    pub async fn is_done(&self) -> Option<Result<(), Error>>;
}
```

---

## Implementation

```rust
#[derive(thiserror::Error, Debug)]
pub enum AlreadyStopped {
    #[error("job already completed: {0:?}")]
    Completed(Result<(), Error>),
}

impl Continuations {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(JobState::Running),
            map: RwLock::new(HashMap::new()),
        }
    }

    pub fn boxed<F, Fut>(f: F) -> ContFn
    where
        F: Fn(Arc<LeTaskData>, Result<(), Error>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Arc::new(move |task, res| Box::pin(f(task, res)))
    }

    pub async fn add_or_run_now(
        &self,
        key: impl Into<String>,
        callback: ContFn,
        job_task: Arc<LeTaskData>,
    ) {
        // Fast path: read state without holding the map lock.
        match &*self.state.read().await {
            JobState::Running => {
                self.map.write().await.insert(key.into(), callback);
            }
            JobState::Done(res) => {
                // LATCHED: run immediately, outside locks, in its own task.
                let res = res.clone();
                tokio::spawn(callback(job_task, res));
            }
        }
    }

    pub async fn add_or_error_if_already_stopped(
        &self,
        key: impl Into<String>,
        callback: ContFn,
    ) -> Result<(), AlreadyStopped> {
        let st = self.state.read().await;
        match &*st {
            JobState::Running => {
                drop(st); // release read lock before taking write
                self.map.write().await.insert(key.into(), callback);
                Ok(())
            }
            JobState::Done(res) => Err(AlreadyStopped::Completed(res.clone())),
        }
    }

    pub async fn remove(&self, key: &str) -> bool {
        self.map.write().await.remove(key).is_some()
    }

    pub async fn complete(&self, result: Result<(), Error>, job_task: Arc<LeTaskData>) {
        // 1) Latch the result
        {
            let mut st = self.state.write().await;
            *st = JobState::Done(result.clone());
        }
        // 2) Drain callbacks atomically, then run outside the lock
        let callbacks = {
            let mut map = self.map.write().await;
            std::mem::take(&mut *map)
        };
        for (_name, cb) in callbacks {
            let task = Arc::clone(&job_task);
            let res = result.clone();
            tokio::spawn(cb(task, res));
        }
    }

    pub async fn is_done(&self) -> Option<Result<(), Error>> {
        match &*self.state.read().await {
            JobState::Running => None,
            JobState::Done(r) => Some(r.clone()),
        }
    }
}
```

**Notes**

* Continuations run in independent spawned tasks so a slow/failing one doesn’t block others.
* We **never** run callbacks while holding the map lock—avoids deadlocks and reentrancy hazards.
* `add_or_error_if_already_stopped` is handy when callers need to know the job has ended and **not** run their logic anymore (unlike `add_or_run_now`).

---

## Integrating with a Job (`LeJob`)

```rust
use tokio::task::JoinHandle;

pub struct LeJob {
    pub task: Arc<LeTaskData>,
    pub continuations: Arc<Continuations>,
}

impl LeJob {
    /// Create a job and hook its completion to the continuations latch.
    pub fn new(
        task: Arc<LeTaskData>,
        handle: JoinHandle<Result<(), Error>>,
    ) -> Arc<Self> {
        let job = Arc::new(Self {
            task: Arc::clone(&task),
            continuations: Arc::new(Continuations::new()),
        });

        // Forward completion into the latch.
        let conts = Arc::clone(&job.continuations);
        let task_for_conts = Arc::clone(&job.task);
        tokio::spawn(async move {
            let outcome = match handle.await {
                Ok(r) => r,
                Err(join_err) => {
                    if join_err.is_cancelled() {
                        Err(anyhow::anyhow!("task aborted"))
                    } else if join_err.is_panic() {
                        Err(anyhow::anyhow!("task panicked"))
                    } else {
                        Err(anyhow::anyhow!("join error: {join_err}"))
                    }
                }
            };
            conts.complete(outcome, task_for_conts).await;
        });

        job
    }
}
```

> If you need **abort** control after wiring the forwarder, consider:
>
> * Pass a `tokio_util::sync::CancellationToken` into the worker task and keep it on `LeJob` for explicit cancellations.
> * Or store the original `JoinHandle` behind a `Mutex<Option<...>>` so the forwarder can `take()` it; callers can still `abort()` as long as they do so before the forwarder takes ownership.

---

## Usage Examples

### 1) Add callbacks that will run on completion

```rust
let job: Arc<LeJob> = jobs_host.run_new_job(...);

job.continuations
   .add_or_run_now(
       "UI_NOTIF",
       Continuations::boxed(|task, res| async move {
           some_fancy_ui_notification(task, res).await;
       }),
       Arc::clone(&job.task),
   )
   .await;

job.continuations
   .add_or_run_now(
       "NETWORK_NOTIF",
       Continuations::boxed(|task, res| async move {
           some_fancy_network_notification(task, res).await;
       }),
       Arc::clone(&job.task),
   )
   .await;
```

### 2) Remove a continuation before completion

```rust
let removed = job.continuations.remove("NETWORK_NOTIF").await;
```

### 3) Late add — runs immediately (latched)

```rust
// Suppose the job already completed.
job.continuations
   .add_or_run_now(
       "LATE_UI",
       Continuations::boxed(|task, res| async move {
           // This runs right away with the latched result.
           some_fancy_ui_notification(task, res).await;
       }),
       Arc::clone(&job.task),
   )
   .await;
```

### 4) Error-on-late add

```rust
match job
    .continuations
    .add_or_error_if_already_stopped(
        "ONLY_WHEN_RUNNING",
        Continuations::boxed(|_task, _res| async move { /* ... */ }),
    )
    .await
{
    Ok(()) => { /* registered */ }
    Err(AlreadyStopped::Completed(final_res)) => {
        // Decide what to do knowing the job is over and with which result.
        eprintln!("not registered; job ended with: {final_res:?}");
    }
}
```

---

## Testing Tips

* Write a test where the job completes and ensure all registered callbacks fire once.
* Register callbacks **after** completion and assert they run immediately for `add_or_run_now` but **not** for `add_or_error_if_already_stopped`.
* Remove a callback prior to completion and assert it does not run.
* Inject a slow callback to ensure others still run (independently spawned).

---

## Extensions

* **Filters:** store metadata per continuation (e.g., only run on `Ok(())` or only on error).
* **Timeouts/Retries:** wrap the callback future before spawning.
* **Metrics/Tracing:** record number of registered/removed/fired callbacks; log callback durations.
* **Backpressure:** if callbacks may be heavy, send them to a bounded worker queue instead of spawning directly.

---

# Part 2 — Rust Concepts Used

### `Arc<T>`

Shared ownership pointer for thread-safe reference counting. We wrap `LeTaskData` and the `Continuations` registry in `Arc` so multiple tasks/threads can use them concurrently without copying the underlying data.

### `Send` and `Sync`

* `Send`: a type can be transferred across thread boundaries.
* `Sync`: a type can be referenced from multiple threads concurrently (i.e., `&T` is `Send`).
  We bound callbacks and futures with `Send` so they can be executed by Tokio worker threads.

### `RwLock` vs `Mutex`

* `RwLock` allows many readers or one writer — good for state checks like `Running` vs `Done` and occasional writes.
* `Mutex` is simpler but serializes all access; either is fine here. Using async locks from `tokio::sync` avoids blocking the runtime.

### Trait Objects and `dyn Fn(...)`

We store **heterogeneous** continuations (different closure types) in one map using a **trait object**: `Arc<dyn Fn(...) -> ... + Send + Sync>`. This erases the concrete type of each closure while retaining a uniform call interface.

### Boxing Futures: `Pin<Box<dyn Future<Output=()> + Send>>`

Async closures yield anonymous future types. To store them in a single map, we **box** them behind `Pin<Box<...>>`, which:

* Allocates on the heap, giving a stable memory address.
* Satisfies the `Future` trait object’s requirements (pinned, sized) for dynamic dispatch.

### `tokio::spawn`

Schedules an async task onto the Tokio runtime. We spawn each continuation so they run concurrently and independently from one another and from the caller.

### Latching Pattern

We set a **final, immutable outcome** (`Done(Result<..>)`) and keep it accessible for late visitors. This is akin to a `OnceCell/OnceLock` but we also need a **drainable registry**, so we keep both the state and the map.

### Join Semantics and Errors

Awaiting a `JoinHandle<Result<(), Error>>` yields:

* `Ok(inner)` — the task returned its own `Result<(), Error>`.
* `Err(join_err)` — the task **did not** return normally (cancelled/aborted or panicked). We translate that into an `anyhow::Error` and publish it.

### Avoiding Lock Poisoning / Deadlocks

* Never run callbacks while holding the map lock; we first `take()` the map, then spawn tasks.
* Prefer short lock scopes. Acquire a read lock for checking state, drop it before taking a write lock.

### Lifetimes and `'static`

We require `'static` on the closure and future so they can outlive the current stack frame and be safely moved into background tasks.

---

## Why this approach?

* **Deterministic fan-out** on completion.
* **Dynamic control**: add/remove by name.
* **Late subscriber behavior** is explicit: either run immediately or reject with an error.
* Minimal dependencies, purely on Tokio + standard concurrency primitives.

> Alternatives: `tokio::sync::watch` (state replay to late listeners), `broadcast` (ephemeral events to many live listeners), or `Notify` + `OnceLock` (very light custom build). The latched map gives you keyed management plus replayable behavior in a single place.
