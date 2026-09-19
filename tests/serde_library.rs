//! A library of sequences survives a trip through a data file.
#![cfg(feature = "serde")]

use std::collections::BTreeSet;

use plotline::{Answer, Library, Requirement, Runner, SequenceRef, Status, Step};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Action {
    Say { who: String, line: String },
    Choose(Vec<(String, SequenceRef)>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
enum Key {
    HasRing,
}

type Script = Library<Action, Key>;

fn say(who: &str, line: &str) -> Step<Action, Key> {
    Step::act(Action::Say {
        who: who.to_owned(),
        line: line.to_owned(),
    })
}

fn ring_quest() -> Script {
    let mut library = Script::new();
    library.insert(
        "hub",
        [
            say("Elder", "Have you found my ring?"),
            Step::when(Requirement::has(Key::HasRing), Step::goto("thanks")),
            Step::act(Action::Choose(vec![
                ("Not yet.".to_owned(), "later".into()),
                ("Where did you lose it?".to_owned(), "hint".into()),
            ])),
        ],
    );
    library.insert(
        "thanks",
        [say("Elder", "You have my thanks."), Step::stop()],
    );
    library.insert("later", [say("Elder", "Come back when you have.")]);
    library.insert("hint", [say("Elder", "Near the old well."), Step::Return]);
    library
}

/// The shape an author writes by hand. A sequence name is a plain string.
const HAND_WRITTEN: &str = r#"
    {
      "hub": [
        { "Act": { "Say": { "who": "Elder", "line": "Have you found my ring?" } } },
        { "When": { "rule": { "Has": "HasRing" }, "step": { "Goto": "thanks" } } },
        { "Act": { "Choose": [["Not yet.", "later"], ["Where did you lose it?", "hint"]] } }
      ],
      "thanks": [
        { "Act": { "Say": { "who": "Elder", "line": "You have my thanks." } } },
        { "Goto": null }
      ],
      "later": [{ "Act": { "Say": { "who": "Elder", "line": "Come back when you have." } } }],
      "hint": [
        { "Act": { "Say": { "who": "Elder", "line": "Near the old well." } } },
        "Return"
      ]
    }
    "#;

#[test]
fn a_library_round_trips() {
    let encoded = serde_json::to_string(&ring_quest()).unwrap();
    let decoded: Script = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, ring_quest());
}

#[test]
fn a_hand_written_library_loads() {
    let library: Script = serde_json::from_str(HAND_WRITTEN).unwrap();
    assert_eq!(library, ring_quest());
}

#[test]
fn a_misspelled_field_is_an_error() {
    // Without the check, the misspelled target loads as None and the chain ends.
    let document = r#"
    { "hub": [{ "Branch": { "rule": { "All": [] }, "if_ture": "hub", "if_false": null } }] }
    "#;
    let error = serde_json::from_str::<Script>(document).unwrap_err();
    assert!(error.to_string().contains("if_ture"), "{error}");
}

#[test]
fn a_repeated_sequence_name_is_an_error() {
    // Without the check, the second "hub" replaces the first without a word.
    let document = r#"{ "hub": ["Return"], "hub": [{ "Goto": null }] }"#;
    let error = serde_json::from_str::<Script>(document).unwrap_err();
    assert!(error.to_string().contains("twice"), "{error}");
}

#[test]
fn a_hand_written_library_runs() {
    let library: Script = serde_json::from_str(HAND_WRITTEN).unwrap();

    let play = |held: &BTreeSet<Key>| {
        let mut runner = Runner::default();
        runner.start("hub").unwrap();
        let mut said = Vec::new();
        let mut status = runner.advance(&library, held);
        while let Status::Act(action) = status {
            let answer = match action {
                Action::Say { line, .. } => {
                    said.push(line.clone());
                    Answer::Done
                }
                // The player asks for the hint, which returns to the hub.
                Action::Choose(replies) => Answer::Call(replies[1].1.clone()),
            };
            status = runner.resume(answer, &library, held);
        }
        assert_eq!(status, Status::Finished);
        said
    };

    assert_eq!(
        play(&BTreeSet::new()),
        ["Have you found my ring?", "Near the old well."]
    );
    assert_eq!(
        play(&BTreeSet::from([Key::HasRing])),
        ["Have you found my ring?", "You have my thanks."]
    );
}
