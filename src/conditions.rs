//! Built-in conditions: the composites and the chain-flag reader.
//!
//! Use them module-qualified — `conditions::All`, `conditions::Not` — the way the
//! variants of an enum read.

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::vocab::{Condition, QueryCtx};

/// A fixed answer: `true` by default, or `false` when authored as "Never".
///
/// Useful as a placeholder while authoring and as the explicit "no gate" in content that
/// wants to say so out loud.
#[derive(Clone, Copy, Debug)]
pub struct Always {
    /// The answer this condition always gives.
    pub value: bool,
}

impl Default for Always {
    fn default() -> Self {
        Self { value: true }
    }
}

impl Condition for Always {
    fn summary(&self) -> String {
        if self.value { "Always" } else { "Never" }.to_owned()
    }

    fn evaluate(&self, _query: &QueryCtx<'_>) -> bool {
        self.value
    }
}

/// Inverts its inner condition.
///
/// A missing inner condition evaluates `false` — failing *closed*, rather than opening a
/// gate that content meant to keep shut. [`warning`](Condition::warning) reports it.
#[derive(Default)]
pub struct Not {
    /// The condition to invert; `None` is an authoring error.
    pub inner: Option<Box<dyn Condition>>,
}

impl Condition for Not {
    fn summary(&self) -> String {
        match &self.inner {
            Some(inner) => format!("Not ({})", inner.summary()),
            None => "Not (missing condition)".to_owned(),
        }
    }

    fn warning(&self) -> Option<String> {
        self.inner
            .is_none()
            .then(|| "No condition to invert; this evaluates false.".to_owned())
    }

    fn evaluate(&self, query: &QueryCtx<'_>) -> bool {
        match &self.inner {
            // No inner condition means no gate to invert. Failing closed is the safe
            // half of the guess; `warning()` is what tells an author about it.
            Some(inner) => !inner.evaluate(query),
            None => false,
        }
    }
}

/// True when every inner condition is true. **Empty means true** — no requirements.
///
/// Short-circuits on the first false.
#[derive(Default)]
pub struct All {
    /// The requirements, all of which must hold.
    pub conditions: Vec<Box<dyn Condition>>,
}

impl Condition for All {
    fn summary(&self) -> String {
        format!("All of {}", self.conditions.len())
    }

    fn evaluate(&self, query: &QueryCtx<'_>) -> bool {
        self.conditions.iter().all(|c| c.evaluate(query))
    }
}

/// True when at least one inner condition is true. **Empty means false** — nothing was
/// offered that could be true.
///
/// Short-circuits on the first true.
#[derive(Default)]
pub struct Any {
    /// The alternatives, one of which must hold.
    pub conditions: Vec<Box<dyn Condition>>,
}

impl Condition for Any {
    fn summary(&self) -> String {
        format!("Any of {}", self.conditions.len())
    }

    fn evaluate(&self, query: &QueryCtx<'_>) -> bool {
        self.conditions.iter().any(|c| c.evaluate(query))
    }
}

/// Reads a chain-local blackboard flag set earlier by a step or an effect.
///
/// Outside a running chain there is no blackboard to read, so this evaluates `false`.
/// (The C# equivalent crashed on a null context; the `Option` in [`QueryCtx::chain`] is
/// that bug made impossible.)
#[derive(Clone, Debug, Default)]
pub struct Flag {
    /// The flag to read.
    pub name: String,
    /// The value that makes this condition true. Defaults to `false`, so an authored
    /// condition usually sets it; the [`is_set`](Flag::is_set) constructor sets `true`.
    pub expected: bool,
}

impl Flag {
    /// A condition that is true when `name` is set.
    #[must_use]
    pub fn is_set(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            expected: true,
        }
    }
}

impl Condition for Flag {
    fn summary(&self) -> String {
        format!("Flag '{}' is {}", self.name, self.expected)
    }

    fn warning(&self) -> Option<String> {
        self.name
            .trim()
            .is_empty()
            .then(|| "No flag name set.".to_owned())
    }

    fn evaluate(&self, query: &QueryCtx<'_>) -> bool {
        match query.chain {
            Some(chain) => chain.flag(&self.name) == self.expected,
            // No chain, no blackboard. The `Option` on `QueryCtx::chain` is what makes
            // a caller acknowledge this; answering false is the safe half.
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::borrow::ToOwned;
    use alloc::boxed::Box;

    use alloc::string::String;
    use alloc::vec;

    use super::*;
    use crate::context::{ChainFlags, TypeMap};

    fn bare_query(caps: &TypeMap) -> QueryCtx<'_> {
        QueryCtx {
            target: None,
            chain: None,
            caps,
        }
    }

    /// A condition that counts its evaluations, for short-circuit proofs.
    struct Counting {
        answer: bool,
        count: alloc::rc::Rc<core::cell::Cell<usize>>,
    }
    impl Counting {
        fn new(answer: bool) -> (Self, alloc::rc::Rc<core::cell::Cell<usize>>) {
            let count = alloc::rc::Rc::new(core::cell::Cell::new(0));
            (
                Self {
                    answer,
                    count: count.clone(),
                },
                count,
            )
        }
    }
    impl Condition for Counting {
        fn summary(&self) -> String {
            "counting".to_owned()
        }
        fn evaluate(&self, _query: &QueryCtx<'_>) -> bool {
            self.count.set(self.count.get() + 1);
            self.answer
        }
    }

    #[test]
    fn all_of_empty_is_true() {
        let caps = TypeMap::new();
        assert!(All::default().evaluate(&bare_query(&caps)));
    }

    #[test]
    fn any_of_empty_is_false() {
        let caps = TypeMap::new();
        assert!(!Any::default().evaluate(&bare_query(&caps)));
    }

    #[test]
    fn not_missing_inner_fails_closed() {
        let caps = TypeMap::new();
        assert!(!Not::default().evaluate(&bare_query(&caps)));
        assert!(Not::default().warning().is_some());
    }

    #[test]
    fn not_inverts() {
        let caps = TypeMap::new();
        let not = Not {
            inner: Some(Box::new(Always { value: true })),
        };
        assert!(!not.evaluate(&bare_query(&caps)));
    }

    #[test]
    fn all_short_circuits_on_first_false() {
        let caps = TypeMap::new();
        let (counting, count) = Counting::new(true);
        let all = All {
            conditions: vec![Box::new(Always { value: false }), Box::new(counting)],
        };
        assert!(!all.evaluate(&bare_query(&caps)));
        assert_eq!(count.get(), 0, "second condition never evaluated");
    }

    #[test]
    fn any_short_circuits_on_first_true() {
        let caps = TypeMap::new();
        let (counting, count) = Counting::new(false);
        let any = Any {
            conditions: vec![Box::new(Always { value: true }), Box::new(counting)],
        };
        assert!(any.evaluate(&bare_query(&caps)));
        assert_eq!(count.get(), 0, "second condition never evaluated");
    }

    #[test]
    fn flag_without_chain_reads_false() {
        // The fixed C# NRE: a trigger gate evaluating a chain flag outside a chain gets
        // a warning and `false`, not a crash.
        let caps = TypeMap::new();
        assert!(!Flag::is_set("accepted").evaluate(&bare_query(&caps)));
    }

    #[test]
    fn flag_reads_the_blackboard() {
        let caps = TypeMap::new();
        let mut chain = ChainFlags::new();
        chain.set_flag("accepted", true);
        let query = QueryCtx {
            target: None,
            chain: Some(&chain),
            caps: &caps,
        };
        assert!(Flag::is_set("accepted").evaluate(&query));
        assert!(!Flag::is_set("refused").evaluate(&query));
    }
}
