use super::*;
use std::sync::atomic::AtomicUsize;

#[cfg(test)]
static ACTIVE_HIGHLIGHTER_WORKERS: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub(super) struct ActiveHighlighterWorkerGuard;

#[cfg(test)]
impl ActiveHighlighterWorkerGuard {
    pub(super) fn new() -> Self {
        ACTIVE_HIGHLIGHTER_WORKERS.fetch_add(1, Ordering::AcqRel);
        Self
    }
}

#[cfg(test)]
impl Drop for ActiveHighlighterWorkerGuard {
    fn drop(&mut self) {
        ACTIVE_HIGHLIGHTER_WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
pub(super) fn active_highlighter_worker_count() -> usize {
    ACTIVE_HIGHLIGHTER_WORKERS.load(Ordering::Acquire)
}
