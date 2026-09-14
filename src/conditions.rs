//! Built-in conditions.
//!
//! Boolean composition lives in [`Requirement`](crate::Requirement), which is
//! data rather than boxed closures. Use [`check`] for a one-off Rust closure,
//! and [`Checks`] to give closures names that a stored requirement can call.

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;

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

    /// Removes a condition. Returns whether the registry changed.
    pub fn remove(&mut self, name: &str) -> bool {
        self.entries.remove(name).is_some()
    }

    /// Returns the registered condition.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&dyn Condition> {
        self.entries.get(name).map(AsRef::as_ref)
    }

    /// Returns whether a name is registered.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Iterates over the registered names in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// Returns the number of registered conditions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the first registered condition that reports a warning.
    #[must_use]
    pub fn warning(&self) -> Option<String> {
        self.entries.iter().find_map(|(name, condition)| {
            condition
                .warning()
                .map(|warning| alloc::format!("Check '{name}': {warning}"))
        })
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

    fn bare_query(caps: &TypeMap) -> QueryCtx<'_> {
        QueryCtx {
            target: None,
            chain: None,
            caps,
        }
    }

    #[test]
    fn always_reports_its_value() {
        let caps = TypeMap::new();
        assert!(Always::default().evaluate(&bare_query(&caps)));
        assert!(!Always { value: false }.evaluate(&bare_query(&caps)));
        assert_eq!(Always::default().summary(), "Always");
        assert_eq!(Always { value: false }.summary(), "Never");
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
    fn explain_defaults_to_one_leaf() {
        let caps = TypeMap::new();
        let explanation = Always::default().explain(&bare_query(&caps));
        assert_eq!(explanation.summary, "Always");
        assert!(explanation.satisfied);
        assert!(explanation.children.is_empty());
    }

    #[test]
    fn registry_stores_and_finds_conditions() {
        let caps = TypeMap::new();
        let mut checks = Checks::new();
        assert!(checks.is_empty());
        assert!(!checks.register("yes", Always::default()));
        assert!(checks.register("yes", Always { value: false }), "replaced");
        assert_eq!(checks.len(), 1);
        assert!(checks.contains("yes"));
        assert!(!checks.get("yes").unwrap().evaluate(&bare_query(&caps)));
        assert!(checks.get("no").is_none());
        assert!(checks.remove("yes"));
        assert!(!checks.remove("yes"));
    }

    #[test]
    fn registry_names_are_ordered() {
        let mut checks = Checks::new();
        checks.register("b", Always::default());
        checks.register("a", Always::default());
        assert_eq!(checks.names().collect::<Vec<_>>(), ["a", "b"]);
    }

    #[test]
    fn registry_reports_a_member_warning() {
        let mut checks = Checks::new();
        checks.register("anonymous", check("", |_query| true));
        assert!(checks.warning().unwrap().contains("anonymous"));
    }
}
