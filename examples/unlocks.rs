//! Requirements that gate what an empire can do.
//!
//! Run it with `cargo run --example unlocks`.

use std::collections::BTreeSet;

use plotline::{Answer, Has, Library, Requirement, Runner, Status, Step};

/// The host owns this vocabulary. The crate never interprets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Cap {
    FrozenHabitation,
    HighGravityHabitation,
    ToxicHabitation,
    Shipyard,
    AlliedFleet,
}

/// Capabilities come from several sources. The rules do not care which.
struct Empire {
    capabilities: BTreeSet<Cap>,
    allied_ships: u32,
}

impl Has<Cap> for Empire {
    fn has(&self, key: &Cap) -> bool {
        match key {
            // Computed from other state, not stored.
            Cap::AlliedFleet => self.allied_ships > 0,
            stored => self.capabilities.contains(stored),
        }
    }
}

/// What a colony sequence asks the host to do.
enum Order {
    Report(&'static str),
    Land,
    Settle(Cap),
}

fn main() {
    let mut empire = Empire {
        capabilities: BTreeSet::from([Cap::FrozenHabitation]),
        allied_ships: 2,
    };

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
    report(&deneb_iv, &empire);
    send_colony_ships(&deneb_iv, &mut empire);

    // Researching a technology grants a capability.
    empire.capabilities.insert(Cap::HighGravityHabitation);

    println!("\n== Deneb IV, after Gravity Compensation ==");
    report(&deneb_iv, &empire);
    send_colony_ships(&deneb_iv, &mut empire);

    // The colony granted ToxicHabitation, which opens a second world.
    let ophiuchus_ii = Requirement::all([
        Requirement::has(Cap::ToxicHabitation),
        Requirement::any([
            Requirement::has(Cap::FrozenHabitation),
            Requirement::has(Cap::HighGravityHabitation),
        ]),
    ]);

    println!("\n== Ophiuchus II, after the colony ==");
    report(&ophiuchus_ii, &empire);
}

/// Prints the boolean answer, the conservative shortfall, and the full tree.
fn report(requirement: &Requirement<Cap>, empire: &Empire) {
    let result = requirement.evaluate(empire);
    println!("satisfied: {}", result.satisfied());
    println!("missing:   {:?}", result.missing());
    println!("{result}");
}

/// Runs a colony sequence that the same requirement gates.
fn send_colony_ships(world: &Requirement<Cap>, empire: &mut Empire) {
    let mut library = Library::new();
    library.insert(
        "colonize",
        [Step::branch(world.clone(), "landing", "turn-back")],
    );
    library.insert(
        "landing",
        [
            Step::act(Order::Land),
            Step::act(Order::Settle(Cap::ToxicHabitation)),
        ],
    );
    library.insert(
        "turn-back",
        [Step::act(Order::Report("The colony ships turn back."))],
    );

    let mut runner = Runner::default();
    runner.start("colonize").expect("no other chain runs");
    let mut status = runner.advance(&library, empire);
    while let Status::Act(order) = status {
        match order {
            Order::Report(line) => println!("{line}"),
            Order::Land => println!("The first dome goes up."),
            Order::Settle(cap) => {
                empire.capabilities.insert(*cap);
                println!("The colony grants {cap:?}.");
            }
        }
        status = runner.resume(Answer::Done, &library, empire);
    }
}
