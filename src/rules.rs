//! Capabilities, the requirements over them, and the answers.

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{Debug, Display, Formatter, Result as FmtResult};

/// Answers whether an entity holds a capability.
///
/// Every rule reads capabilities through this trait. The crate implements it
/// for [`BTreeSet`] and [`CapabilitySet`]. A host type can implement it too,
/// and its answer can mix stored keys with keys it computes from other state,
/// such as a level or an item count. Each key then has one answer, whoever
/// asks.
///
/// ```
/// use std::collections::BTreeSet;
/// use plotline::{Has, Requirement};
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// enum Key {
///     Fusion,
///     RichTreasury,
/// }
///
/// struct Empire {
///     researched: BTreeSet<Key>,
///     gold: u32,
/// }
///
/// impl Has<Key> for Empire {
///     fn has(&self, key: &Key) -> bool {
///         match key {
///             Key::RichTreasury => self.gold >= 500,
///             stored => self.researched.contains(stored),
///         }
///     }
/// }
///
/// let empire = Empire { researched: BTreeSet::from([Key::Fusion]), gold: 800 };
/// let rule = Requirement::all([Requirement::has(Key::Fusion), Requirement::has(Key::RichTreasury)]);
/// assert!(rule.satisfies(&empire));
/// ```
pub trait Has<K> {
    /// Returns whether the entity holds this key.
    fn has(&self, key: &K) -> bool;
}

impl<K: Ord> Has<K> for BTreeSet<K> {
    fn has(&self, key: &K) -> bool {
        self.contains(key)
    }
}

impl<K: Ord> Has<K> for CapabilitySet<K> {
    fn has(&self, key: &K) -> bool {
        self.contains(key)
    }
}

/// The capabilities one entity holds.
///
/// The crate never interprets a key. The host chooses the type and its
/// meaning. Keys are ordered so that iteration and evaluation are
/// deterministic.
///
/// ```
/// use plotline::CapabilitySet;
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// enum Tech {
///     Fusion,
///     Warp,
/// }
///
/// let mut caps = CapabilitySet::from([Tech::Fusion]);
/// assert!(caps.contains(&Tech::Fusion));
/// assert!(!caps.contains(&Tech::Warp));
///
/// caps.insert(Tech::Warp);
/// assert_eq!(caps.len(), 2);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "K: serde::Serialize",
        deserialize = "K: Ord + serde::Deserialize<'de>"
    ))
)]
pub struct CapabilitySet<K> {
    keys: BTreeSet<K>,
}

impl<K> CapabilitySet<K> {
    /// Creates an empty set.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            keys: BTreeSet::new(),
        }
    }

    /// Returns the number of keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Returns whether the set holds no keys.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Iterates over the keys in order.
    pub fn iter(&self) -> alloc::collections::btree_set::Iter<'_, K> {
        self.keys.iter()
    }
}

impl<K: Ord> CapabilitySet<K> {
    /// Adds a key. Returns whether the set changed.
    pub fn insert(&mut self, key: K) -> bool {
        self.keys.insert(key)
    }

    /// Removes a key. Returns whether the set changed.
    pub fn remove(&mut self, key: &K) -> bool {
        self.keys.remove(key)
    }

    /// Returns whether the set holds this key.
    #[must_use]
    pub fn contains(&self, key: &K) -> bool {
        self.keys.contains(key)
    }
}

impl<K> Default for CapabilitySet<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord> Extend<K> for CapabilitySet<K> {
    fn extend<T: IntoIterator<Item = K>>(&mut self, iter: T) {
        self.keys.extend(iter);
    }
}

impl<K: Ord> FromIterator<K> for CapabilitySet<K> {
    fn from_iter<T: IntoIterator<Item = K>>(iter: T) -> Self {
        Self {
            keys: iter.into_iter().collect(),
        }
    }
}

impl<K: Ord, const N: usize> From<[K; N]> for CapabilitySet<K> {
    fn from(keys: [K; N]) -> Self {
        keys.into_iter().collect()
    }
}

impl<'a, K> IntoIterator for &'a CapabilitySet<K> {
    type Item = &'a K;
    type IntoIter = alloc::collections::btree_set::Iter<'a, K>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<K> IntoIterator for CapabilitySet<K> {
    type Item = K;
    type IntoIter = alloc::collections::btree_set::IntoIter<K>;

    fn into_iter(self) -> Self::IntoIter {
        self.keys.into_iter()
    }
}

/// A requirement over capability keys, held as data.
///
/// The crate never interprets a key. It only asks a [`Has`] holder whether it
/// holds one. A requirement is plain data, so the host can
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
/// error. [`Requirement::warning`] reports the traps at author time.
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
    /// The holder has this key.
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

    /// Returns an authoring warning, if any.
    ///
    /// A warning marks a rule that is legal but almost certainly a mistake,
    /// such as an empty `Any`, which never holds. Nested warnings name the
    /// position of the offending child.
    #[must_use]
    pub fn warning(&self) -> Option<String> {
        match self {
            Self::Has(_) => None,
            Self::Any(items) if items.is_empty() => Some("An empty Any never holds.".to_owned()),
            Self::All(items) | Self::Any(items) => first_child_warning(items),
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
        }
    }
}

fn first_child_warning<K>(items: &[Requirement<K>]) -> Option<String> {
    items.iter().enumerate().find_map(|(index, item)| {
        item.warning()
            .map(|warning| format!("Requirement {index}: {warning}"))
    })
}

impl<K: Ord + Clone> Requirement<K> {
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
    /// assert_eq!(rule.optional().iter().copied().collect::<Vec<_>>(), ["gate", "warp"]);
    /// ```
    #[must_use]
    pub fn required(&self) -> CapabilitySet<K> {
        self.edges().0
    }

    /// Returns the capabilities that help but are not required.
    ///
    /// These are the keys inside a [`Requirement::Any`] or a
    /// [`Requirement::AtLeast`]. The walk never enters a
    /// [`Requirement::Not`], because a forbidden key does not help.
    ///
    /// This is the soft edge set of a dependency graph.
    #[must_use]
    pub fn optional(&self) -> CapabilitySet<K> {
        self.edges().1
    }

    /// Returns the required and optional keys in one walk.
    fn edges(&self) -> (CapabilitySet<K>, CapabilitySet<K>) {
        let mut sets = (CapabilitySet::new(), CapabilitySet::new());
        self.collect_edges(&mut sets, false);
        sets
    }

    fn collect_edges(&self, sets: &mut (CapabilitySet<K>, CapabilitySet<K>), in_choice: bool) {
        match self {
            Self::Has(key) => {
                let side = if in_choice { &mut sets.1 } else { &mut sets.0 };
                side.insert(key.clone());
            }
            Self::All(items) => {
                for item in items {
                    item.collect_edges(sets, in_choice);
                }
            }
            Self::Any(items)
            | Self::AtLeast {
                requirements: items,
                ..
            } => {
                for item in items {
                    item.collect_edges(sets, true);
                }
            }
            // Not forbids rather than demands.
            Self::Not(_) => {}
        }
    }
}

impl<K> Requirement<K> {
    /// Returns whether the holder satisfies this requirement.
    ///
    /// This call allocates nothing and stops at the first decisive child.
    /// It always agrees with [`Evaluation::satisfied`] from
    /// [`Requirement::evaluate`].
    #[must_use]
    pub fn satisfies(&self, holder: &(impl Has<K> + ?Sized)) -> bool {
        match self {
            Self::Has(key) => holder.has(key),
            Self::All(items) => items.iter().all(|item| item.satisfies(holder)),
            Self::Any(items) => items.iter().any(|item| item.satisfies(holder)),
            Self::Not(inner) => !inner.satisfies(holder),
            Self::AtLeast {
                count,
                requirements,
            } => {
                requirements
                    .iter()
                    .filter(|item| item.satisfies(holder))
                    .count()
                    >= *count
            }
        }
    }

    /// Evaluates the requirement and returns the full result tree.
    #[must_use]
    pub fn evaluate(&self, holder: &(impl Has<K> + ?Sized)) -> Evaluation<K>
    where
        K: Clone,
    {
        match self {
            Self::Has(key) => Evaluation::Has {
                key: key.clone(),
                satisfied: holder.has(key),
            },
            Self::All(items) => {
                let children = Self::evaluate_each(items, holder);
                Evaluation::All {
                    satisfied: children.iter().all(Evaluation::satisfied),
                    children,
                }
            }
            Self::Any(items) => {
                let children = Self::evaluate_each(items, holder);
                Evaluation::Any {
                    satisfied: children.iter().any(Evaluation::satisfied),
                    children,
                }
            }
            Self::Not(inner) => {
                let child = inner.evaluate(holder);
                Evaluation::Not {
                    satisfied: !child.satisfied(),
                    child: Box::new(child),
                }
            }
            Self::AtLeast {
                count,
                requirements,
            } => {
                let children = Self::evaluate_each(requirements, holder);
                let met = children.iter().filter(|c| c.satisfied()).count();
                Evaluation::AtLeast {
                    count: *count,
                    met,
                    satisfied: met >= *count,
                    children,
                }
            }
        }
    }

    fn evaluate_each(items: &[Self], holder: &(impl Has<K> + ?Sized)) -> Vec<Evaluation<K>>
    where
        K: Clone,
    {
        items.iter().map(|item| item.evaluate(holder)).collect()
    }
}

/// The result of evaluating a [`Requirement`].
///
/// The tree mirrors the requirement that produced it, so a caller can explain
/// a result without evaluating anything again. Every node reports its own
/// answer in `satisfied`.
///
/// The [`Display`] implementation draws the tree, one node per line.
///
/// ```
/// use plotline::{CapabilitySet, Requirement};
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// enum Tech {
///     Fusion,
///     Warp,
/// }
///
/// let caps = CapabilitySet::from([Tech::Fusion]);
/// let rule = Requirement::all([Requirement::has(Tech::Fusion), Requirement::has(Tech::Warp)]);
///
/// let result = rule.evaluate(&caps);
/// assert!(!result.satisfied());
/// assert_eq!(result.missing(), vec![Tech::Warp]);
/// assert_eq!(result.to_string(), "\
/// ✗ All of 2
///   ✓ Has Fusion
///   ✗ Has Warp");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Evaluation<K> {
    /// One capability lookup.
    Has {
        /// The capability that was looked up.
        key: K,
        /// Whether the holder has it.
        satisfied: bool,
    },
    /// Every child must hold. An empty child list holds.
    All {
        /// Whether every child holds.
        satisfied: bool,
        /// One result per child requirement.
        children: Vec<Evaluation<K>>,
    },
    /// Any one child must hold. An empty child list fails.
    Any {
        /// Whether at least one child holds.
        satisfied: bool,
        /// One result per child requirement.
        children: Vec<Evaluation<K>>,
    },
    /// The child must not hold.
    Not {
        /// Whether the child failed.
        satisfied: bool,
        /// The inverted result.
        child: Box<Evaluation<K>>,
    },
    /// At least `count` children must hold.
    AtLeast {
        /// The number of children that must hold.
        count: usize,
        /// The number of children that did hold.
        met: usize,
        /// Whether `met` reached `count`.
        satisfied: bool,
        /// One result per child requirement.
        children: Vec<Evaluation<K>>,
    },
}

impl<K> Evaluation<K> {
    /// Returns whether this node holds.
    #[must_use]
    pub fn satisfied(&self) -> bool {
        match self {
            Self::Has { satisfied, .. }
            | Self::All { satisfied, .. }
            | Self::Any { satisfied, .. }
            | Self::Not { satisfied, .. }
            | Self::AtLeast { satisfied, .. } => *satisfied,
        }
    }

    /// Returns the child results, which is empty for a leaf.
    #[must_use]
    pub fn children(&self) -> &[Evaluation<K>] {
        match self {
            Self::All { children, .. }
            | Self::Any { children, .. }
            | Self::AtLeast { children, .. } => children,
            Self::Not { child, .. } => core::slice::from_ref(child),
            Self::Has { .. } => &[],
        }
    }
}

impl<K: Clone> Evaluation<K> {
    /// Returns the capabilities that must be added to satisfy the requirement.
    ///
    /// The answer is deliberately conservative. It descends through
    /// [`Evaluation::All`] nodes only, and collects the keys of failed
    /// [`Evaluation::Has`] leaves. Adding every returned key always helps.
    ///
    /// A key under [`Evaluation::Any`], [`Evaluation::Not`], or
    /// [`Evaluation::AtLeast`] is never returned, because those nodes offer a
    /// choice rather than a fixed shortfall. Walk the tree directly to present
    /// those alternatives.
    ///
    /// ```
    /// use plotline::{CapabilitySet, Requirement};
    ///
    /// let held = CapabilitySet::<&str>::new();
    ///
    /// // One of two routes is enough, so neither is "missing".
    /// let choice = Requirement::any([Requirement::has("warp"), Requirement::has("gate")]);
    /// assert!(choice.evaluate(&held).missing().is_empty());
    ///
    /// // Both are needed, so both are missing.
    /// let both = Requirement::all([Requirement::has("warp"), Requirement::has("gate")]);
    /// assert_eq!(both.evaluate(&held).missing(), vec!["warp", "gate"]);
    /// ```
    #[must_use]
    pub fn missing(&self) -> Vec<K> {
        let mut keys = Vec::new();
        self.collect_missing(&mut keys);
        keys
    }

    fn collect_missing(&self, keys: &mut Vec<K>) {
        match self {
            Self::Has {
                key,
                satisfied: false,
            } => keys.push(key.clone()),
            Self::All { children, .. } => {
                for child in children {
                    child.collect_missing(keys);
                }
            }
            // Any, Not, and AtLeast offer a choice. A flat answer would mislead.
            _ => {}
        }
    }
}

impl<K: Debug> Display for Evaluation<K> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        self.write_at(f, 0)
    }
}

impl<K: Debug> Evaluation<K> {
    fn write_at(&self, f: &mut Formatter<'_>, depth: usize) -> FmtResult {
        let mark = if self.satisfied() { '✓' } else { '✗' };
        write!(f, "{:pad$}{mark} ", "", pad = depth * 2)?;
        match self {
            Self::Has { key, .. } => write!(f, "Has {key:?}")?,
            Self::All { children, .. } => write!(f, "All of {}", children.len())?,
            Self::Any { children, .. } => write!(f, "Any of {}", children.len())?,
            Self::Not { .. } => write!(f, "Not")?,
            Self::AtLeast {
                count,
                met,
                children,
                ..
            } => write!(f, "At least {count} of {}, {met} met", children.len())?,
        }
        for child in self.children() {
            writeln!(f)?;
            child.write_at(f, depth + 1)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;
    use alloc::vec;

    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Tech {
        A,
        B,
        C,
        D,
    }
    use Tech::{A, B, C, D};

    fn caps(keys: &[Tech]) -> CapabilitySet<Tech> {
        keys.iter().copied().collect()
    }

    #[test]
    fn a_set_tracks_membership_and_order() {
        // Tech has no Default, which proves the impl carries no K bound.
        let mut set = CapabilitySet::<Tech>::default();
        assert!(set.is_empty());

        assert!(set.insert(C));
        assert!(!set.insert(C), "a repeat insert changes nothing");
        set.extend([A, C]);
        assert_eq!(set.iter().copied().collect::<Vec<_>>(), vec![A, C]);

        assert!(set.remove(&C));
        assert!(!set.remove(&C));
        assert_eq!(set, CapabilitySet::from([A]));
    }

    #[test]
    fn a_set_holds_any_ordered_key() {
        let mut set = CapabilitySet::new();
        set.insert(String::from("survive-frozen"));
        assert!(set.contains(&String::from("survive-frozen")));
    }

    /// The boolean answer and the evaluation tree must always agree.
    #[test]
    fn boolean_semantics() {
        let cases: Vec<(&[Tech], Requirement<Tech>, bool)> = vec![
            (&[A], Requirement::has(A), true),
            (&[], Requirement::has(A), false),
            (
                &[A, B],
                Requirement::all([Requirement::has(A), Requirement::has(B)]),
                true,
            ),
            (
                &[A],
                Requirement::all([Requirement::has(A), Requirement::has(B)]),
                false,
            ),
            (&[], Requirement::all([]), true),
            (
                &[A],
                Requirement::any([Requirement::has(A), Requirement::has(B)]),
                true,
            ),
            (
                &[C],
                Requirement::any([Requirement::has(A), Requirement::has(B)]),
                false,
            ),
            (&[], Requirement::any([]), false),
            (&[A], Requirement::not(Requirement::has(A)), false),
            (&[], Requirement::not(Requirement::has(A)), true),
            (
                &[A],
                Requirement::not(Requirement::all([Requirement::has(A), Requirement::has(B)])),
                true,
            ),
            (
                &[A],
                Requirement::at_least(
                    2,
                    [
                        Requirement::has(A),
                        Requirement::has(B),
                        Requirement::has(C),
                    ],
                ),
                false,
            ),
            (
                &[A, B],
                Requirement::at_least(
                    2,
                    [
                        Requirement::has(A),
                        Requirement::has(B),
                        Requirement::has(C),
                    ],
                ),
                true,
            ),
            (&[], Requirement::at_least(0, [Requirement::has(A)]), true),
            (
                &[A, B],
                Requirement::at_least(5, [Requirement::has(A), Requirement::has(B)]),
                false,
            ),
            (
                &[A, C],
                Requirement::all([
                    Requirement::has(A),
                    Requirement::any([Requirement::has(B), Requirement::has(C)]),
                ]),
                true,
            ),
            (
                &[B, C],
                Requirement::all([
                    Requirement::has(A),
                    Requirement::any([Requirement::has(B), Requirement::has(C)]),
                ]),
                false,
            ),
            (
                &[C, D],
                Requirement::any([
                    Requirement::all([Requirement::has(A), Requirement::has(B)]),
                    Requirement::all([Requirement::has(C), Requirement::has(D)]),
                ]),
                true,
            ),
        ];

        for (held, rule, expect) in cases {
            let held = caps(held);
            assert_eq!(rule.satisfies(&held), expect, "{rule:?} against {held:?}");
            assert_eq!(
                rule.evaluate(&held).satisfied(),
                expect,
                "tree disagrees with bool: {rule:?}"
            );
        }
    }

    /// A host type that stores some keys and computes others.
    struct World {
        stored: BTreeSet<Tech>,
        level: u32,
    }

    impl Has<Tech> for World {
        fn has(&self, key: &Tech) -> bool {
            match key {
                D => self.level >= 5,
                stored => self.stored.contains(stored),
            }
        }
    }

    #[test]
    fn a_computed_key_gets_one_answer_everywhere() {
        let rule = Requirement::all([Requirement::has(A), Requirement::has(D)]);
        let novice = World {
            stored: BTreeSet::from([A]),
            level: 1,
        };
        let veteran = World {
            stored: BTreeSet::from([A]),
            level: 5,
        };

        assert!(!rule.satisfies(&novice));
        assert_eq!(rule.evaluate(&novice).missing(), vec![D]);
        assert!(rule.satisfies(&veteran));
        assert!(rule.evaluate(&veteran).satisfied());

        let erased: &dyn Has<Tech> = &veteran;
        assert!(rule.satisfies(erased));
    }

    #[test]
    fn string_keys_need_no_rust_vocabulary() {
        let held: CapabilitySet<String> = ["warp".to_owned()].into_iter().collect();
        assert!(Requirement::all([Requirement::has("warp".to_owned())]).satisfies(&held));
    }

    #[test]
    fn missing_reports_an_unavoidable_shortfall_only() {
        let held = caps(&[A]);
        let cases: Vec<(Requirement<Tech>, Vec<Tech>)> = vec![
            (Requirement::has(B), vec![B]),
            (
                Requirement::all([
                    Requirement::has(A),
                    Requirement::has(B),
                    Requirement::has(C),
                ]),
                vec![B, C],
            ),
            (
                Requirement::all([Requirement::has(A), Requirement::all([Requirement::has(B)])]),
                vec![B],
            ),
            (Requirement::all([Requirement::has(A)]), vec![]),
            // A choice, a negation, and a count all offer alternatives.
            (
                Requirement::any([Requirement::has(B), Requirement::has(C)]),
                vec![],
            ),
            (Requirement::not(Requirement::has(A)), vec![]),
            (
                Requirement::at_least(2, [Requirement::has(B), Requirement::has(C)]),
                vec![],
            ),
        ];

        for (rule, expect) in cases {
            assert_eq!(rule.evaluate(&held).missing(), expect, "{rule:?}");
        }
    }

    #[test]
    fn required_and_optional_split_the_dependency_edges() {
        let rule = Requirement::all([
            Requirement::has(A),
            Requirement::all([Requirement::has(B)]),
            Requirement::any([Requirement::has(C), Requirement::has(D)]),
            Requirement::not(Requirement::has(D)),
        ]);

        assert_eq!(
            rule.required().iter().copied().collect::<Vec<_>>(),
            vec![A, B]
        );
        assert_eq!(
            rule.optional().iter().copied().collect::<Vec<_>>(),
            vec![C, D]
        );
    }

    #[test]
    fn required_matches_missing_against_an_empty_set() {
        let rule = Requirement::all([
            Requirement::has(A),
            Requirement::any([Requirement::has(B), Requirement::has(C)]),
        ]);
        assert_eq!(
            rule.required().iter().copied().collect::<Vec<_>>(),
            rule.evaluate(&CapabilitySet::new()).missing()
        );
    }

    #[test]
    fn a_choice_demands_no_one_key() {
        let rule = Requirement::any([
            Requirement::all([Requirement::has(A), Requirement::has(B)]),
            Requirement::has(C),
        ]);
        assert!(rule.required().is_empty());
        assert_eq!(rule.optional().len(), 3);
    }

    #[test]
    fn a_result_draws_itself_as_a_tree() {
        let rule = Requirement::all([
            Requirement::has(A),
            Requirement::not(Requirement::has(B)),
            Requirement::at_least(1, [Requirement::has(C)]),
        ]);
        let drawn = rule.evaluate(&caps(&[A])).to_string();
        assert_eq!(
            drawn,
            "\
✗ All of 3
  ✓ Has A
  ✓ Not
    ✗ Has B
  ✗ At least 1 of 1, 0 met
    ✗ Has C"
        );
    }

    #[test]
    fn warnings_report_authoring_traps() {
        assert!(Requirement::<Tech>::any([]).warning().is_some());
        assert!(
            Requirement::at_least(3, [Requirement::has(A)])
                .warning()
                .unwrap()
                .contains("never hold")
        );
        assert!(
            Requirement::at_least(0, [Requirement::has(A)])
                .warning()
                .is_some()
        );
        assert!(Requirement::has(A).warning().is_none());
    }

    #[test]
    fn warnings_name_the_offending_child() {
        let rule = Requirement::all([Requirement::has(A), Requirement::any([])]);
        assert!(rule.warning().unwrap().starts_with("Requirement 1:"));

        let nested = Requirement::not(Requirement::<Tech>::any([]));
        assert!(nested.warning().unwrap().contains("Inner requirement"));
    }
}
