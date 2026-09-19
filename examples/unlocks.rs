//! Requirements that gate what an empire can do.
//!
//! Run it with `cargo run --example unlocks`.

use std::collections::BTreeSet;

use plotline::Requirement;

/// The host owns this vocabulary. The crate never interprets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Cap {
    FrozenHabitation,
    HighGravityHabitation,
    ToxicHabitation,
    Shipyard,
    AlliedFleet,
}

fn main() {
    // Capabilities arrive from several sources. The evaluator does not care
    // which one granted what.
    let species = [Cap::FrozenHabitation];
    let technologies = [Cap::Shipyard];
    let mut held = BTreeSet::new();
    held.extend(species);
    held.extend(technologies);

    // A world states what it demands. This is plain data, so it could just as
    // easily come from a file.
    let deneb_iv = Requirement::all([
        Requirement::has(Cap::FrozenHabitation),
        Requirement::has(Cap::HighGravityHabitation),
        Requirement::any([
            Requirement::has(Cap::Shipyard),
            Requirement::has(Cap::AlliedFleet),
        ]),
    ]);

    println!("== Deneb IV, before Gravity Compensation ==");
    report(&deneb_iv, &held);

    // Researching a technology grants a capability.
    held.insert(Cap::HighGravityHabitation);

    println!("\n== Deneb IV, after Gravity Compensation ==");
    report(&deneb_iv, &held);

    // A world we still cannot reach, to show a choice rather than a shortfall.
    let ophiuchus_ii = Requirement::all([
        Requirement::has(Cap::ToxicHabitation),
        Requirement::any([
            Requirement::has(Cap::FrozenHabitation),
            Requirement::has(Cap::HighGravityHabitation),
        ]),
    ]);

    println!("\n== Ophiuchus II ==");
    report(&ophiuchus_ii, &held);
}

/// Prints the boolean answer, the conservative shortfall, and the full tree.
fn report(requirement: &Requirement<Cap>, held: &BTreeSet<Cap>) {
    let result = requirement.evaluate(held);
    println!("satisfied: {}", result.satisfied());
    println!("missing:   {:?}", result.missing());
    println!("{result}");
}
