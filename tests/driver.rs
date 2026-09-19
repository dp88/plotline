//! A host drives the runner and performs every action itself.

use std::collections::BTreeSet;

use plotline::{
    Abort, Answer, Library, NodeStatus, Requirement, Runner, Status, Step, Unlock, Unlocks,
};

#[derive(Debug)]
enum Action {
    Say(&'static str),
    Grant(Key),
    Choose(Vec<&'static str>),
    Research(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Ring,
    Fusion,
    Warp,
}

type Script = Library<Action, Key>;
type Tree = Unlocks<&'static str, Key>;

/// A host with the state its actions change.
#[derive(Default)]
struct Host {
    held: BTreeSet<Key>,
    taken: BTreeSet<&'static str>,
    said: Vec<&'static str>,
    picks: Vec<usize>,
}

impl Host {
    fn play(&mut self, library: &Script, tree: &Tree, start: &str) -> Result<(), Abort> {
        let mut runner = Runner::default();
        runner.start(start).unwrap();
        let mut status = runner.advance(library, &self.held);
        loop {
            let answer = match status {
                Status::Act(action) => self.perform(action, tree),
                Status::Finished => return Ok(()),
                Status::Aborted(abort) => return Err(abort),
                other => panic!("a driven chain reported {other:?}"),
            };
            status = runner.resume(answer, library, &self.held);
        }
    }

    fn perform(&mut self, action: &Action, tree: &Tree) -> Answer {
        match action {
            Action::Say(line) => self.said.push(line),
            Action::Grant(key) => {
                self.held.insert(*key);
            }
            Action::Choose(targets) => return Answer::goto(targets[self.picks.remove(0)]),
            Action::Research(node) => {
                if tree.take(node, &mut self.held) {
                    self.taken.insert(node);
                }
            }
        }
        Answer::Done
    }
}

fn say(line: &'static str) -> Step<Action, Key> {
    Step::act(Action::Say(line))
}

#[test]
fn a_grant_during_a_chain_opens_a_later_gate() {
    let mut library = Script::new();
    library.insert(
        "quest",
        [
            Step::when(Requirement::has(Key::Ring), say("You already have it.")),
            say("You search the well."),
            Step::act(Action::Grant(Key::Ring)),
            Step::when(Requirement::has(Key::Ring), say("You found the ring!")),
        ],
    );

    let mut host = Host::default();
    host.play(&library, &Tree::new(), "quest").unwrap();
    assert_eq!(host.said, ["You search the well.", "You found the ring!"]);
}

#[test]
fn a_choice_leads_to_the_branch_the_player_picked() {
    let mut library = Script::new();
    library.insert(
        "hub",
        [
            say("Will you help?"),
            Step::act(Action::Choose(vec!["yes", "no"])),
        ],
    );
    library.insert("yes", [say("Thank you.")]);
    library.insert("no", [say("A pity.")]);

    for (pick, reply) in [(0, "Thank you."), (1, "A pity.")] {
        let mut host = Host {
            picks: vec![pick],
            ..Host::default()
        };
        host.play(&library, &Tree::new(), "hub").unwrap();
        assert_eq!(host.said, ["Will you help?", reply]);
    }
}

#[test]
fn a_sequence_researches_tree_nodes_and_the_view_shows_them() {
    let mut tree = Tree::new();
    tree.insert("fusion-power", Unlock::free().granting([Key::Fusion]));
    tree.insert(
        "warp-drive",
        Unlock::new(Requirement::has(Key::Fusion)).granting([Key::Warp]),
    );

    let mut library = Script::new();
    library.insert(
        "lab",
        [
            Step::act(Action::Research("warp-drive")),
            Step::act(Action::Research("fusion-power")),
            Step::act(Action::Research("warp-drive")),
            Step::branch(Requirement::has(Key::Warp), "launch", "grounded"),
        ],
    );
    library.insert("launch", [say("Warp engaged.")]);
    library.insert("grounded", [say("The ship stays in dock.")]);

    let mut host = Host::default();
    host.play(&library, &tree, "lab").unwrap();

    // The first attempt at warp-drive came too early, and the tree refused it.
    assert_eq!(host.said, ["Warp engaged."]);
    let statuses: Vec<_> = tree
        .view(&host.held, &host.taken)
        .into_iter()
        .map(|node| node.status)
        .collect();
    assert_eq!(statuses, [NodeStatus::Unlocked, NodeStatus::Unlocked]);
}
