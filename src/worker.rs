//! The engine's two threads. `raven_call` may block (`node.start` can take a
//! few seconds, API.md), so the GTK app never calls the engine on the main
//! thread: every call is a job sent over a channel to an engine thread, and
//! each job returns a [`Reply`], a small future the GTK main loop awaits
//! (`glib::spawn_future_local`); glib's wakers may be woken from any thread.
//!
//! There are two lanes, both over the same engine (`raven_call` is
//! thread-safe, API.md):
//!
//! - **calls** (thread `raven-engine`): every operation except `events.poll`,
//!   one after another in the order they were sent, so a job that makes
//!   several calls that belong together, or calls that depend on each other
//!   (save a setting, then read it), behave as on one thread;
//! - **events** (thread `raven-events`): only `events.poll`, which "never
//!   blocks" (API.md), so a poll never waits behind a slow call such as
//!   `node.start` and the heartbeat and incoming messages keep arriving.
//!
//! Standard library only (no new crates), and GTK-free, so it is tested with
//! `cargo test --lib --no-default-features`.

use std::future::Future;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Waker};
use std::thread;

use crate::api::{self, ApiError, RavenEngine};

type Job = Box<dyn FnOnce(&dyn RavenEngine) + Send>;

/// Thread names, also seen in debuggers and `top -H`.
pub const CALLS_THREAD: &str = "raven-engine";
pub const EVENTS_THREAD: &str = "raven-events";

/// The engine threads and the sending ends of their job queues.
pub struct EngineWorker {
    calls: Mutex<Option<mpsc::Sender<Job>>>,
    events: Mutex<Option<mpsc::Sender<Job>>>,
    threads: Mutex<Vec<thread::JoinHandle<()>>>,
}

/// Runs the jobs of one lane in order. A panic in one job loses only that
/// job's answer (its Reply resolves to `None`); the lane keeps serving.
fn serve(jobs: mpsc::Receiver<Job>, engine: &dyn RavenEngine) {
    for job in jobs {
        let _ = catch_unwind(AssertUnwindSafe(|| job(engine)));
    }
}

impl EngineWorker {
    /// Start both threads. `make` builds the engine on the calls thread; the
    /// events thread shares it once it is built.
    pub fn start<F>(make: F) -> Self
    where
        F: FnOnce() -> Box<dyn RavenEngine> + Send + 'static,
    {
        let (calls_tx, calls_rx) = mpsc::channel::<Job>();
        let (events_tx, events_rx) = mpsc::channel::<Job>();
        let (engine_tx, engine_rx) = mpsc::channel::<Arc<dyn RavenEngine>>();
        let calls = thread::Builder::new()
            .name(CALLS_THREAD.into())
            .spawn(move || {
                let engine: Arc<dyn RavenEngine> = Arc::from(make());
                let _ = engine_tx.send(engine.clone());
                drop(engine_tx);
                serve(calls_rx, engine.as_ref());
            })
            .expect("the engine thread starts");
        let events = thread::Builder::new()
            .name(EVENTS_THREAD.into())
            .spawn(move || {
                // No engine (building it panicked): queued polls answer `None`.
                if let Ok(engine) = engine_rx.recv() {
                    serve(events_rx, engine.as_ref());
                }
            })
            .expect("the events thread starts");
        Self {
            calls: Mutex::new(Some(calls_tx)),
            events: Mutex::new(Some(events_tx)),
            threads: Mutex::new(vec![calls, events]),
        }
    }

    /// Run `job` on the calls lane, after every job sent before it. The reply
    /// resolves to `Some(result)`, or to `None` when the worker stopped before
    /// the job ran (or the job panicked).
    pub fn run<T, F>(&self, job: F) -> Reply<T>
    where
        T: Send + 'static,
        F: FnOnce(&dyn RavenEngine) -> T + Send + 'static,
    {
        submit(&self.calls, job)
    }

    /// Like [`EngineWorker::run`] for a call that returns a `Result`: a
    /// stopped worker becomes an `INTERNAL` error instead of `None`.
    pub fn call<T, F>(&self, job: F) -> impl Future<Output = Result<T, ApiError>> + 'static
    where
        T: Send + 'static,
        F: FnOnce(&dyn RavenEngine) -> Result<T, ApiError> + Send + 'static,
    {
        flatten(self.run(job))
    }

    /// `events.poll` on its own lane: it never waits behind a slow call on
    /// the calls lane (API.md: "`events.poll` never blocks"). Polls run in
    /// the order they were sent; the app sends one at a time anyway.
    pub fn poll_events(
        &self,
        after_seq: u64,
    ) -> impl Future<Output = Result<api::Poll, ApiError>> + 'static {
        flatten(submit(&self.events, move |e: &dyn RavenEngine| {
            e.events_poll(after_seq)
        }))
    }

    /// Stop taking jobs, let the queued ones finish, and wait for both
    /// threads. Called when the app quits; safe to call more than once.
    pub fn shutdown(&self) {
        drop(lock(&self.calls).take());
        drop(lock(&self.events).take());
        let handles: Vec<_> = lock(&self.threads).drain(..).collect();
        for h in handles {
            if h.thread().id() != thread::current().id() {
                let _ = h.join();
            }
        }
    }
}

impl Drop for EngineWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Queue `job` on one lane; a stopped lane drops it with its sender (`None`).
fn submit<T, F>(lane: &Mutex<Option<mpsc::Sender<Job>>>, job: F) -> Reply<T>
where
    T: Send + 'static,
    F: FnOnce(&dyn RavenEngine) -> T + Send + 'static,
{
    let (reply, sender) = Reply::pair();
    let boxed: Job = Box::new(move |engine| sender.send(job(engine)));
    if let Some(tx) = lock(lane).as_ref() {
        let _ = tx.send(boxed);
    }
    reply
}

/// A reply that never came (stopped worker, panicking call) is an
/// `INTERNAL` error for the caller.
async fn flatten<T>(reply: Reply<Result<T, ApiError>>) -> Result<T, ApiError> {
    reply
        .await
        .unwrap_or_else(|| Err(ApiError::internal("the engine thread stopped")))
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

struct Slot<T> {
    value: Option<T>,
    closed: bool,
    waker: Option<Waker>,
}

/// The answer to one job: a future that resolves once, on the thread that
/// awaits it.
pub struct Reply<T> {
    slot: Arc<Mutex<Slot<T>>>,
}

struct ReplySender<T> {
    slot: Arc<Mutex<Slot<T>>>,
}

impl<T> Reply<T> {
    fn pair() -> (Reply<T>, ReplySender<T>) {
        let slot = Arc::new(Mutex::new(Slot {
            value: None,
            closed: false,
            waker: None,
        }));
        (Reply { slot: slot.clone() }, ReplySender { slot })
    }
}

impl<T> ReplySender<T> {
    fn send(self, value: T) {
        let waker = {
            let mut s = lock(&self.slot);
            s.value = Some(value);
            s.closed = true;
            s.waker.take()
        };
        if let Some(w) = waker {
            w.wake();
        }
    }
}

impl<T> Drop for ReplySender<T> {
    fn drop(&mut self) {
        let waker = {
            let mut s = lock(&self.slot);
            if s.closed {
                return;
            }
            s.closed = true;
            s.waker.take()
        };
        if let Some(w) = waker {
            w.wake();
        }
    }
}

impl<T> Future for Reply<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let mut s = lock(&self.slot);
        if let Some(v) = s.value.take() {
            return Poll::Ready(Some(v));
        }
        if s.closed {
            return Poll::Ready(None);
        }
        s.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::codes;
    use serde_json::{json, Value};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Wake;
    use std::time::Duration;

    /// Wakes the test thread (a tiny `block_on`).
    struct ThreadWaker(thread::Thread);

    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    fn block_on<F: Future>(f: F) -> F::Output {
        block_on_for(f, Duration::from_secs(30)).expect("the answer came within 30 s")
    }

    /// `None` when `f` is not ready within `limit` (a test never hangs).
    fn block_on_for<F: Future>(f: F, limit: Duration) -> Option<F::Output> {
        let mut f = Box::pin(f);
        let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
        let mut cx = Context::from_waker(&waker);
        let start = std::time::Instant::now();
        loop {
            if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
                return Some(v);
            }
            if start.elapsed() > limit {
                return None;
            }
            thread::park_timeout(Duration::from_millis(50));
        }
    }

    /// Records the order of calls and the thread they ran on.
    struct Recorder {
        calls: Mutex<Vec<String>>,
        threads: Mutex<Vec<thread::ThreadId>>,
    }

    impl RavenEngine for Recorder {
        fn call(&self, op: &str, args: Value) -> Result<Value, ApiError> {
            lock(&self.calls).push(op.to_string());
            lock(&self.threads).push(thread::current().id());
            match op {
                // `seq` = how many calls the engine has seen, this one included;
                // the thread name says which lane ran it.
                "events.poll" => match args["after_seq"].as_u64() {
                    Some(99) => panic!("engine bug in a poll"),
                    _ => Ok(json!({
                        "seq": lock(&self.calls).len(),
                        "events": [],
                        "thread": thread::current().name(),
                    })),
                },
                "slow" => {
                    thread::sleep(Duration::from_millis(30));
                    Ok(json!({ "slow": true }))
                }
                "fail" => Err(ApiError::new(codes::BAD_REQUEST, "no", "detail")),
                "panic" => panic!("engine bug"),
                "seen" => Ok(json!({ "calls": lock(&self.calls).len() })),
                _ => Ok(json!({ "op": op })),
            }
        }
    }

    fn recorder() -> EngineWorker {
        EngineWorker::start(|| {
            Box::new(Recorder {
                calls: Mutex::new(Vec::new()),
                threads: Mutex::new(Vec::new()),
            })
        })
    }

    #[test]
    fn results_come_back_in_order_from_one_other_thread() {
        let w = recorder();
        // Queued while the first job still sleeps: they run after it, in order.
        let replies: Vec<_> = ["slow", "a", "b", "c"]
            .into_iter()
            .map(|op| w.call(move |e| e.call(op, json!({}))))
            .collect();
        let seen = w.call(|e| e.call("seen", json!({})));
        let results: Vec<Value> = replies.into_iter().map(|r| block_on(r).unwrap()).collect();
        assert_eq!(results[0], json!({ "slow": true }));
        assert_eq!(results[3], json!({ "op": "c" }));
        assert_eq!(block_on(seen).unwrap(), json!({ "calls": 5 }));
        let threads = block_on(w.run(|_| thread::current().id())).unwrap();
        assert_ne!(threads, thread::current().id(), "not the caller's thread");
    }

    #[test]
    fn a_poll_never_waits_behind_a_slow_call() {
        let w = recorder();
        // A "node.start" that takes as long as the test wants, then another call.
        let (release, gate) = mpsc::channel::<()>();
        let slow = w.call(move |e| {
            let _ = gate.recv();
            e.call("node.start", json!({}))
        });
        let after = w.call(|e| e.call("after", json!({})));
        // The calls lane is busy, yet the poll answers at once; the engine has
        // seen only the poll (seq 1): "node.start" is still waiting at the gate.
        let p = block_on_for(w.poll_events(0), Duration::from_secs(5))
            .expect("the poll did not wait for node.start")
            .unwrap();
        assert_eq!(p.seq, 1);
        assert!(p.events.is_empty());
        let p = block_on_for(w.poll_events(p.seq), Duration::from_secs(5))
            .expect("a second poll too")
            .unwrap();
        assert_eq!(p.seq, 2);
        release.send(()).unwrap();
        assert_eq!(block_on(slow).unwrap(), json!({ "op": "node.start" }));
        assert_eq!(block_on(after).unwrap(), json!({ "op": "after" }));
        let seen = block_on(w.call(|e| e.call("seen", json!({})))).unwrap();
        assert_eq!(seen, json!({ "calls": 5 }));
    }

    #[test]
    fn calls_keep_their_order_while_polls_run_beside_them() {
        let w = recorder();
        let mut calls = Vec::new();
        let mut polls = Vec::new();
        for i in 0..20u64 {
            let op = if i % 5 == 0 { "slow" } else { "x" };
            calls.push(w.run(move |e| {
                let _ = e.call(op, json!({}));
                (i, thread::current().name().map(str::to_string))
            }));
            polls.push(w.poll_events(i));
        }
        let order: Vec<u64> = calls
            .into_iter()
            .map(|r| {
                let (i, name) = block_on(r).unwrap();
                assert_eq!(name.as_deref(), Some(CALLS_THREAD));
                i
            })
            .collect();
        assert_eq!(
            order,
            (0..20).collect::<Vec<_>>(),
            "calls run in the order sent"
        );
        for p in polls {
            assert!(block_on(p).is_ok());
        }
        // Polls run on their own thread, never on the calls thread.
        let lane = block_on(w.poll_events(0)).unwrap();
        let raw = block_on(w.run(|e| e.call("events.poll", json!({ "after_seq": 0 }))))
            .unwrap()
            .unwrap();
        assert_eq!(
            raw["thread"],
            json!(CALLS_THREAD),
            "a plain call stays on the calls lane"
        );
        assert!(lane.seq > 0);
        let name = block_on(submit(&w.events, |_| {
            thread::current().name().map(str::to_string)
        }))
        .unwrap();
        assert_eq!(name.as_deref(), Some(EVENTS_THREAD));
    }

    #[test]
    fn errors_and_panics_reach_the_caller() {
        let w = recorder();
        let e = block_on(w.call(|e| e.call("fail", json!({})))).unwrap_err();
        assert!(e.is(codes::BAD_REQUEST));
        // A panicking call answers INTERNAL; the thread keeps serving.
        let e = block_on(w.call(|e| e.call("panic", json!({})))).unwrap_err();
        assert!(e.is(codes::INTERNAL));
        assert_eq!(e.message, crate::strings::tr("error.internal"));
        let ok = block_on(w.call(|e| e.call("after", json!({})))).unwrap();
        assert_eq!(ok, json!({ "op": "after" }));
        // The same on the events lane: a panicking poll answers INTERNAL and
        // the next poll works.
        let e = block_on(w.poll_events(99)).unwrap_err();
        assert!(e.is(codes::INTERNAL));
        assert!(block_on(w.poll_events(0)).is_ok());
    }

    #[test]
    fn shutdown_finishes_queued_jobs_then_refuses_new_ones() {
        let w = recorder();
        let done = Arc::new(AtomicUsize::new(0));
        let replies: Vec<_> = (0..3)
            .map(|_| {
                let done = done.clone();
                w.run(move |e| {
                    let _ = e.call("slow", json!({}));
                    done.fetch_add(1, Ordering::SeqCst);
                })
            })
            .collect();
        w.shutdown();
        assert_eq!(done.load(Ordering::SeqCst), 3, "queued jobs ran first");
        for r in replies {
            assert_eq!(block_on(r), Some(()));
        }
        assert_eq!(block_on(w.run(|_| 1)), None, "no thread any more");
        let e = block_on(w.call(|e| e.call("x", json!({})))).unwrap_err();
        assert!(e.is(codes::INTERNAL));
        let e = block_on(w.poll_events(0)).unwrap_err();
        assert!(e.is(codes::INTERNAL), "no events thread either");
        w.shutdown(); // twice is fine
    }

    #[cfg(debug_assertions)]
    #[test]
    fn the_mock_behaves_the_same_behind_the_worker() {
        use crate::api::ProfileStateKind;
        use crate::mock::{Clock, MockRaven, RENA_DEMO_OLD_CODE};
        let t = Arc::new(std::sync::atomic::AtomicU64::new(1_792_505_100_000));
        let clock = Clock::Manual(t.clone());
        let w = EngineWorker::start(move || Box::new(MockRaven::with_clock(clock, true)));
        let state = block_on(w.call(|e| e.profile_state())).unwrap();
        assert_eq!(state.state, ProfileStateKind::Ready);
        let contacts = block_on(w.call(|e| e.contacts_list())).unwrap();
        let rena = contacts
            .iter()
            .find(|c| c.name == "Rena")
            .unwrap()
            .address
            .clone();
        let a = rena.clone();
        let outcome = block_on(w.call(move |e| e.verify_check_words(&a, &[])));
        assert!(outcome.unwrap_err().is(codes::WORDS_INCOMPLETE));
        let a = rena.clone();
        let r = block_on(w.call(move |e| e.verify_check_old_code(&a, RENA_DEMO_OLD_CODE)));
        assert_eq!(r.unwrap(), crate::api::OldCodeOutcome::Match);
        let verified = block_on(w.call(|e| e.contacts_list()))
            .unwrap()
            .into_iter()
            .any(|c| c.address == rena && c.verified);
        assert!(verified);
        // The mock's own clock still drives the demo events, polled on the
        // events lane.
        let p = block_on(w.poll_events(0)).unwrap();
        t.fetch_add(1_200, Ordering::SeqCst);
        let p2 = block_on(w.poll_events(p.seq)).unwrap();
        assert!(!p2.events.is_empty());
    }
}
