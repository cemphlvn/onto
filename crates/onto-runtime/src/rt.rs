//! The executor the engine runs on (D70, `docs/11-platforms.md`): tokio on
//! native targets, the browser's event loop on `wasm32`. The engine asks
//! this module to spawn, sleep, read the clock and collect tasks; the
//! synchronization it uses (`tokio::sync`) needs no runtime.
//!
//! On `wasm32` everything runs on one thread: walks still interleave while
//! a model call is pending, and nothing blocks the page.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;
#[cfg(target_arch = "wasm32")]
pub use web_time::Instant;

/// Waits for `d` without blocking the executor.
pub async fn sleep(d: Duration) {
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(d).await;
    #[cfg(target_arch = "wasm32")]
    SingleThread(gloo_timers::future::sleep(d)).await;
}

/// A browser future (a timer, a JS promise) made usable where the model
/// interfaces ask for `Send`. Sound only because wasm32 without the
/// `atomics` feature has exactly one thread; a threaded wasm build does
/// not compile this.
#[cfg(target_arch = "wasm32")]
pub struct SingleThread<F>(pub F);

#[cfg(all(target_arch = "wasm32", not(target_feature = "atomics")))]
unsafe impl<F> Send for SingleThread<F> {}

#[cfg(target_arch = "wasm32")]
impl<F: Future> Future for SingleThread<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        // Structural pinning: the inner future is never moved out.
        unsafe { self.map_unchecked_mut(|s| &mut s.0) }.poll(cx)
    }
}

/// A spawned task's result: `Err` when the task panicked or was aborted.
pub struct JoinHandle<T> {
    #[cfg(not(target_arch = "wasm32"))]
    inner: tokio::task::JoinHandle<T>,
    #[cfg(target_arch = "wasm32")]
    inner: tokio::sync::oneshot::Receiver<T>,
    #[cfg(target_arch = "wasm32")]
    abort: std::rc::Rc<std::cell::Cell<bool>>,
}

#[derive(Debug)]
pub struct JoinError;

impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("task panicked or was aborted")
    }
}

impl std::error::Error for JoinError {}

impl<T> JoinHandle<T> {
    /// Stops the task at its next await point.
    pub fn abort(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.inner.abort();
        #[cfg(target_arch = "wasm32")]
        self.abort.set(true);
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.inner).poll(cx).map_err(|_| JoinError)
    }
}

/// Runs `fut` concurrently with the caller.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn<F>(fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    JoinHandle {
        inner: tokio::spawn(fut),
    }
}

/// Runs `fut` concurrently with the caller, on the page's event loop.
#[cfg(target_arch = "wasm32")]
pub fn spawn<F>(fut: F) -> JoinHandle<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    let abort = std::rc::Rc::new(std::cell::Cell::new(false));
    let aborted = abort.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let fut = std::pin::pin!(fut);
        let out = Abortable { fut, aborted }.await;
        if let Some(out) = out {
            let _ = tx.send(out);
        }
    });
    JoinHandle { inner: rx, abort }
}

/// Resolves to `None` once aborted, checked each time the task is polled.
#[cfg(target_arch = "wasm32")]
struct Abortable<F> {
    fut: F,
    aborted: std::rc::Rc<std::cell::Cell<bool>>,
}

#[cfg(target_arch = "wasm32")]
impl<F: Future + Unpin> Future for Abortable<F> {
    type Output = Option<F::Output>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.aborted.get() {
            return Poll::Ready(None);
        }
        Pin::new(&mut self.fut).poll(cx).map(Some)
    }
}

/// A set of tasks whose results arrive in completion order.
pub struct JoinSet<T> {
    #[cfg(not(target_arch = "wasm32"))]
    inner: tokio::task::JoinSet<T>,
    #[cfg(target_arch = "wasm32")]
    tx: tokio::sync::mpsc::UnboundedSender<T>,
    #[cfg(target_arch = "wasm32")]
    rx: tokio::sync::mpsc::UnboundedReceiver<T>,
    #[cfg(target_arch = "wasm32")]
    pending: usize,
}

impl<T: 'static> Default for JoinSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: 'static> JoinSet<T> {
    pub fn new() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        return Self {
            inner: tokio::task::JoinSet::new(),
        };
        #[cfg(target_arch = "wasm32")]
        {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            Self { tx, rx, pending: 0 }
        }
    }

    /// The next finished task's result; `None` when the set is empty.
    pub async fn join_next(&mut self) -> Option<Result<T, JoinError>> {
        #[cfg(not(target_arch = "wasm32"))]
        return self
            .inner
            .join_next()
            .await
            .map(|r| r.map_err(|_| JoinError));
        #[cfg(target_arch = "wasm32")]
        {
            // A panic aborts a wasm instance, so every task reports.
            if self.pending == 0 {
                return None;
            }
            let out = self.rx.recv().await?;
            self.pending -= 1;
            Some(Ok(out))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + 'static> JoinSet<T> {
    pub fn spawn<F>(&mut self, fut: F)
    where
        F: Future<Output = T> + Send + 'static,
    {
        self.inner.spawn(fut);
    }
}

#[cfg(target_arch = "wasm32")]
impl<T: 'static> JoinSet<T> {
    pub fn spawn<F>(&mut self, fut: F)
    where
        F: Future<Output = T> + 'static,
    {
        self.pending += 1;
        let tx = self.tx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = tx.send(fut.await);
        });
    }
}
