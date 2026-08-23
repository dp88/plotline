//! A one-shot wait handle.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};

/// A thread-safe, one-shot completion handle.
///
/// Clones share state. Signaling is idempotent and safe after the runner drops its wait.
///
/// ```
/// use plotline::Completion;
///
/// let completion = Completion::new();
/// let waiter = completion.clone();
/// assert!(!completion.is_complete());
/// waiter.signal();
/// assert!(completion.is_complete());
/// ```
#[derive(Clone, Debug, Default)]
pub struct Completion(Arc<AtomicBool>);

impl Completion {
    /// Creates a pending handle.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an already-complete handle.
    #[must_use]
    pub fn done() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }

    /// Marks the handle complete.
    pub fn signal(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Returns whether the handle has been signaled.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn new_is_pending() {
        assert!(!Completion::new().is_complete());
    }

    #[test]
    fn done_is_already_complete() {
        assert!(Completion::done().is_complete());
    }

    #[test]
    fn clones_share_state() {
        let a = Completion::new();
        let b = a.clone();
        b.signal();
        assert!(a.is_complete());
    }

    #[test]
    fn signal_is_idempotent() {
        let c = Completion::new();
        c.signal();
        c.signal();
        assert!(c.is_complete());
    }

    #[test]
    fn signal_from_another_thread_is_seen() {
        let c = Completion::new();
        let handle = c.clone();
        std::thread::spawn(move || handle.signal()).join().unwrap();
        assert!(c.is_complete());
    }

    #[test]
    fn completion_crosses_threads() {
        const fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Completion>();
    }

    #[test]
    fn orphaned_handle_signal_is_harmless() {
        let c = Completion::new();
        let orphan = c.clone();
        drop(c);
        orphan.signal();
        assert!(orphan.is_complete());
    }
}
