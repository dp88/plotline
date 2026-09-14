//! Structured requirement results.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::vocab::Explanation;

/// The result of evaluating a [`Requirement`](crate::Requirement).
///
/// The tree mirrors the requirement that produced it, so a caller can explain
/// a result without evaluating anything again. Every node reports its own
/// answer in `satisfied`.
///
/// ```
/// use plotline::{CapabilitySet, Evaluation, Requirement};
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
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Evaluation<K> {
    /// One capability lookup.
    Has {
        /// The capability that was looked up.
        key: K,
        /// Whether the set holds it.
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
    /// A leaf with no capability key, such as a flag or a named check.
    Opaque {
        /// A display summary of the leaf.
        summary: String,
        /// Whether the leaf held.
        satisfied: bool,
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
            | Self::AtLeast { satisfied, .. }
            | Self::Opaque { satisfied, .. } => *satisfied,
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

impl<K: core::fmt::Debug> From<&Evaluation<K>> for Explanation {
    /// Formats the keys so a host can explain the result without naming `K`.
    fn from(evaluation: &Evaluation<K>) -> Self {
        match evaluation {
            Evaluation::Has { key, satisfied } => {
                Explanation::leaf(alloc::format!("Has {key:?}"), *satisfied)
            }
            Evaluation::All {
                satisfied,
                children,
            } => Explanation {
                summary: alloc::format!("All of {}", children.len()),
                satisfied: *satisfied,
                children: children.iter().map(Explanation::from).collect(),
            },
            Evaluation::Any {
                satisfied,
                children,
            } => Explanation {
                summary: alloc::format!("Any of {}", children.len()),
                satisfied: *satisfied,
                children: children.iter().map(Explanation::from).collect(),
            },
            Evaluation::Not { satisfied, child } => {
                let child = Explanation::from(child.as_ref());
                Explanation {
                    summary: alloc::format!("Not ({})", child.summary),
                    satisfied: *satisfied,
                    children: alloc::vec![child],
                }
            }
            Evaluation::AtLeast {
                count,
                satisfied,
                children,
                ..
            } => Explanation {
                summary: alloc::format!("At least {count} of {}", children.len()),
                satisfied: *satisfied,
                children: children.iter().map(Explanation::from).collect(),
            },
            Evaluation::Opaque { summary, satisfied } => {
                Explanation::leaf(summary.clone(), *satisfied)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::capability::CapabilitySet;
    use crate::requirement::Requirement;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Tech {
        Fusion,
        Warp,
        Cloaking,
    }

    fn held() -> CapabilitySet<Tech> {
        CapabilitySet::from([Tech::Fusion])
    }

    #[test]
    fn missing_reports_failed_all_children() {
        let rule = Requirement::all([
            Requirement::has(Tech::Fusion),
            Requirement::has(Tech::Warp),
            Requirement::has(Tech::Cloaking),
        ]);
        assert_eq!(
            rule.evaluate(&held()).missing(),
            vec![Tech::Warp, Tech::Cloaking]
        );
    }

    #[test]
    fn missing_descends_nested_all_nodes() {
        let rule = Requirement::all([
            Requirement::has(Tech::Fusion),
            Requirement::all([Requirement::has(Tech::Warp)]),
        ]);
        assert_eq!(rule.evaluate(&held()).missing(), vec![Tech::Warp]);
    }

    #[test]
    fn missing_skips_a_choice() {
        let rule = Requirement::any([
            Requirement::has(Tech::Warp),
            Requirement::has(Tech::Cloaking),
        ]);
        assert!(rule.evaluate(&held()).missing().is_empty());
    }

    #[test]
    fn missing_skips_negation_and_at_least() {
        let negated = Requirement::not(Requirement::has(Tech::Fusion));
        assert!(!negated.evaluate(&held()).satisfied());
        assert!(negated.evaluate(&held()).missing().is_empty());

        let counted = Requirement::at_least(
            2,
            [
                Requirement::has(Tech::Warp),
                Requirement::has(Tech::Cloaking),
            ],
        );
        assert!(counted.evaluate(&held()).missing().is_empty());
    }

    #[test]
    fn missing_is_empty_when_satisfied() {
        let rule = Requirement::all([Requirement::has(Tech::Fusion)]);
        let result = rule.evaluate(&held());
        assert!(result.satisfied());
        assert!(result.missing().is_empty());
    }

    #[test]
    fn an_explanation_mirrors_the_evaluation() {
        let rule = Requirement::all([
            Requirement::has(Tech::Fusion),
            Requirement::not(Requirement::has(Tech::Warp)),
        ]);
        let explanation = Explanation::from(&rule.evaluate(&held()));

        assert!(explanation.satisfied);
        assert_eq!(explanation.summary, "All of 2");
        assert_eq!(explanation.children[0].summary, "Has Fusion");
        assert_eq!(explanation.children[1].summary, "Not (Has Warp)");
        assert_eq!(explanation.children[1].children.len(), 1);
    }

    #[test]
    fn an_explanation_keeps_an_opaque_summary() {
        let rule = crate::Rule::flag("accepted");
        let explanation = Explanation::from(&rule.evaluate(&CapabilitySet::new()));
        assert_eq!(explanation.summary, "Flag 'accepted' is true");
        assert!(!explanation.satisfied);
    }

    #[test]
    fn a_bare_failed_leaf_is_missing() {
        assert_eq!(
            Requirement::has(Tech::Warp).evaluate(&held()).missing(),
            vec![Tech::Warp]
        );
    }
}
