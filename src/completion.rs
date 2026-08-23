//! The one way a step waits on the outside world.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};

/// A one-shot completion handle: the only wait primitive in this crate.
///
/// A step that cannot finish immediately returns [`Progress::Wait`] holding one of these,
/// and hands a clone to whatever will finish later — a dialog panel, an animation, a timer
/// owned by the host. When that something calls [`signal`], the next [`Runner::advance`]
/// moves past the step.
///
/// Two properties carry the safety story:
///
/// - **Orphan-safe.** A handle left behind by a stopped chain is inert: nothing polls it any
///   more, so a late [`signal`] from a lingering callback is a harmless no-op rather than a
///   use-after-free.
/// - **Idempotent.** Signaling twice is the same as signaling once; there is no way to
///   "un-complete" a handle.
///
/// Cloning is cheap and clones share the same state — signal any clone, and every holder
/// sees it. The signal may come from another thread.
///
/// [`Progress::Wait`]: crate::Progress::Wait
/// [`Runner::advance`]: crate::Runner::advance
/// [`signal`]: Completion::signal
///
/// # Examples
///
/// ```
/// use plotline::Completion;
///
/// let completion = Completion::new();
/// let handle = completion.clone(); // give this to whatever finishes later
///
/// assert!(!completion.is_complete());
/// handle.signal();
/// assert!(completion.is_complete());
/// handle.signal(); // idempotent: signaling again changes nothing
/// ```
#[derive(Clone, Debug, Default)]
pub struct Completion(Arc<AtomicBool>);

impl Completion {
    /// A pending completion: [`is_complete`](Completion::is_complete) is `false` until
    /// someone calls [`signal`](Completion::signal).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An already-complete handle, for steps that discover mid-`start` that there is
    /// nothing to wait for. Waiting on it never blocks.
    #[must_use]
    pub fn done() -> Self {
        let completion = Self::new();
        completion.signal();
        completion
    }

    /// Marks the handle complete. Idempotent; safe to call from any thread; safe to call
    /// on a handle nobody is waiting on any more.
    pub fn signal(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Whether [`signal`](Completion::signal) has been called on this handle or any clone
    /// of it.
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
        // The docs promise this is the one type that does; a change that broke it would
        // otherwise only show up in a downstream build.
        const fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Completion>();
    }

    #[test]
    fn orphaned_handle_signal_is_harmless() {
        // A chain that stopped holding the waiting side simply drops it; a late signal
        // touches only the shared flag.
        let c = Completion::new();
        let orphan = c.clone();
        drop(c);
        orphan.signal();
        assert!(orphan.is_complete());
    }
}
