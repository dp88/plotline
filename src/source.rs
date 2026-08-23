//! Sequence storage interfaces.

use alloc::format;
use alloc::string::String;

use crate::context::Context;
use crate::step::{Progress, StepFacts};

/// An opaque sequence handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SequenceRef(u64);

impl SequenceRef {
    /// Creates a handle from a raw payload.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the raw payload.
    #[must_use]
    pub const fn to_raw(self) -> u64 {
        self.0
    }
}

/// Read-only sequence facts for analysis and editors.
pub trait SequenceFacts {
    /// Returns the step count, or `None` if the sequence is missing.
    fn step_count(&mut self, sequence: SequenceRef) -> Option<usize>;

    /// Returns one step's facts, or `None` if the sequence or step is missing.
    fn step_facts(&mut self, sequence: SequenceRef, index: usize) -> Option<StepFacts>;

    /// Returns a display name.
    fn name(&mut self, sequence: SequenceRef) -> String {
        format!("seq#{:x}", sequence.to_raw())
    }
}

/// Sequence facts plus step execution.
pub trait SequenceSource: SequenceFacts {
    /// Starts one step. Returns `None` if the sequence or step is missing.
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
