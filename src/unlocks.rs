//! Unlock graphs built from requirements and grants.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Debug;

use crate::rules::{Evaluation, Has, Requirement};

/// One node: what it demands, and what it gives.
///
/// ```
/// use std::collections::BTreeSet;
/// use plotline::{Requirement, Unlock};
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// enum Cap {
///     Fusion,
///     Warp,
/// }
///
/// let fusion = Unlock::free().granting([Cap::Fusion]);
/// let warp = Unlock::new(Requirement::has(Cap::Fusion)).granting([Cap::Warp]);
///
/// assert!(fusion.requires.satisfies(&BTreeSet::new()));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "K: serde::Serialize",
        deserialize = "K: Ord + serde::Deserialize<'de>"
    ))
)]
pub struct Unlock<K> {
    /// What the entity must hold before it can take this node.
    pub requires: Requirement<K>,
    /// What taking this node adds to the entity.
    pub grants: BTreeSet<K>,
}

impl<K: Ord> Unlock<K> {
    /// Creates a node behind a requirement. It grants nothing yet.
    #[must_use]
    pub fn new(requires: Requirement<K>) -> Self {
        Self {
            requires,
            grants: BTreeSet::new(),
        }
    }

    /// Creates a node that requires nothing. It grants nothing yet.
    #[must_use]
    pub fn free() -> Self {
        Self::new(Requirement::all([]))
    }

    /// Adds capabilities to the grant list and returns the node.
    #[must_use]
    pub fn granting(mut self, keys: impl IntoIterator<Item = K>) -> Self {
        self.grants.extend(keys);
        self
    }
}

/// One problem found by [`Unlocks::validate`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnlockWarning<I, K> {
    /// The node requires a capability that nothing else grants.
    ///
    /// A node that requires only what it grants itself reports here too. It
    /// is unreachable, but it forms no cycle, because a node never depends on
    /// itself.
    Ungrantable {
        /// The node that cannot be reached.
        id: I,
        /// The capability with no other source.
        capability: K,
    },
    /// The node sits on a dependency cycle, so nothing can ever take it.
    Cycle {
        /// The node on the cycle.
        id: I,
    },
    /// The node grants nothing, so no other node can depend on it.
    GrantsNothing {
        /// The node with an empty grant list.
        id: I,
    },
    /// The node requirement reports an authoring warning.
    Requirement {
        /// The node that holds the requirement.
        id: I,
        /// The warning text.
        message: String,
    },
}

/// Where one node stands for one entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeStatus {
    /// The host's record says the entity took the node.
    Unlocked,
    /// The node's requirement holds, and the entity has not taken it.
    Available,
    /// The node's requirement does not hold.
    Locked,
}

/// Everything a tree view draws for one node. [`Unlocks::view`] returns one
/// per node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeView<'a, I> {
    /// The node.
    pub id: &'a I,
    /// Where the node stands.
    pub status: NodeStatus,
    /// The layout column, or `None` for a node on a dependency cycle. See
    /// [`Unlocks::ranks`].
    pub rank: Option<usize>,
    /// The nodes that must come first. Draw these as solid edges. See
    /// [`Unlocks::dependencies`].
    pub dependencies: Vec<&'a I>,
    /// The nodes that could help instead. Draw these as dashed edges. See
    /// [`Unlocks::optional_dependencies`].
    pub optional_dependencies: Vec<&'a I>,
}

/// A graph of things an entity can unlock.
///
/// The graph edges are never authored. One node grants a capability, another
/// requires it, and that is the arrow. Change a requirement and the graph
/// changes with it.
///
/// ```
/// use std::collections::BTreeSet;
/// use plotline::{Requirement, Unlock, Unlocks};
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// enum Cap {
///     Fusion,
///     Warp,
/// }
///
/// let mut tree = Unlocks::new();
/// tree.insert("fusion-power", Unlock::free().granting([Cap::Fusion]));
/// tree.insert(
///     "warp-drive",
///     Unlock::new(Requirement::has(Cap::Fusion)).granting([Cap::Warp]),
/// );
///
/// // The arrow from fusion-power to warp-drive is derived, not declared.
/// assert_eq!(tree.dependencies(&"warp-drive"), vec![&"fusion-power"]);
///
/// let mut held = BTreeSet::new();
/// assert_eq!(tree.available(&held).collect::<Vec<_>>(), vec![&"fusion-power"]);
///
/// tree.take(&"fusion-power", &mut held);
///
/// // fusion-power stays available, because its requirement still holds. The
/// // host tracks which nodes it already took.
/// assert_eq!(
///     tree.available(&held).collect::<Vec<_>>(),
///     vec![&"fusion-power", &"warp-drive"],
/// );
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "I: serde::Serialize, K: serde::Serialize",
        deserialize = "I: Ord + serde::Deserialize<'de>, K: Ord + serde::Deserialize<'de>"
    ))
)]
pub struct Unlocks<I, K> {
    nodes: BTreeMap<I, Unlock<K>>,
}

impl<I, K> Unlocks<I, K> {
    /// Creates an empty graph.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
        }
    }

    /// Returns the number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns whether the graph holds no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Iterates over the node identifiers in order.
    pub fn ids(&self) -> alloc::collections::btree_map::Keys<'_, I, Unlock<K>> {
        self.nodes.keys()
    }
}

impl<I, K> Default for Unlocks<I, K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: Ord, K> Unlocks<I, K> {
    /// Adds a node and returns the node it replaced.
    pub fn insert(&mut self, id: I, unlock: Unlock<K>) -> Option<Unlock<K>> {
        self.nodes.insert(id, unlock)
    }

    /// Returns a node.
    #[must_use]
    pub fn get(&self, id: &I) -> Option<&Unlock<K>> {
        self.nodes.get(id)
    }
}

impl<I: Ord, K: Ord> Unlocks<I, K> {
    /// Returns whether a node can be taken. A missing node never can.
    ///
    /// The registry does not track which nodes a player already took, because
    /// two nodes may grant the same capability. Keep that record in the host
    /// and overlay it on this answer.
    #[must_use]
    pub fn is_available(&self, id: &I, holder: &(impl Has<K> + ?Sized)) -> bool {
        self.get(id)
            .is_some_and(|node| node.requires.satisfies(holder))
    }

    /// Returns where a node stands, or `None` when it is missing.
    ///
    /// `taken` is the host's record of the nodes the entity took. A taken node
    /// reads [`NodeStatus::Unlocked`] even when its requirement no longer
    /// holds, because the record, not the rule, says what the entity took.
    #[must_use]
    pub fn status(
        &self,
        id: &I,
        holder: &(impl Has<K> + ?Sized),
        taken: &BTreeSet<I>,
    ) -> Option<NodeStatus> {
        self.get(id).map(|node| status_of(id, node, holder, taken))
    }

    /// Iterates over the nodes that can be taken now.
    pub fn available<'a, H: Has<K> + ?Sized>(
        &'a self,
        holder: &'a H,
    ) -> impl Iterator<Item = &'a I> + 'a {
        self.nodes
            .iter()
            .filter(move |(_, node)| node.requires.satisfies(holder))
            .map(|(id, _)| id)
    }

    /// Returns every node that grants this capability.
    #[must_use]
    pub fn providers(&self, capability: &K) -> Vec<&I> {
        self.nodes
            .iter()
            .filter(|(_, node)| node.grants.contains(capability))
            .map(|(id, _)| id)
            .collect()
    }

    /// Adds a node's grants to the holder when the node is available.
    ///
    /// Returns whether it applied. Taking a node twice is harmless.
    ///
    /// The holder both answers the requirement and receives the grants. A
    /// set does both. A host type that computes some keys implements
    /// [`Extend`] to store the grants where its [`Has`] reads them.
    pub fn take<H>(&self, id: &I, holder: &mut H) -> bool
    where
        H: Has<K> + Extend<K>,
        K: Clone,
    {
        let Some(node) = self.get(id) else {
            return false;
        };
        if !node.requires.satisfies(holder) {
            return false;
        }
        holder.extend(node.grants.iter().cloned());
        true
    }
}

impl<I: Ord, K: Ord + Clone> Unlocks<I, K> {
    /// Explains why a node is available or locked, or `None` when it is missing.
    #[must_use]
    pub fn evaluate(&self, id: &I, holder: &(impl Has<K> + ?Sized)) -> Option<Evaluation<K>> {
        self.get(id).map(|node| node.requires.evaluate(holder))
    }

    /// Returns the nodes that must be taken before this one.
    ///
    /// These are the providers of every capability the requirement demands
    /// outright. A node never depends on itself.
    #[must_use]
    pub fn dependencies(&self, id: &I) -> Vec<&I> {
        self.edges(id, Requirement::required)
    }

    /// Returns the nodes that could help unlock this one without being required.
    ///
    /// These are the providers of the capabilities inside a choice. Draw them
    /// differently from [`Unlocks::dependencies`].
    #[must_use]
    pub fn optional_dependencies(&self, id: &I) -> Vec<&I> {
        self.edges(id, Requirement::optional)
    }

    fn edges(&self, id: &I, keys_of: fn(&Requirement<K>) -> BTreeSet<K>) -> Vec<&I> {
        let Some(node) = self.get(id) else {
            return Vec::new();
        };
        let mut found = BTreeSet::new();
        for key in &keys_of(&node.requires) {
            for provider in self.providers(key) {
                if provider != id {
                    found.insert(provider);
                }
            }
        }
        found.into_iter().collect()
    }

    /// Returns the rank of every node that has one.
    ///
    /// A rank counts how deep a node sits along its required dependencies. A
    /// node with no required dependency ranks 0. Use it as the column index in
    /// a tree layout. A node on a dependency cycle has no rank, so it is left
    /// out. [`Unlocks::validate`] reports those.
    #[must_use]
    pub fn ranks(&self) -> BTreeMap<I, usize>
    where
        I: Clone,
    {
        let mut done = BTreeMap::new();
        for id in self.nodes.keys() {
            let mut visiting = BTreeSet::new();
            self.rank_of(id, &mut done, &mut visiting);
        }
        done.into_iter()
            .filter_map(|(id, rank)| rank.map(|rank| (id, rank)))
            .collect()
    }

    fn rank_of(
        &self,
        id: &I,
        done: &mut BTreeMap<I, Option<usize>>,
        visiting: &mut BTreeSet<I>,
    ) -> Option<usize>
    where
        I: Clone,
    {
        if let Some(known) = done.get(id) {
            return *known;
        }
        if !visiting.insert(id.clone()) {
            return None; // a cycle; the caller records it as unrankable
        }

        let mut rank = Some(0);
        for parent in self.dependencies(id) {
            match self.rank_of(parent, done, visiting) {
                Some(parent_rank) => {
                    if let Some(current) = rank {
                        rank = Some(current.max(parent_rank + 1));
                    }
                }
                None => rank = None,
            }
        }

        visiting.remove(id);
        done.insert(id.clone(), rank);
        rank
    }

    /// Returns everything a tree view draws, one entry per node, in id order.
    ///
    /// Each entry holds the node's status, its rank, and both kinds of edge.
    /// `taken` is the host's record of the nodes the entity took. Sort the
    /// entries by rank to lay them out in columns.
    ///
    /// ```
    /// use std::collections::BTreeSet;
    /// use plotline::{NodeStatus, Requirement, Unlock, Unlocks};
    ///
    /// let mut tree = Unlocks::new();
    /// tree.insert("fusion-power", Unlock::free().granting(["fusion"]));
    /// tree.insert(
    ///     "warp-drive",
    ///     Unlock::new(Requirement::has("fusion")).granting(["warp"]),
    /// );
    ///
    /// let mut held = BTreeSet::new();
    /// let mut taken = BTreeSet::new();
    /// tree.take(&"fusion-power", &mut held);
    /// taken.insert("fusion-power");
    ///
    /// let view = tree.view(&held, &taken);
    /// assert_eq!(view[0].status, NodeStatus::Unlocked);
    /// assert_eq!(view[1].id, &"warp-drive");
    /// assert_eq!(view[1].status, NodeStatus::Available);
    /// assert_eq!(view[1].rank, Some(1));
    /// assert_eq!(view[1].dependencies, vec![&"fusion-power"]);
    /// ```
    #[must_use]
    pub fn view(&self, holder: &(impl Has<K> + ?Sized), taken: &BTreeSet<I>) -> Vec<NodeView<'_, I>>
    where
        I: Clone,
    {
        let ranks = self.ranks();
        self.nodes
            .iter()
            .map(|(id, node)| NodeView {
                id,
                status: status_of(id, node, holder, taken),
                rank: ranks.get(id).copied(),
                dependencies: self.dependencies(id),
                optional_dependencies: self.optional_dependencies(id),
            })
            .collect()
    }
}

fn status_of<I: Ord, K>(
    id: &I,
    node: &Unlock<K>,
    holder: &(impl Has<K> + ?Sized),
    taken: &BTreeSet<I>,
) -> NodeStatus {
    if taken.contains(id) {
        NodeStatus::Unlocked
    } else if node.requires.satisfies(holder) {
        NodeStatus::Available
    } else {
        NodeStatus::Locked
    }
}

impl<I: Ord + Clone, K: Ord + Clone> Unlocks<I, K> {
    /// Reports authoring problems.
    ///
    /// `external` holds the capabilities that other systems grant, such as a
    /// species trait or a starting bonus. Without it, every requirement that
    /// no node satisfies looks like dead content.
    #[must_use]
    pub fn validate(&self, external: &(impl Has<K> + ?Sized)) -> Vec<UnlockWarning<I, K>> {
        let mut warnings = Vec::new();
        let ranks = self.ranks();

        for (id, node) in &self.nodes {
            for key in &node.requires.required() {
                // A node never depends on itself, so a key it alone grants
                // leaves it unreachable without forming a cycle.
                let elsewhere = self
                    .providers(key)
                    .into_iter()
                    .any(|provider| provider != id);
                if !external.has(key) && !elsewhere {
                    warnings.push(UnlockWarning::Ungrantable {
                        id: id.clone(),
                        capability: key.clone(),
                    });
                }
            }

            if !ranks.contains_key(id) {
                warnings.push(UnlockWarning::Cycle { id: id.clone() });
            }

            if node.grants.is_empty() {
                warnings.push(UnlockWarning::GrantsNothing { id: id.clone() });
            }

            if let Some(message) = node.requires.warning() {
                warnings.push(UnlockWarning::Requirement {
                    id: id.clone(),
                    message,
                });
            }
        }

        warnings
    }
}

impl<I, K> core::fmt::Display for UnlockWarning<I, K>
where
    I: Debug,
    K: Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Ungrantable { id, capability } => {
                write!(
                    f,
                    "{id:?} requires {capability:?}, which nothing else grants"
                )
            }
            Self::Cycle { id } => write!(f, "{id:?} sits on a dependency cycle"),
            Self::GrantsNothing { id } => write!(f, "{id:?} grants nothing"),
            Self::Requirement { id, message } => write!(f, "{id:?} requirement: {message}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Cap {
        Fusion,
        Warp,
        Antimatter,
        Cloaking,
        Shields,
    }

    /// fusion ─┬─▶ warp ──▶ cloaking
    ///         └─▶ antimatter
    /// shields needs warp or antimatter, so both are optional and neither required.
    fn tree() -> Unlocks<&'static str, Cap> {
        let mut tree = Unlocks::new();
        tree.insert("fusion", Unlock::free().granting([Cap::Fusion]));
        tree.insert(
            "warp",
            Unlock::new(Requirement::has(Cap::Fusion)).granting([Cap::Warp]),
        );
        tree.insert(
            "antimatter",
            Unlock::new(Requirement::has(Cap::Fusion)).granting([Cap::Antimatter]),
        );
        tree.insert(
            "cloaking",
            Unlock::new(Requirement::has(Cap::Warp)).granting([Cap::Cloaking]),
        );
        tree.insert(
            "shields",
            Unlock::new(Requirement::any([
                Requirement::has(Cap::Warp),
                Requirement::has(Cap::Antimatter),
            ]))
            .granting([Cap::Shields]),
        );
        tree
    }

    #[test]
    fn a_free_node_is_available_from_nothing() {
        let tree = tree();
        let held = BTreeSet::new();
        assert!(tree.is_available(&"fusion", &held));
        assert!(!tree.is_available(&"warp", &held));
        assert!(!tree.is_available(&"absent", &held));
    }

    #[test]
    fn available_and_locked_split_the_graph() {
        let tree = tree();
        let held = BTreeSet::from([Cap::Fusion]);
        let available: Vec<_> = tree.available(&held).copied().collect();
        assert_eq!(available, vec!["antimatter", "fusion", "warp"]);
    }

    #[test]
    fn edges_are_derived_from_grants_and_requirements() {
        let tree = tree();
        assert_eq!(tree.dependencies(&"warp"), vec![&"fusion"]);
        assert_eq!(tree.dependencies(&"cloaking"), vec![&"warp"]);
        assert!(tree.dependencies(&"fusion").is_empty());
    }

    #[test]
    fn a_choice_makes_optional_edges_not_required_ones() {
        let tree = tree();
        assert!(
            tree.dependencies(&"shields").is_empty(),
            "neither route is required"
        );
        assert_eq!(
            tree.optional_dependencies(&"shields"),
            vec![&"antimatter", &"warp"]
        );
    }

    #[test]
    fn providers_finds_every_source() {
        let mut tree = tree();
        tree.insert("salvaged-warp", Unlock::free().granting([Cap::Warp]));
        assert_eq!(tree.providers(&Cap::Warp), vec![&"salvaged-warp", &"warp"]);
        assert_eq!(
            tree.dependencies(&"cloaking"),
            vec![&"salvaged-warp", &"warp"],
            "either source unlocks it"
        );
    }

    #[test]
    fn a_node_never_depends_on_itself() {
        let mut tree = Unlocks::new();
        tree.insert(
            "bootstrap",
            Unlock::new(Requirement::has(Cap::Fusion)).granting([Cap::Fusion]),
        );
        assert!(tree.dependencies(&"bootstrap").is_empty());
    }

    #[test]
    fn taking_a_node_adds_its_grants() {
        let tree = tree();
        let mut held = BTreeSet::new();

        assert!(!tree.take(&"warp", &mut held), "locked nodes do not apply");
        assert!(held.is_empty());

        assert!(tree.take(&"fusion", &mut held));
        assert!(held.contains(&Cap::Fusion));
        assert!(tree.take(&"warp", &mut held));
        assert!(held.contains(&Cap::Warp));

        assert!(tree.take(&"warp", &mut held), "taking twice is harmless");
        assert_eq!(held.len(), 2);
        assert!(!tree.take(&"absent", &mut held));
    }

    /// A host that stores researched keys and computes Shields from its fleet.
    struct Empire {
        researched: BTreeSet<Cap>,
        warships: u32,
    }

    impl Has<Cap> for Empire {
        fn has(&self, key: &Cap) -> bool {
            match key {
                Cap::Shields => self.warships >= 3,
                stored => self.researched.contains(stored),
            }
        }
    }

    impl Extend<Cap> for Empire {
        fn extend<T: IntoIterator<Item = Cap>>(&mut self, keys: T) {
            self.researched.extend(keys);
        }
    }

    #[test]
    fn a_node_behind_a_computed_key_unlocks() {
        let mut tree = Unlocks::new();
        tree.insert(
            "fleet-doctrine",
            Unlock::new(Requirement::all([
                Requirement::has(Cap::Fusion),
                Requirement::has(Cap::Shields),
            ]))
            .granting([Cap::Cloaking]),
        );
        let mut empire = Empire {
            researched: BTreeSet::from([Cap::Fusion]),
            warships: 1,
        };

        assert!(!tree.is_available(&"fleet-doctrine", &empire));
        assert_eq!(
            tree.evaluate(&"fleet-doctrine", &empire)
                .map(|result| result.missing()),
            Some(vec![Cap::Shields])
        );

        empire.warships = 3;
        assert!(tree.is_available(&"fleet-doctrine", &empire));
        assert!(tree.take(&"fleet-doctrine", &mut empire));
        assert!(empire.researched.contains(&Cap::Cloaking));

        // Shields comes from outside the tree, so validation accepts the node.
        let warnings = tree.validate(&empire);
        assert!(!warnings.iter().any(|warning| matches!(
            warning,
            UnlockWarning::Ungrantable {
                capability: Cap::Shields,
                ..
            }
        )));
    }

    #[test]
    fn a_frontier_falls_out_of_the_missing_count() {
        let tree = tree();
        let held = BTreeSet::new();
        // A satisfied node has no gap at all, so one gap means locked.
        let frontier: Vec<_> = tree
            .ids()
            .filter(|id| {
                tree.evaluate(id, &held)
                    .is_some_and(|result| result.missing().len() == 1)
            })
            .copied()
            .collect();
        assert_eq!(frontier, vec!["antimatter", "cloaking", "warp"]);
    }

    #[test]
    fn evaluate_explains_a_locked_node() {
        let tree = tree();
        let result = tree.evaluate(&"cloaking", &BTreeSet::new()).unwrap();
        assert!(!result.satisfied());
        assert_eq!(result.missing(), vec![Cap::Warp]);
        assert!(tree.evaluate(&"absent", &BTreeSet::new()).is_none());
    }

    fn rank(tree: &Unlocks<&'static str, Cap>, id: &'static str) -> Option<usize> {
        tree.ranks().get(id).copied()
    }

    #[test]
    fn rank_counts_required_depth() {
        let tree = tree();
        assert_eq!(rank(&tree, "fusion"), Some(0));
        assert_eq!(rank(&tree, "warp"), Some(1));
        assert_eq!(rank(&tree, "antimatter"), Some(1));
        assert_eq!(rank(&tree, "cloaking"), Some(2));
        assert_eq!(rank(&tree, "shields"), Some(0), "no required dependency");
        assert_eq!(rank(&tree, "absent"), None);
    }

    #[test]
    fn rank_takes_the_longest_route() {
        let mut tree = Unlocks::new();
        tree.insert("a", Unlock::free().granting([Cap::Fusion]));
        tree.insert(
            "b",
            Unlock::new(Requirement::has(Cap::Fusion)).granting([Cap::Warp]),
        );
        // c depends on both a and b, so the long route wins.
        tree.insert(
            "c",
            Unlock::new(Requirement::all([
                Requirement::has(Cap::Fusion),
                Requirement::has(Cap::Warp),
            ]))
            .granting([Cap::Cloaking]),
        );
        assert_eq!(rank(&tree, "c"), Some(2));
    }

    #[test]
    fn a_view_shows_each_status() {
        let tree = tree();
        let mut held = BTreeSet::new();
        let mut taken = BTreeSet::new();
        tree.take(&"fusion", &mut held);
        taken.insert("fusion");

        let statuses: Vec<_> = tree
            .view(&held, &taken)
            .into_iter()
            .map(|node| (*node.id, node.status))
            .collect();
        assert_eq!(
            statuses,
            vec![
                ("antimatter", NodeStatus::Available),
                ("cloaking", NodeStatus::Locked),
                ("fusion", NodeStatus::Unlocked),
                ("shields", NodeStatus::Locked),
                ("warp", NodeStatus::Available),
            ]
        );
    }

    #[test]
    fn a_view_agrees_with_the_single_queries() {
        let tree = tree();
        let held = BTreeSet::from([Cap::Fusion, Cap::Warp]);
        let taken = BTreeSet::from(["fusion"]);
        let ranks = tree.ranks();

        let view = tree.view(&held, &taken);
        assert_eq!(view.len(), tree.len());
        for node in view {
            assert_eq!(Some(node.status), tree.status(node.id, &held, &taken));
            assert_eq!(node.rank, ranks.get(node.id).copied());
            assert_eq!(node.dependencies, tree.dependencies(node.id));
            assert_eq!(
                node.optional_dependencies,
                tree.optional_dependencies(node.id)
            );
        }
        assert_eq!(tree.status(&"absent", &held, &taken), None);
    }

    #[test]
    fn a_taken_node_stays_unlocked_when_its_rule_fails() {
        let tree = tree();
        let taken = BTreeSet::from(["warp"]);
        // The capability was revoked, but the host's record still says taken.
        assert_eq!(
            tree.status(&"warp", &BTreeSet::new(), &taken),
            Some(NodeStatus::Unlocked)
        );
    }

    #[test]
    fn a_cycle_has_no_rank() {
        let mut tree = Unlocks::new();
        tree.insert(
            "a",
            Unlock::new(Requirement::has(Cap::Warp)).granting([Cap::Fusion]),
        );
        tree.insert(
            "b",
            Unlock::new(Requirement::has(Cap::Fusion)).granting([Cap::Warp]),
        );
        assert!(tree.ranks().is_empty());
        let view = tree.view(&BTreeSet::new(), &BTreeSet::new());
        assert!(view.iter().all(|node| node.rank.is_none()));
        assert!(view.iter().all(|node| node.status == NodeStatus::Locked));
    }

    #[test]
    fn a_cycle_does_not_hide_the_rest_of_the_graph() {
        let mut tree = tree();
        tree.insert(
            "loop-a",
            Unlock::new(Requirement::has(Cap::Shields)).granting([Cap::Shields]),
        );
        let ranks = tree.ranks();
        assert_eq!(ranks.get("fusion"), Some(&0));
        assert_eq!(ranks.get("cloaking"), Some(&2));
    }

    #[test]
    fn validate_accepts_a_sound_tree() {
        assert!(tree().validate(&BTreeSet::new()).is_empty());
    }

    #[test]
    fn validate_reports_a_capability_nothing_grants() {
        let mut tree = Unlocks::new();
        tree.insert(
            "orbital-yard",
            Unlock::new(Requirement::has(Cap::Fusion)).granting([Cap::Warp]),
        );
        assert_eq!(
            tree.validate(&BTreeSet::new()),
            vec![UnlockWarning::Ungrantable {
                id: "orbital-yard",
                capability: Cap::Fusion,
            }]
        );
    }

    #[test]
    fn validate_accepts_a_capability_granted_elsewhere() {
        let mut tree = Unlocks::new();
        tree.insert(
            "orbital-yard",
            Unlock::new(Requirement::has(Cap::Fusion)).granting([Cap::Warp]),
        );
        // A species trait grants Fusion, so the node is reachable after all.
        let external = BTreeSet::from([Cap::Fusion]);
        assert!(tree.validate(&external).is_empty());
    }

    #[test]
    fn validate_reports_cycles_empty_grants_and_bad_requirements() {
        let mut tree = Unlocks::new();
        tree.insert("dead-end", Unlock::free());
        tree.insert(
            "impossible",
            Unlock::new(Requirement::at_least(4, [Requirement::has(Cap::Fusion)]))
                .granting([Cap::Shields]),
        );
        tree.insert(
            "loop-a",
            Unlock::new(Requirement::has(Cap::Warp)).granting([Cap::Fusion]),
        );
        tree.insert(
            "loop-b",
            Unlock::new(Requirement::has(Cap::Fusion)).granting([Cap::Warp]),
        );

        let warnings = tree.validate(&BTreeSet::new());
        assert!(warnings.contains(&UnlockWarning::GrantsNothing { id: "dead-end" }));
        assert!(warnings.contains(&UnlockWarning::Cycle { id: "loop-a" }));
        assert!(warnings.contains(&UnlockWarning::Cycle { id: "loop-b" }));
        assert!(warnings.iter().any(|warning| matches!(
            warning,
            UnlockWarning::Requirement { id, message } if *id == "impossible" && message.contains("never hold")
        )));
    }

    #[test]
    fn validate_reports_a_node_that_only_unlocks_itself() {
        let mut tree = Unlocks::new();
        tree.insert(
            "bootstrap",
            Unlock::new(Requirement::has(Cap::Fusion)).granting([Cap::Fusion]),
        );
        // It forms no cycle, because a node never depends on itself.
        assert_eq!(rank(&tree, "bootstrap"), Some(0));
        assert!(
            tree.validate(&BTreeSet::new())
                .contains(&UnlockWarning::Ungrantable {
                    id: "bootstrap",
                    capability: Cap::Fusion,
                })
        );
    }

    #[test]
    fn a_second_source_clears_the_self_grant_warning() {
        let mut tree = Unlocks::new();
        tree.insert(
            "bootstrap",
            Unlock::new(Requirement::has(Cap::Fusion)).granting([Cap::Fusion]),
        );
        tree.insert("reactor", Unlock::free().granting([Cap::Fusion]));
        assert!(tree.validate(&BTreeSet::new()).is_empty());
    }

    #[test]
    fn a_warning_prints_a_readable_line() {
        let warning: UnlockWarning<&str, Cap> = UnlockWarning::Ungrantable {
            id: "warp",
            capability: Cap::Fusion,
        };
        assert_eq!(
            alloc::format!("{warning}"),
            r#""warp" requires Fusion, which nothing else grants"#
        );
    }

    #[test]
    fn taking_every_available_node_walks_the_whole_tree() {
        let tree = tree();
        let mut held = BTreeSet::new();
        let mut taken = alloc::collections::BTreeSet::new();

        loop {
            let next: Vec<_> = tree
                .available(&held)
                .filter(|id| !taken.contains(*id))
                .copied()
                .collect();
            if next.is_empty() {
                break;
            }
            for id in next {
                tree.take(&id, &mut held);
                taken.insert(id);
            }
        }

        assert_eq!(taken.len(), tree.len(), "every node became reachable");
        assert_eq!(held.len(), 5);
    }
}
