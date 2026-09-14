//! Requirements that gate what an empire can do.
//!
//! Run it with `cargo run --example unlocks`.

use core::task::Poll;

use plotline::{
    CapabilitySet, Explanation, Library, QueryCtx, Requirement, Runner, Sequence, TypeMap,
    conditions, effects, steps,
};

/// The host owns this vocabulary. The crate never interprets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Cap {
    FrozenHabitation,
    HighGravityHabitation,
    ToxicHabitation,
    Shipyard,
}

/// Host state that the capability vocabulary does not cover.
struct Empire {
    colony_ships: usize,
}

fn main() {
    // Capabilities arrive from several sources. The evaluator does not care
    // which one granted what.
    let species = [Cap::FrozenHabitation];
    let technologies = [Cap::Shipyard];
    let mut held = CapabilitySet::new();
    held.extend(species);
    held.extend(technologies);

    // A world states what it demands. This is plain data, so it could just as
    // easily come from a file.
    let deneb_iv = Requirement::all([
        Requirement::has(Cap::FrozenHabitation),
        Requirement::has(Cap::HighGravityHabitation),
        Requirement::any([
            Requirement::has(Cap::Shipyard),
            Requirement::named("borrowed-fleet"),
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

    colonize(deneb_iv, held);
}

/// Prints the boolean answer, the conservative shortfall, and the full tree.
fn report(requirement: &Requirement<Cap>, held: &CapabilitySet<Cap>) {
    let result = requirement.evaluate(held);
    println!("satisfied: {}", result.satisfied());
    println!("missing:   {:?}", result.missing());
    print_tree(&Explanation::from(&result), 0);
}

fn print_tree(node: &Explanation, depth: usize) {
    let mark = if node.satisfied { '✓' } else { '✗' };
    println!("{:indent$}{mark} {}", "", node.summary, indent = depth * 2);
    for child in &node.children {
        print_tree(child, depth + 1);
    }
}

/// Runs a sequence whose first step refuses to continue without the capabilities.
fn colonize(requirement: Requirement<Cap>, held: CapabilitySet<Cap>) {
    println!("\n== Colonizing ==");

    // A name lets a stored rule reach state the capability vocabulary misses.
    let mut checks = conditions::Checks::new();
    checks.register(
        "borrowed-fleet",
        conditions::check("An ally lent us ships", |query: &QueryCtx<'_>| {
            query
                .service::<Empire>()
                .is_some_and(|e| e.colony_ships > 0)
        }),
    );

    let mut services = TypeMap::new();
    services.insert(Empire { colony_ships: 0 });
    services.insert(checks);
    services.insert(held);

    let mut library = Library::new();
    let landing = library.insert(
        Sequence::new("landing")
            .with_step(steps::when(Requirement::not(requirement), steps::stop()))
            .with_step(steps::run("Land the colony", |_ctx| {
                println!("The first dome goes up.");
            }))
            // The colony itself grants a new capability.
            .with_step(steps::ApplyEffects {
                effects: vec![Box::new(effects::grant(Cap::ToxicHabitation))],
            }),
    );

    let mut runner = Runner::default();
    runner.start(landing, None).unwrap();
    while let Poll::Pending = runner.advance(&mut library, &mut services) {}

    let held = services.get::<CapabilitySet<Cap>>().unwrap();
    println!("capabilities now: {:?}", held.iter().collect::<Vec<_>>());
}
