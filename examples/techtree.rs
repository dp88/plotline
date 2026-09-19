//! A technology tree whose edges nobody authored.
//!
//! Run it with `cargo run --example techtree`.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use plotline::{NodeStatus, Requirement, Unlock, Unlocks};

/// The host owns this vocabulary. The crate never interprets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Cap {
    Fusion,
    Warp,
    JumpGate,
    Cloaking,
    DeepColony,
    FrozenHabitation,
    HighGravityHabitation,
}

type Tree = Unlocks<&'static str, Cap>;

fn tech_tree() -> Tree {
    let mut tree = Tree::new();

    tree.insert("fusion-power", Unlock::free().granting([Cap::Fusion]));
    tree.insert(
        "cryogenics",
        Unlock::free().granting([Cap::FrozenHabitation]),
    );

    tree.insert(
        "warp-drive",
        Unlock::new(Requirement::has(Cap::Fusion)).granting([Cap::Warp]),
    );
    tree.insert(
        "gravity-compensation",
        Unlock::new(Requirement::has(Cap::Fusion)).granting([Cap::HighGravityHabitation]),
    );
    tree.insert(
        "jump-gates",
        Unlock::new(Requirement::all([
            Requirement::has(Cap::Fusion),
            Requirement::has(Cap::Warp),
        ]))
        .granting([Cap::JumpGate]),
    );
    tree.insert(
        "cloaking-field",
        Unlock::new(Requirement::has(Cap::Warp)).granting([Cap::Cloaking]),
    );

    // Either habitation route works, so neither is a required edge.
    tree.insert(
        "deep-space-colonies",
        Unlock::new(Requirement::all([
            Requirement::has(Cap::JumpGate),
            Requirement::any([
                Requirement::has(Cap::FrozenHabitation),
                Requirement::has(Cap::HighGravityHabitation),
            ]),
        ]))
        .granting([Cap::DeepColony]),
    );

    tree
}

fn main() {
    let tree = tech_tree();
    let mut held = BTreeSet::new();
    let mut taken = BTreeSet::new();

    println!("== The tree, before any research ==");
    draw(&tree, &held, &taken);

    println!("\n== Why deep-space-colonies is locked ==");
    let result = tree.evaluate(&"deep-space-colonies", &held).unwrap();
    println!("{result}");
    println!("hard shortfall: {:?}", result.missing());

    println!("\n== Researching everything reachable ==");
    let mut round = 1;
    loop {
        let next: Vec<_> = tree
            .available(&held)
            .filter(|id| !taken.contains(*id))
            .copied()
            .collect();
        if next.is_empty() {
            break;
        }
        println!("round {round}: {next:?}");
        for id in next {
            tree.take(&id, &mut held);
            taken.insert(id);
        }
        round += 1;
    }

    println!("\n== The tree, fully researched ==");
    draw(&tree, &held, &taken);

    println!("\n== Authoring check ==");
    report_warnings(&tree);
    report_warnings(&broken_tree());
}

/// Prints the tree by rank. Rank is the column a layout would use.
fn draw(tree: &Tree, held: &BTreeSet<Cap>, taken: &BTreeSet<&str>) {
    let width = tree.ids().map(|id| id.len()).max().unwrap_or(0);

    let mut nodes = tree.view(held, taken);
    nodes.sort_by_key(|node| (node.rank.unwrap_or(usize::MAX), *node.id));

    for node in nodes {
        let mark = match node.status {
            NodeStatus::Unlocked => '●',
            NodeStatus::Available => '○',
            NodeStatus::Locked => '·',
        };
        let rank = node
            .rank
            .map_or_else(|| "  ?".into(), |rank| format!("{rank:>3}"));

        let mut edges = String::new();
        if !node.dependencies.is_empty() {
            let _ = write!(edges, "  after {:?}", node.dependencies);
        }
        if !node.optional_dependencies.is_empty() {
            let _ = write!(edges, "  or one of {:?}", node.optional_dependencies);
        }

        let id = node.id;
        println!(
            "{}",
            format!("{rank}  {mark} {id:width$}{edges}").trim_end()
        );
    }
    println!("      ● taken   ○ available   · locked");
}

fn report_warnings(tree: &Tree) {
    let warnings = tree.validate(&BTreeSet::new());
    if warnings.is_empty() {
        println!("no problems in a {}-node tree", tree.len());
        return;
    }
    for warning in warnings {
        println!("  {warning}");
    }
}

/// A tree an author got wrong, to show what validation catches.
fn broken_tree() -> Tree {
    let mut tree = Tree::new();
    // Nothing grants Cloaking, so this node is dead content.
    tree.insert(
        "phase-cannon",
        Unlock::new(Requirement::has(Cap::Cloaking)).granting([Cap::Fusion]),
    );
    // These two wait on each other forever.
    tree.insert(
        "gate-theory",
        Unlock::new(Requirement::has(Cap::Warp)).granting([Cap::JumpGate]),
    );
    tree.insert(
        "gate-engines",
        Unlock::new(Requirement::has(Cap::JumpGate)).granting([Cap::Warp]),
    );
    tree
}
