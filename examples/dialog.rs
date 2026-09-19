//! A conversation with a choice, an event, and a quest gate.
//!
//! Run it with `cargo run --example dialog`.

use std::collections::BTreeSet;

use plotline::{Answer, Has, Library, Requirement, Runner, SequenceRef, Status, Step};

/// What the host knows how to do. The runner never interprets it.
#[derive(Debug)]
enum Action {
    /// A line of dialog.
    Say(&'static str, &'static str),
    /// A choice of replies. Each reply names the sequence it leads to.
    Choose(Vec<(&'static str, SequenceRef)>),
    /// An event between two lines.
    Shake,
    /// Records a fact that later rules read.
    Remember(Key),
    /// Moves an item out of the player's pack.
    Take(Item),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Item {
    GoldRing,
}

/// What the rules ask about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    /// Stored after the introduction.
    MetTheElder,
    /// Computed from the pack.
    CarriesRing,
    /// Computed from the clock.
    Night,
}

struct World {
    facts: BTreeSet<Key>,
    pack: BTreeSet<Item>,
    hour: u32,
}

impl Has<Key> for World {
    fn has(&self, key: &Key) -> bool {
        match key {
            Key::CarriesRing => self.pack.contains(&Item::GoldRing),
            Key::Night => self.hour >= 20 || self.hour < 6,
            Key::MetTheElder => self.facts.contains(key),
        }
    }
}

type Script = Library<Action, Key>;

fn say(who: &'static str, line: &'static str) -> Step<Action, Key> {
    Step::act(Action::Say(who, line))
}

/// The script is plain data. It could just as easily come from a file.
fn script() -> Script {
    let mut library = Script::new();
    library.insert(
        "hub",
        [
            Step::when(
                Requirement::not(Requirement::has(Key::MetTheElder)),
                Step::call("introduction"),
            ),
            say("Elder", "Have you found my ring?"),
            Step::when(Requirement::has(Key::CarriesRing), Step::goto("returned")),
            Step::act(Action::Choose(vec![
                ("Where did you lose it?", "hint".into()),
                ("Not yet.", "later".into()),
            ])),
        ],
    );
    library.insert(
        "introduction",
        [
            say("Elder", "Welcome, traveler. I keep this village."),
            Step::act(Action::Remember(Key::MetTheElder)),
        ],
    );
    library.insert(
        "hint",
        [
            say("Elder", "Near the old well."),
            Step::when(
                Requirement::has(Key::Night),
                say("Elder", "Look for it by moonlight."),
            ),
            Step::goto("hub"),
        ],
    );
    library.insert("later", [say("Elder", "Come back when you have.")]);
    library.insert(
        "returned",
        [
            Step::act(Action::Shake),
            Step::act(Action::Take(Item::GoldRing)),
            say("Elder", "You have my thanks."),
        ],
    );
    library
}

/// Lists the sequences an action names, so validation can check them.
fn refs_in(action: &Action) -> Vec<SequenceRef> {
    match action {
        Action::Choose(replies) => replies.iter().map(|(_, target)| target.clone()).collect(),
        _ => Vec::new(),
    }
}

fn main() {
    let library = script();
    for warning in library.validate(refs_in) {
        println!("warning: {warning}");
    }

    let mut world = World {
        facts: BTreeSet::new(),
        pack: BTreeSet::new(),
        hour: 22,
    };

    println!("== First visit ==");
    // The player asks for a hint, then admits they have no ring yet.
    talk(&library, &mut world, &mut [0, 1].into_iter());

    println!("\n== After the search ==");
    world.pack.insert(Item::GoldRing);
    talk(&library, &mut world, &mut std::iter::empty());
    println!("pack: {:?}", world.pack);
}

/// Runs one conversation. `picks` stands in for the player's choices.
fn talk(library: &Script, world: &mut World, picks: &mut impl Iterator<Item = usize>) {
    let mut runner = Runner::default();
    runner.start("hub").expect("no other chain runs");

    let mut status = runner.advance(library, world);
    loop {
        let answer = match status {
            Status::Act(action) => perform(action, world, picks),
            Status::Finished => return,
            Status::Aborted(why) => {
                println!("[the conversation broke off: {why}]");
                return;
            }
            Status::Waiting | Status::Idle => unreachable!("every action gets an answer at once"),
        };
        status = runner.resume(answer, library, world);
    }
}

/// Performs one action. The host's effects live here and nowhere else.
fn perform(action: &Action, world: &mut World, picks: &mut impl Iterator<Item = usize>) -> Answer {
    match action {
        Action::Say(who, line) => println!("{who}: {line}"),
        Action::Choose(replies) => {
            let (reply, target) = &replies[picks.next().unwrap_or(0)];
            println!("> {reply}");
            return Answer::Goto(Some(target.clone()));
        }
        Action::Shake => println!("[the ground trembles]"),
        Action::Remember(key) => {
            world.facts.insert(*key);
        }
        Action::Take(item) => {
            world.pack.remove(item);
            println!("[you hand over the {item:?}]");
        }
    }
    Answer::Done
}
