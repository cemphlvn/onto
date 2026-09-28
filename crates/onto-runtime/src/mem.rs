//! Memory measurement: exact heap bytes from a counting allocator, process
//! RSS from the OS, and a sampler that emits both as telemetry.
//!
//! Heap counting needs the binary to install the allocator:
//!
//! ```ignore
//! #[global_allocator]
//! static ALLOC: onto_runtime::mem::CountingAlloc = onto_runtime::mem::CountingAlloc;
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use std::time::Duration;

use serde::Serialize;

static HEAP: AtomicUsize = AtomicUsize::new(0);
static HEAP_PEAK: AtomicUsize = AtomicUsize::new(0);
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Wraps the system allocator and tracks live and peak heap bytes.
pub struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged to the system allocator.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            grow(layout.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged to the system allocator.
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            grow(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was allocated by `System` with this `layout`.
        unsafe { System.dealloc(ptr, layout) };
        HEAP.fetch_sub(layout.size(), Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: `ptr` was allocated by `System` with this `layout`.
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            if new_size >= layout.size() {
                grow(new_size - layout.size());
            } else {
                HEAP.fetch_sub(layout.size() - new_size, Relaxed);
            }
        }
        p
    }
}

fn grow(by: usize) {
    INSTALLED.store(true, Relaxed);
    let now = HEAP.fetch_add(by, Relaxed) + by;
    HEAP_PEAK.fetch_max(now, Relaxed);
}

/// One memory reading. Heap fields are `None` when `CountingAlloc` is not
/// the global allocator; `rss_bytes` is `None` where the OS query fails.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct MemSample {
    pub rss_bytes: Option<usize>,
    pub heap_bytes: Option<usize>,
    pub heap_peak_bytes: Option<usize>,
}

/// Resident memory of this process; a browser does not report it.
fn rss() -> Option<usize> {
    #[cfg(not(target_arch = "wasm32"))]
    return memory_stats::memory_stats().map(|m| m.physical_mem);
    #[cfg(target_arch = "wasm32")]
    None
}

pub fn sample() -> MemSample {
    let counted = INSTALLED.load(Relaxed);
    MemSample {
        rss_bytes: rss(),
        heap_bytes: counted.then(|| HEAP.load(Relaxed)),
        heap_peak_bytes: counted.then(|| HEAP_PEAK.load(Relaxed)),
    }
}

/// Emits a `mem.sample` event every `every` until the returned handle is
/// aborted, and tracks the peak RSS it saw.
pub fn spawn_sampler(every: Duration) -> Sampler {
    let peak_rss = std::sync::Arc::new(AtomicUsize::new(0));
    let peak = peak_rss.clone();
    let task = crate::rt::spawn(async move {
        loop {
            let s = sample();
            if let Some(rss) = s.rss_bytes {
                peak.fetch_max(rss, Relaxed);
            }
            tracing::info!(
                target: "onto",
                event = "mem.sample",
                rss_bytes = s.rss_bytes,
                heap_bytes = s.heap_bytes,
                heap_peak_bytes = s.heap_peak_bytes,
            );
            crate::rt::sleep(every).await;
        }
    });
    Sampler { task, peak_rss }
}

pub struct Sampler {
    task: crate::rt::JoinHandle<()>,
    peak_rss: std::sync::Arc<AtomicUsize>,
}

impl Sampler {
    /// Stops sampling; returns the peak RSS observed (including a final read).
    pub fn stop(self) -> Option<usize> {
        self.task.abort();
        let last = sample().rss_bytes.unwrap_or(0);
        let peak = self.peak_rss.load(Relaxed).max(last);
        (peak > 0).then_some(peak)
    }
}
