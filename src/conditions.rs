//! Built-in conditions.

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::vocab::{Condition, QueryCtx};

/// A fixed boolean answer. The default is `true`.
#[derive(Clone, Copy, Debug)]
pub struct Always {
    /// The fixed answer.
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

/// A condition created from a closure.
pub struct Check<F> {
    name: String,
    body: F,
}

/// Wraps a closure as a condition.
///
/// The closure receives the read-only query context and returns the condition's answer.
///
/// ```
/// use plotline::{Condition, QueryCtx, TypeMap, conditions};
///
/// let caps = TypeMap::new();
/// let condition = conditions::check("Has a target", |query| query.target.is_some());
/// assert!(!condition.evaluate(&QueryCtx {
///     target: None,
///     chain: None,
///     caps: &caps,
/// }));
/// ```
pub fn check<F>(name: impl Into<String>, body: F) -> Check<F>
where
    F: for<'a, 'b> Fn(&'a QueryCtx<'b>) -> bool,
{
    Check {
        name: name.into(),
        body,
    }
}

impl<F> Condition for Check<F>
where
    F: for<'a, 'b> Fn(&'a QueryCtx<'b>) -> bool,
{
    fn summary(&self) -> String {
        self.name.clone()
    }

    fn warning(&self) -> Option<String> {
        self.name
            .trim()
            .is_empty()
            .then(|| "No name set; this condition is anonymous in inspectors.".to_owned())
    }

    fn evaluate(&self, query: &QueryCtx<'_>) -> bool {
        (self.body)(query)
    }
}

/// Inverts a condition. A missing condition evaluates to `false`.
#[derive(Default)]
pub struct Not {
    /// The condition to invert.
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
            Some(inner) => !inner.evaluate(query),
            None => false,
        }
    }
}

/// Creates an inverted condition.
#[must_use]
pub fn not(condition: impl Condition + 'static) -> Not {
    Not {
        inner: Some(Box::new(condition)),
    }
}

/// True when every inner condition is true. An empty list is true.
#[derive(Default)]
pub struct All {
    /// Conditions to evaluate.
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

/// Creates a condition that requires every supplied condition.
#[must_use]
pub fn all(conditions: impl IntoIterator<Item = Box<dyn Condition>>) -> All {
    All {
        conditions: conditions.into_iter().collect(),
    }
}

/// True when any inner condition is true. An empty list is false.
#[derive(Default)]
pub struct Any {
    /// Conditions to evaluate.
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

/// Creates a condition that accepts any supplied condition.
#[must_use]
pub fn any(conditions: impl IntoIterator<Item = Box<dyn Condition>>) -> Any {
    Any {
        conditions: conditions.into_iter().collect(),
    }
}

/// Reads a chain flag. Outside a chain, it evaluates to `false`.
#[derive(Clone, Debug, Default)]
pub struct Flag {
    /// Flag name.
    pub name: String,
    /// Expected value.
    pub expected: bool,
}

impl Flag {
    /// Creates a condition that expects the flag to be set.
    #[must_use]
    pub fn is_set(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            expected: true,
        }
    }

    /// Creates a condition that expects the flag to be clear.
    #[must_use]
    pub fn is_clear(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            expected: false,
        }
    }
}

/// Creates a condition that expects the flag to be set.
#[must_use]
pub fn flag(name: impl Into<String>) -> Flag {
    Flag::is_set(name)
}

/// Creates a condition that expects the flag to be clear.
#[must_use]
pub fn flag_clear(name: impl Into<String>) -> Flag {
    Flag::is_clear(name)
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
    fn closure_condition_reports_and_evaluates() {
        let caps = TypeMap::new();
        let condition = check("Has a target", |query| query.target.is_some());
        assert_eq!(condition.summary(), "Has a target");
        assert!(!condition.evaluate(&bare_query(&caps)));
        assert!(condition.warning().is_none());
        assert!(check("", |_query| true).warning().is_some());
    }

    #[test]
    fn flag_without_chain_reads_false() {
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
