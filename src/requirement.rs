//! Requirements as inspectable data.

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::any::Any;
use core::fmt::Debug;

use crate::capability::CapabilitySet;
use crate::conditions::Checks;
use crate::evaluation::Evaluation;
use crate::vocab::{Condition, Explanation, QueryCtx};

/// A capability key that cannot exist.
///
/// Use it through [`Rule`] when a requirement tree holds flags and named
/// checks but no capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Nothing {}

/// A requirement with no capability leaves.
pub type Rule = Requirement<Nothing>;

/// A requirement over capability keys, held as data.
///
/// The crate never interprets a key. It only asks whether a
/// [`CapabilitySet`] holds one. A requirement is plain data, so the host can
/// clone it, compare it, print it, store it in a file, and read its structure
/// without running it.
///
/// # Empty and degenerate cases
///
/// | Expression | Answer |
/// |---|---|
/// | `all([])` | true |
/// | `any([])` | false |
/// | `at_least(0, ..)` | true |
/// | `at_least(n, items)` where `n > items.len()` | false |
///
/// These follow ordinary boolean and set conventions, so no case returns an
/// error. [`Condition::warning`] reports the traps at author time.
///
/// # Two ways to evaluate
///
/// [`Requirement::evaluate`] takes only a capability set. It is pure, needs no
/// runner, and suits a user interface or a planner. [`Requirement::Flag`] and
/// [`Requirement::Named`] have no meaning there, so both read as false.
///
/// `Requirement` also implements [`Condition`], so a
/// [`Branch`](crate::steps::Branch) or [`when`](crate::steps::when) step can
/// hold one. That path reads chain flags and the [`Checks`] registry, so every
/// leaf works.
///
/// ```
/// use plotline::{CapabilitySet, Requirement};
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// enum Tech {
///     Fusion,
///     Warp,
///     Cloaking,
/// }
///
/// let caps = CapabilitySet::from([Tech::Fusion]);
///
/// let cruiser = Requirement::all([
///     Requirement::has(Tech::Fusion),
///     Requirement::any([Requirement::has(Tech::Warp), Requirement::has(Tech::Cloaking)]),
/// ]);
///
/// assert!(!cruiser.satisfies(&caps));
/// assert!(cruiser.satisfies(&CapabilitySet::from([Tech::Fusion, Tech::Warp])));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Requirement<K> {
    /// The capability set holds this key.
    Has(K),
    /// Every requirement holds. An empty list holds.
    All(Vec<Requirement<K>>),
    /// At least one requirement holds. An empty list fails.
    Any(Vec<Requirement<K>>),
    /// The inner requirement does not hold.
    Not(Box<Requirement<K>>),
    /// At least `count` of the requirements hold.
    AtLeast {
        /// How many must hold.
        count: usize,
        /// The requirements to count.
        requirements: Vec<Requirement<K>>,
    },
    /// A chain flag has this value. Outside a chain it reads false.
    Flag {
        /// The flag name.
        name: String,
        /// The value the flag must have.
        expected: bool,
    },
    /// A condition registered in a [`Checks`] registry. An unregistered name
    /// reads false.
    Named(String),
}

impl<K> Requirement<K> {
    /// Requires one capability.
    #[must_use]
    pub fn has(key: K) -> Self {
        Self::Has(key)
    }

    /// Requires every listed requirement. An empty list holds.
    #[must_use]
    pub fn all(requirements: impl IntoIterator<Item = Self>) -> Self {
        Self::All(requirements.into_iter().collect())
    }

    /// Requires any listed requirement. An empty list fails.
    #[must_use]
    pub fn any(requirements: impl IntoIterator<Item = Self>) -> Self {
        Self::Any(requirements.into_iter().collect())
    }

    /// Requires that the inner requirement fails.
    #[must_use]
    #[expect(
        clippy::should_implement_trait,
        reason = "the name matches the Not variant it builds"
    )]
    pub fn not(requirement: Self) -> Self {
        Self::Not(Box::new(requirement))
    }

    /// Requires at least `count` of the listed requirements.
    #[must_use]
    pub fn at_least(count: usize, requirements: impl IntoIterator<Item = Self>) -> Self {
        Self::AtLeast {
            count,
            requirements: requirements.into_iter().collect(),
        }
    }

    /// Requires a set chain flag.
    #[must_use]
    pub fn flag(name: impl Into<String>) -> Self {
        Self::Flag {
            name: name.into(),
            expected: true,
        }
    }

    /// Requires a clear chain flag.
    #[must_use]
    pub fn flag_clear(name: impl Into<String>) -> Self {
        Self::Flag {
            name: name.into(),
            expected: false,
        }
    }

    /// Requires a condition registered in a [`Checks`] registry.
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into())
    }

    /// Returns every check name this requirement uses that `checks` does not
    /// hold. Use it to validate rules loaded from a file.
    #[must_use]
    pub fn unknown_checks(&self, checks: &Checks) -> Vec<String> {
        let mut names = Vec::new();
        self.collect_unknown_checks(checks, &mut names);
        names
    }

    fn collect_unknown_checks(&self, checks: &Checks, names: &mut Vec<String>) {
        match self {
            Self::Named(name) => {
                if checks.get(name).is_none() {
                    names.push(name.clone());
                }
            }
            Self::All(items)
            | Self::Any(items)
            | Self::AtLeast {
                requirements: items,
                ..
            } => {
                for item in items {
                    item.collect_unknown_checks(checks, names);
                }
            }
            Self::Not(inner) => inner.collect_unknown_checks(checks, names),
            Self::Has(_) | Self::Flag { .. } => {}
        }
    }
}

impl<K: Ord + Clone> Requirement<K> {
    /// Returns every capability this requirement mentions, in any position.
    ///
    /// A key appears whether the requirement demands it or forbids it, so this
    /// answers "which capabilities matter here", not "which are needed".
    #[must_use]
    pub fn capabilities(&self) -> CapabilitySet<K> {
        let mut keys = CapabilitySet::new();
        self.collect_capabilities(&mut keys);
        keys
    }

    /// Returns the capabilities that must be held, whatever else happens.
    ///
    /// The walk descends [`Requirement::All`] nodes only, so it reports the
    /// same keys that [`Evaluation::missing`] reports against an empty set. A
    /// key inside [`Requirement::Any`], [`Requirement::AtLeast`], or
    /// [`Requirement::Not`] is never returned, because none of those demand
    /// one particular key.
    ///
    /// This is the hard edge set of a dependency graph.
    ///
    /// ```
    /// use plotline::Requirement;
    ///
    /// let rule = Requirement::all([
    ///     Requirement::has("fusion"),
    ///     Requirement::any([Requirement::has("warp"), Requirement::has("gate")]),
    /// ]);
    /// assert_eq!(rule.required().iter().copied().collect::<Vec<_>>(), ["fusion"]);
    /// ```
    #[must_use]
    pub fn required(&self) -> CapabilitySet<K> {
        let mut keys = CapabilitySet::new();
        self.collect_required(&mut keys);
        keys
    }

    fn collect_required(&self, keys: &mut CapabilitySet<K>) {
        match self {
            Self::Has(key) => {
                keys.insert(key.clone());
            }
            Self::All(items) => {
                for item in items {
                    item.collect_required(keys);
                }
            }
            // A choice demands no one key, and Not forbids rather than demands.
            Self::Any(_)
            | Self::AtLeast { .. }
            | Self::Not(_)
            | Self::Flag { .. }
            | Self::Named(_) => {}
        }
    }

    /// Returns the capabilities that help but are not required.
    ///
    /// These are the keys inside a [`Requirement::Any`] or a
    /// [`Requirement::AtLeast`]. The walk never enters a
    /// [`Requirement::Not`], because a forbidden key does not help.
    ///
    /// This is the soft edge set of a dependency graph.
    ///
    /// ```
    /// use plotline::Requirement;
    ///
    /// let rule = Requirement::all([
    ///     Requirement::has("fusion"),
    ///     Requirement::any([Requirement::has("warp"), Requirement::has("gate")]),
    /// ]);
    /// assert_eq!(rule.optional().iter().copied().collect::<Vec<_>>(), ["gate", "warp"]);
    /// ```
    #[must_use]
    pub fn optional(&self) -> CapabilitySet<K> {
        let mut keys = CapabilitySet::new();
        self.collect_optional(&mut keys, false);
        keys
    }

    fn collect_optional(&self, keys: &mut CapabilitySet<K>, inside_choice: bool) {
        match self {
            Self::Has(key) => {
                if inside_choice {
                    keys.insert(key.clone());
                }
            }
            Self::All(items) => {
                for item in items {
                    item.collect_optional(keys, inside_choice);
                }
            }
            Self::Any(items)
            | Self::AtLeast {
                requirements: items,
                ..
            } => {
                for item in items {
                    item.collect_optional(keys, true);
                }
            }
            // A forbidden key never helps.
            Self::Not(_) | Self::Flag { .. } | Self::Named(_) => {}
        }
    }

    fn collect_capabilities(&self, keys: &mut CapabilitySet<K>) {
        match self {
            Self::Has(key) => {
                keys.insert(key.clone());
            }
            Self::All(items)
            | Self::Any(items)
            | Self::AtLeast {
                requirements: items,
                ..
            } => {
                for item in items {
                    item.collect_capabilities(keys);
                }
            }
            Self::Not(inner) => inner.collect_capabilities(keys),
            Self::Flag { .. } | Self::Named(_) => {}
        }
    }
}

impl<K: Ord> Requirement<K> {
    /// Returns whether the capability set satisfies this requirement.
    ///
    /// [`Requirement::Flag`] and [`Requirement::Named`] read as false. This
    /// call allocates nothing and stops at the first decisive child.
    #[must_use]
    pub fn satisfies(&self, capabilities: &CapabilitySet<K>) -> bool {
        self.check(&SetOnly(capabilities))
    }

    /// Evaluates the requirement and returns the full result tree.
    ///
    /// [`Requirement::Flag`] and [`Requirement::Named`] read as false.
    #[must_use]
    pub fn evaluate(&self, capabilities: &CapabilitySet<K>) -> Evaluation<K>
    where
        K: Clone,
    {
        self.walk(&SetOnly(capabilities))
    }

    /// Returns whether a step context satisfies this requirement.
    ///
    /// Unlike [`Requirement::satisfies`], this reads chain flags and the
    /// [`Checks`] registry, so every leaf works. It is the same answer as
    /// [`Condition::evaluate`], under a name that the inherent
    /// [`Requirement::satisfies`] does not shadow.
    #[must_use]
    pub fn satisfies_in(&self, query: &QueryCtx<'_>) -> bool
    where
        K: Any,
    {
        self.check(&FromQuery(query))
    }

    /// Evaluates against a step context and returns the full result tree.
    ///
    /// Unlike [`Requirement::evaluate`], this reads chain flags and the
    /// [`Checks`] registry, so every leaf works.
    #[must_use]
    pub fn evaluate_in(&self, query: &QueryCtx<'_>) -> Evaluation<K>
    where
        K: Any + Clone,
    {
        self.walk(&FromQuery(query))
    }

    fn check(&self, resolver: &dyn Resolver<K>) -> bool {
        match self {
            Self::Has(key) => resolver.has(key),
            Self::All(items) => items.iter().all(|item| item.check(resolver)),
            Self::Any(items) => items.iter().any(|item| item.check(resolver)),
            Self::Not(inner) => !inner.check(resolver),
            Self::AtLeast {
                count,
                requirements,
            } => requirements.iter().filter(|r| r.check(resolver)).count() >= *count,
            Self::Flag { name, expected } => resolver.flag(name) == *expected,
            Self::Named(name) => resolver.named(name),
        }
    }

    fn walk(&self, resolver: &dyn Resolver<K>) -> Evaluation<K>
    where
        K: Clone,
    {
        match self {
            Self::Has(key) => Evaluation::Has {
                key: key.clone(),
                satisfied: resolver.has(key),
            },
            Self::All(items) => {
                let children: Vec<_> = items.iter().map(|item| item.walk(resolver)).collect();
                Evaluation::All {
                    satisfied: children.iter().all(Evaluation::satisfied),
                    children,
                }
            }
            Self::Any(items) => {
                let children: Vec<_> = items.iter().map(|item| item.walk(resolver)).collect();
                Evaluation::Any {
                    satisfied: children.iter().any(Evaluation::satisfied),
                    children,
                }
            }
            Self::Not(inner) => {
                let child = inner.walk(resolver);
                Evaluation::Not {
                    satisfied: !child.satisfied(),
                    child: Box::new(child),
                }
            }
            Self::AtLeast {
                count,
                requirements,
            } => {
                let children: Vec<_> = requirements
                    .iter()
                    .map(|item| item.walk(resolver))
                    .collect();
                let met = children.iter().filter(|c| c.satisfied()).count();
                Evaluation::AtLeast {
                    count: *count,
                    met,
                    satisfied: met >= *count,
                    children,
                }
            }
            Self::Flag { name, expected } => Evaluation::Opaque {
                summary: flag_summary(name, *expected),
                satisfied: resolver.flag(name) == *expected,
            },
            Self::Named(name) => Evaluation::Opaque {
                summary: resolver.named_summary(name),
                satisfied: resolver.named(name),
            },
        }
    }
}

fn flag_summary(name: &str, expected: bool) -> String {
    format!("Flag '{name}' is {expected}")
}

/// Answers the leaf questions for one evaluation.
trait Resolver<K> {
    fn has(&self, key: &K) -> bool;
    fn flag(&self, name: &str) -> bool;
    fn named(&self, name: &str) -> bool;
    fn named_summary(&self, name: &str) -> String;
}

/// Resolves against a capability set alone.
struct SetOnly<'a, K>(&'a CapabilitySet<K>);

impl<K: Ord> Resolver<K> for SetOnly<'_, K> {
    fn has(&self, key: &K) -> bool {
        self.0.contains(key)
    }

    fn flag(&self, _name: &str) -> bool {
        false
    }

    fn named(&self, _name: &str) -> bool {
        false
    }

    fn named_summary(&self, name: &str) -> String {
        format!("Check '{name}' (no registry)")
    }
}

/// Resolves against a full step context.
struct FromQuery<'a, 'b>(&'a QueryCtx<'b>);

impl<K: Ord + Any> Resolver<K> for FromQuery<'_, '_> {
    fn has(&self, key: &K) -> bool {
        self.0
            .service::<CapabilitySet<K>>()
            .is_some_and(|caps| caps.contains(key))
    }

    fn flag(&self, name: &str) -> bool {
        self.0.chain.is_some_and(|chain| chain.flag(name))
    }

    fn named(&self, name: &str) -> bool {
        self.0
            .service::<Checks>()
            .and_then(|checks| checks.get(name))
            .is_some_and(|condition| condition.evaluate(self.0))
    }

    fn named_summary(&self, name: &str) -> String {
        self.0
            .service::<Checks>()
            .and_then(|checks| checks.get(name))
            .map_or_else(
                || format!("Check '{name}' (not registered)"),
                Condition::summary,
            )
    }
}

impl<K: Ord + Any + Clone + Debug> Condition for Requirement<K> {
    fn summary(&self) -> String {
        match self {
            Self::Has(key) => format!("Has {key:?}"),
            Self::All(items) => format!("All of {}", items.len()),
            Self::Any(items) => format!("Any of {}", items.len()),
            Self::Not(inner) => format!("Not ({})", inner.summary()),
            Self::AtLeast {
                count,
                requirements,
            } => format!("At least {count} of {}", requirements.len()),
            Self::Flag { name, expected } => flag_summary(name, *expected),
            Self::Named(name) => format!("Check '{name}'"),
        }
    }

    fn warning(&self) -> Option<String> {
        match self {
            Self::Has(_) => None,
            Self::All(items) => first_child_warning(items),
            Self::Any(items) => {
                if items.is_empty() {
                    Some("An empty Any never holds.".to_owned())
                } else {
                    first_child_warning(items)
                }
            }
            Self::Not(inner) => inner
                .warning()
                .map(|warning| format!("Inner requirement: {warning}")),
            Self::AtLeast {
                count,
                requirements,
            } => {
                if *count == 0 {
                    Some("At least 0 always holds.".to_owned())
                } else if *count > requirements.len() {
                    Some(format!(
                        "Requires {count} of {}; it can never hold.",
                        requirements.len()
                    ))
                } else {
                    first_child_warning(requirements)
                }
            }
            Self::Flag { name, .. } => name
                .trim()
                .is_empty()
                .then(|| "No flag name set.".to_owned()),
            Self::Named(name) => name
                .trim()
                .is_empty()
                .then(|| "No check name set.".to_owned()),
        }
    }

    fn evaluate(&self, query: &QueryCtx<'_>) -> bool {
        self.satisfies_in(query)
    }

    fn explain(&self, query: &QueryCtx<'_>) -> Explanation {
        Explanation::from(&self.evaluate_in(query))
    }
}

fn first_child_warning<K: Ord + Any + Clone + Debug>(items: &[Requirement<K>]) -> Option<String> {
    items.iter().enumerate().find_map(|(index, item)| {
        item.warning()
            .map(|warning| format!("Requirement {index}: {warning}"))
    })
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::conditions::check;
    use crate::context::{ChainFlags, TypeMap};

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Tech {
        A,
        B,
        C,
        D,
    }

    fn caps(keys: &[Tech]) -> CapabilitySet<Tech> {
        keys.iter().copied().collect()
    }

    #[test]
    fn required_reports_only_unavoidable_keys() {
        let rule = Requirement::all([
            Requirement::has(Tech::A),
            Requirement::all([Requirement::has(Tech::B)]),
            Requirement::any([Requirement::has(Tech::C)]),
            Requirement::not(Requirement::has(Tech::D)),
        ]);
        let required: Vec<_> = rule.required().iter().copied().collect();
        assert_eq!(required, vec![Tech::A, Tech::B]);
    }

    #[test]
    fn required_matches_missing_against_an_empty_set() {
        let rule = Requirement::all([
            Requirement::has(Tech::A),
            Requirement::any([Requirement::has(Tech::B), Requirement::has(Tech::C)]),
        ]);
        let missing = rule.evaluate(&CapabilitySet::new()).missing();
        assert_eq!(rule.required().iter().copied().collect::<Vec<_>>(), missing);
    }

    #[test]
    fn optional_reports_choices_and_skips_forbidden_keys() {
        let rule = Requirement::all([
            Requirement::has(Tech::A),
            Requirement::any([Requirement::has(Tech::B), Requirement::has(Tech::C)]),
            Requirement::at_least(1, [Requirement::has(Tech::D)]),
            Requirement::not(Requirement::has(Tech::A)),
        ]);
        let optional: Vec<_> = rule.optional().iter().copied().collect();
        assert_eq!(optional, vec![Tech::B, Tech::C, Tech::D]);
    }

    #[test]
    fn optional_keeps_nested_all_inside_a_choice() {
        let rule = Requirement::any([
            Requirement::all([Requirement::has(Tech::A), Requirement::has(Tech::B)]),
            Requirement::has(Tech::C),
        ]);
        assert!(rule.required().is_empty(), "a choice demands no one key");
        assert_eq!(rule.optional().len(), 3);
    }

    #[test]
    fn required_and_optional_do_not_overlap_for_a_plain_tree() {
        let rule = Requirement::all([
            Requirement::has(Tech::A),
            Requirement::any([Requirement::has(Tech::B), Requirement::has(Tech::C)]),
        ]);
        for key in &rule.required() {
            assert!(!rule.optional().contains(key));
        }
    }

    #[test]
    fn has_reads_the_set() {
        assert!(Requirement::has(Tech::A).satisfies(&caps(&[Tech::A])));
        assert!(!Requirement::has(Tech::A).satisfies(&caps(&[])));
    }

    #[test]
    fn all_requires_every_child() {
        let rule = Requirement::all([Requirement::has(Tech::A), Requirement::has(Tech::B)]);
        assert!(rule.satisfies(&caps(&[Tech::A, Tech::B])));
        assert!(!rule.satisfies(&caps(&[Tech::A])));
        assert!(!rule.satisfies(&caps(&[])));
    }

    #[test]
    fn all_of_empty_holds() {
        assert!(Requirement::<Tech>::all([]).satisfies(&caps(&[])));
    }

    #[test]
    fn any_requires_one_child() {
        let rule = Requirement::any([Requirement::has(Tech::A), Requirement::has(Tech::B)]);
        assert!(rule.satisfies(&caps(&[Tech::A])));
        assert!(rule.satisfies(&caps(&[Tech::A, Tech::B])));
        assert!(!rule.satisfies(&caps(&[Tech::C])));
    }

    #[test]
    fn any_of_empty_fails() {
        assert!(!Requirement::<Tech>::any([]).satisfies(&caps(&[])));
    }

    #[test]
    fn not_inverts() {
        let rule = Requirement::not(Requirement::has(Tech::A));
        assert!(!rule.satisfies(&caps(&[Tech::A])));
        assert!(rule.satisfies(&caps(&[])));
    }

    #[test]
    fn not_inverts_a_nested_expression() {
        let rule = Requirement::not(Requirement::all([
            Requirement::has(Tech::A),
            Requirement::has(Tech::B),
        ]));
        assert!(rule.satisfies(&caps(&[Tech::A])));
        assert!(!rule.satisfies(&caps(&[Tech::A, Tech::B])));
    }

    #[test]
    fn at_least_counts_children() {
        let rule = Requirement::at_least(
            2,
            [
                Requirement::has(Tech::A),
                Requirement::has(Tech::B),
                Requirement::has(Tech::C),
            ],
        );
        assert!(!rule.satisfies(&caps(&[Tech::A])));
        assert!(rule.satisfies(&caps(&[Tech::A, Tech::B])));
        assert!(rule.satisfies(&caps(&[Tech::A, Tech::B, Tech::C])));
    }

    #[test]
    fn at_least_zero_always_holds() {
        let rule = Requirement::at_least(0, [Requirement::has(Tech::A)]);
        assert!(rule.satisfies(&caps(&[])));
    }

    #[test]
    fn at_least_more_than_available_never_holds() {
        let rule = Requirement::at_least(5, [Requirement::has(Tech::A), Requirement::has(Tech::B)]);
        assert!(!rule.satisfies(&caps(&[Tech::A, Tech::B])));
    }

    #[test]
    fn nesting_mixes_all_and_any() {
        let rule = Requirement::all([
            Requirement::has(Tech::A),
            Requirement::any([Requirement::has(Tech::B), Requirement::has(Tech::C)]),
        ]);
        assert!(rule.satisfies(&caps(&[Tech::A, Tech::C])));
        assert!(!rule.satisfies(&caps(&[Tech::B, Tech::C])));
    }

    #[test]
    fn nesting_mixes_any_and_all() {
        let rule = Requirement::any([
            Requirement::all([Requirement::has(Tech::A), Requirement::has(Tech::B)]),
            Requirement::all([Requirement::has(Tech::C), Requirement::has(Tech::D)]),
        ]);
        assert!(rule.satisfies(&caps(&[Tech::C, Tech::D])));
        assert!(!rule.satisfies(&caps(&[Tech::A, Tech::D])));
    }

    #[test]
    fn string_keys_work() {
        let held: CapabilitySet<String> = ["warp".to_owned()].into_iter().collect();
        let rule = Requirement::all([Requirement::has("warp".to_owned())]);
        assert!(rule.satisfies(&held));
    }

    #[test]
    fn the_tree_agrees_with_the_boolean() {
        let rules = vec![
            Requirement::has(Tech::A),
            Requirement::all([Requirement::has(Tech::A), Requirement::has(Tech::B)]),
            Requirement::any([Requirement::has(Tech::C), Requirement::has(Tech::D)]),
            Requirement::not(Requirement::has(Tech::A)),
            Requirement::at_least(2, [Requirement::has(Tech::A), Requirement::has(Tech::B)]),
            Requirement::all([]),
            Requirement::any([]),
        ];
        for held in [
            caps(&[]),
            caps(&[Tech::A]),
            caps(&[Tech::A, Tech::B]),
            caps(&[Tech::C, Tech::D]),
        ] {
            for rule in &rules {
                assert_eq!(
                    rule.satisfies(&held),
                    rule.evaluate(&held).satisfied(),
                    "{rule:?} against {held:?}"
                );
            }
        }
    }

    #[test]
    fn flags_and_checks_read_false_without_a_context() {
        assert!(!Rule::flag("accepted").satisfies(&CapabilitySet::new()));
        assert!(Rule::flag_clear("accepted").satisfies(&CapabilitySet::new()));
        assert!(!Rule::named("anything").satisfies(&CapabilitySet::new()));
    }

    #[test]
    fn capabilities_lists_every_mentioned_key() {
        let rule = Requirement::all([
            Requirement::has(Tech::A),
            Requirement::not(Requirement::has(Tech::B)),
            Requirement::any([Requirement::has(Tech::A), Requirement::has(Tech::C)]),
            Requirement::flag("ignored"),
        ]);
        let keys = rule.capabilities();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&Tech::A) && keys.contains(&Tech::B) && keys.contains(&Tech::C));
    }

    #[test]
    fn unknown_checks_finds_unregistered_names() {
        let mut checks = Checks::new();
        checks.register("known", check("Known", |_query| true));

        let rule = Requirement::<Tech>::all([
            Requirement::named("known"),
            Requirement::not(Requirement::named("missing")),
        ]);
        assert_eq!(rule.unknown_checks(&checks), vec!["missing".to_owned()]);
    }

    #[test]
    fn condition_path_reads_capabilities_flags_and_checks() {
        let mut services = TypeMap::new();
        services.insert(caps(&[Tech::A]));
        let mut checks = Checks::new();
        checks.register("always", check("Always true", |_query| true));
        services.insert(checks);

        let mut chain = ChainFlags::new();
        chain.set_flag("accepted", true);
        let query = QueryCtx {
            target: None,
            chain: Some(&chain),
            caps: &services,
        };

        let rule = Requirement::all([
            Requirement::has(Tech::A),
            Requirement::flag("accepted"),
            Requirement::named("always"),
        ]);
        assert!(rule.satisfies_in(&query));

        let missing_check = Requirement::<Tech>::named("absent");
        assert!(!missing_check.satisfies_in(&query));
    }

    #[test]
    fn condition_path_and_pure_path_agree_on_capabilities() {
        let mut services = TypeMap::new();
        services.insert(caps(&[Tech::A, Tech::B]));
        let query = QueryCtx {
            target: None,
            chain: None,
            caps: &services,
        };
        let rule = Requirement::all([Requirement::has(Tech::A), Requirement::has(Tech::C)]);
        assert_eq!(
            rule.satisfies_in(&query),
            rule.satisfies(&caps(&[Tech::A, Tech::B]))
        );
    }

    #[test]
    fn explain_matches_the_boolean_answer() {
        let services = TypeMap::new();
        let query = QueryCtx {
            target: None,
            chain: None,
            caps: &services,
        };
        let rule = Requirement::any([Requirement::has(Tech::A), Requirement::flag("nope")]);
        let explanation = rule.explain(&query);
        assert_eq!(explanation.satisfied, rule.satisfies_in(&query));
        assert_eq!(explanation.children.len(), 2);
        assert_eq!(explanation.children[1].summary, "Flag 'nope' is true");
    }

    #[test]
    fn summaries_describe_each_node() {
        assert_eq!(Requirement::has(Tech::A).summary(), "Has A");
        assert_eq!(Requirement::<Tech>::all([]).summary(), "All of 0");
        assert_eq!(
            Requirement::not(Requirement::has(Tech::A)).summary(),
            "Not (Has A)"
        );
        assert_eq!(
            Requirement::at_least(1, [Requirement::has(Tech::A)]).summary(),
            "At least 1 of 1"
        );
        assert_eq!(Rule::named("x").summary(), "Check 'x'");
    }

    #[test]
    fn warnings_report_authoring_traps() {
        assert!(Requirement::<Tech>::any([]).warning().is_some());
        assert!(
            Requirement::at_least(3, [Requirement::has(Tech::A)])
                .warning()
                .unwrap()
                .contains("never hold")
        );
        assert!(
            Requirement::at_least(0, [Requirement::has(Tech::A)])
                .warning()
                .is_some()
        );
        assert!(Rule::flag("").warning().is_some());
        assert!(Rule::named("  ").warning().is_some());
        assert!(Requirement::has(Tech::A).warning().is_none());
    }

    #[test]
    fn warnings_name_the_offending_child() {
        let rule = Requirement::all([Requirement::has(Tech::A), Requirement::flag("")]);
        assert!(rule.warning().unwrap().starts_with("Requirement 1:"));

        let nested = Requirement::not(Requirement::<Tech>::any([]));
        assert!(nested.warning().unwrap().contains("Inner requirement"));
    }
}
