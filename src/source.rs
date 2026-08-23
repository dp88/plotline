//! How the runner and analysis reach sequences without owning them.
//!
//! The core's own storage is [`Library`](crate::Library); the traits here are the adapter
//! seam that lets any other storage — a database, an asset, a test double — act as
//! sequences without copying into a second representation.

use crate::context::Context;
use crate::step::{Progress, StepFacts};

/// An opaque handle to a sequence.
///
/// Steps that point at other sequences — a branch target, a subroutine — hold one of
/// these. It is minted and understood only by the [`SequenceSource`] driving the runner;
/// the core never inspects the payload. [`Library`](crate::Library) uses its arena index;
/// another storage implementation may use a database or asset identifier.
///
/// This is the arena-and-handle idiom: the C# original linked sequences by direct asset
/// reference, and a copyable handle is the ownership-friendly way to say the same thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SequenceRef(u64);

impl SequenceRef {
    /// Wraps a raw payload. Only a [`SequenceSource`] implementation should mint these;
    /// a handle is meaningless except to the source that made it.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw payload this handle was minted with.
    #[must_use]
    pub const fn to_raw(self) -> u64 {
        self.0
    }
}

/// Read-only access to sequence structure: enough for analysis and editors, nothing that
/// can execute. [`FlowModel`](crate::FlowModel) consumes exactly this.
///
/// Methods take `&mut self` because implementations may maintain caches while answering;
/// pure implementations simply don't use the mutability.
pub trait SequenceFacts {
    /// Number of steps in the sequence, or `None` when the handle does not resolve —
    /// a freed resource, a stale handle, an index that never existed.
    fn step_count(&mut self, sequence: SequenceRef) -> Option<usize>;

    /// A snapshot of one step's self-reported facts, or `None` when the sequence or the
    /// step is missing. Implementations should gather through [`StepFacts::of`] so a
    /// half-authored step that panics describing itself cannot take the caller down.
    fn step_facts(&mut self, sequence: SequenceRef, index: usize) -> Option<StepFacts>;

    /// A human-readable name for logs: `"'greeting' step 3"` starts here.
    fn name(&mut self, sequence: SequenceRef) -> String {
        format!("seq#{:x}", sequence.to_raw())
    }
}

/// Everything [`SequenceFacts`] offers, plus the ability to execute a step.
///
/// This is what [`Runner::advance`](crate::Runner::advance) drives. Returning `None` from
/// [`start_step`](SequenceSource::start_step) means the step itself is missing. This lets
/// serialized storage tolerate a stale or unknown step while the runner logs and skips it.
pub trait SequenceSource: SequenceFacts {
    /// Begins executing one step, returning how it progressed. `None` when the sequence
    /// or step does not resolve.
    fn start_step(
        &mut self,
        sequence: SequenceRef,
        index: usize,
        ctx: &mut Context<'_>,
    ) -> Option<Progress>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_ref_round_trips_raw() {
        let r = SequenceRef::from_raw(0xDEAD_BEEF);
        assert_eq!(r.to_raw(), 0xDEAD_BEEF);
        assert_eq!(r, SequenceRef::from_raw(0xDEAD_BEEF));
    }
}
