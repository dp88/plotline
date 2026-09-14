//! A technology tree whose edges nobody authored.
//!
//! Run it with `cargo run --example techtree`.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use plotline::{CapabilitySet, Explanation, Requirement, Status, Unlock, Unlocks};

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
    let mut held = CapabilitySet::new();
    let mut taken = BTreeSet::new();

    println!("== The tree, before any research ==");
    draw(&tree, &held, &taken);

    println!("\n== Why deep-space-colonies is locked ==");
    let result = tree.evaluate(&"deep-space-colonies", &held).unwrap();
    print_tree(&Explanation::from(&result), 0);
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
fn draw(tree: &Tree, held: &CapabilitySet<Cap>, taken: &BTreeSet<&str>) {
    let ranks = tree.ranks();
    let width = tree.ids().map(|id| id.len()).max().unwrap_or(0);

    let mut rows: Vec<_> = tree.ids().map(|id| (ranks.get(id), id)).collect();
    rows.sort_by_key(|(rank, id)| (rank.copied().unwrap_or(usize::MAX), *id));

    for (rank, id) in rows {
        let mark = match (taken.contains(id), tree.status(id, held)) {
            (true, _) => '●',
            (false, Some(Status::Available)) => '○',
            _ => '·',
        };
        let rank = rank.map_or_else(|| "  ?".into(), |rank| format!("{rank:>3}"));

        let mut edges = String::new();
        let hard = tree.dependencies(id);
        if !hard.is_empty() {
            let _ = write!(edges, "  after {hard:?}");
        }
        let soft = tree.optional_dependencies(id);
        if !soft.is_empty() {
            let _ = write!(edges, "  or one of {soft:?}");
        }

        println!(
            "{}",
            format!("{rank}  {mark} {id:width$}{edges}").trim_end()
        );
    }
    println!("      ● taken   ○ available   · locked");
}

fn print_tree(node: &Explanation, depth: usize) {
    let mark = if node.satisfied { '✓' } else { '✗' };
    println!("{:indent$}{mark} {}", "", node.summary, indent = depth * 2);
    for child in &node.children {
        print_tree(child, depth + 1);
    }
}

fn report_warnings(tree: &Tree) {
    let warnings = tree.validate(&CapabilitySet::new());
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
