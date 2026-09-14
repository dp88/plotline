//! Closure-backed conditions.
//!
//! Boolean composition lives in [`Requirement`](crate::Requirement), which is
//! data rather than boxed closures. Use [`check`] for a one-off Rust closure,
//! and [`Checks`] to give closures names that a stored requirement can call.

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;

use crate::vocab::{Condition, QueryCtx};

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

/// Named conditions that a stored requirement can call.
///
/// A [`Requirement`](crate::Requirement) is data, so it cannot hold a closure.
/// It holds a name instead, and
/// [`Requirement::Named`](crate::Requirement::Named) looks that name up here.
/// This is how a rule loaded from a file reaches host state that the
/// capability vocabulary does not cover.
///
/// Put the registry in the service [`TypeMap`](crate::TypeMap). An
/// unregistered name evaluates to false;
/// [`Requirement::unknown_checks`](crate::Requirement::unknown_checks) finds
/// those names before they run.
///
/// ```
/// use plotline::{QueryCtx, Rule, TypeMap, conditions};
///
/// struct Empire {
///     colonies: usize,
/// }
///
/// let mut checks = conditions::Checks::new();
/// checks.register(
///     "three-colonies",
///     conditions::check("Three or more colonies", |query| {
///         query.service::<Empire>().is_some_and(|e| e.colonies >= 3)
///     }),
/// );
///
/// let mut services = TypeMap::new();
/// services.insert(Empire { colonies: 4 });
/// services.insert(checks);
///
/// let rule = Rule::named("three-colonies");
/// assert!(rule.satisfies_in(&QueryCtx {
///     target: None,
///     chain: None,
///     caps: &services,
/// }));
/// ```
#[derive(Default)]
pub struct Checks {
    entries: BTreeMap<String, Box<dyn Condition>>,
}

impl Checks {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a condition. Returns whether it replaced an entry.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        condition: impl Condition + 'static,
    ) -> bool {
        self.entries
            .insert(name.into(), Box::new(condition))
            .is_some()
    }

    /// Returns the registered condition.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&dyn Condition> {
        self.entries.get(name).map(AsRef::as_ref)
    }

    /// Iterates over the registered names in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

impl core::fmt::Debug for Checks {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.names()).finish()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;
    use crate::context::TypeMap;

    #[test]
    fn a_closure_condition_reports_and_evaluates() {
        let caps = TypeMap::new();
        let query = QueryCtx {
            target: None,
            chain: None,
            caps: &caps,
        };

        let condition = check("Has a target", |query| query.target.is_some());
        assert_eq!(condition.summary(), "Has a target");
        assert!(!condition.evaluate(&query));
        assert!(condition.warning().is_none());
        assert!(check("", |_query| true).warning().is_some());
    }

    #[test]
    fn the_registry_stores_and_finds_conditions() {
        let caps = TypeMap::new();
        let query = QueryCtx {
            target: None,
            chain: None,
            caps: &caps,
        };

        let mut checks = Checks::new();
        assert!(!checks.register("yes", check("Yes", |_query| true)));
        assert!(
            checks.register("yes", check("No", |_query| false)),
            "replaced"
        );
        checks.register("other", check("Other", |_query| true));

        assert!(!checks.get("yes").unwrap().evaluate(&query));
        assert!(checks.get("absent").is_none());
        assert_eq!(checks.names().collect::<Vec<_>>(), ["other", "yes"]);
    }
}
