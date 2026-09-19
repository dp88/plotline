//! Rules survive a trip through a data file.
#![cfg(feature = "serde")]

use plotline::{CapabilitySet, Evaluation, Requirement, Unlock, Unlocks};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
enum Tech {
    Fusion,
    Warp,
    Cloaking,
}

fn cruiser() -> Requirement<Tech> {
    Requirement::all([
        Requirement::has(Tech::Fusion),
        Requirement::any([
            Requirement::has(Tech::Warp),
            Requirement::not(Requirement::has(Tech::Cloaking)),
        ]),
        Requirement::at_least(
            1,
            [
                Requirement::has(Tech::Warp),
                Requirement::has(Tech::Cloaking),
            ],
        ),
    ])
}

#[test]
fn a_nested_requirement_round_trips() {
    let encoded = serde_json::to_string(&cruiser()).unwrap();
    let decoded: Requirement<Tech> = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, cruiser());
}

#[test]
fn a_decoded_requirement_answers_the_same_way() {
    let encoded = serde_json::to_string(&cruiser()).unwrap();
    let decoded: Requirement<Tech> = serde_json::from_str(&encoded).unwrap();

    for held in [
        CapabilitySet::from([]),
        CapabilitySet::from([Tech::Fusion]),
        CapabilitySet::from([Tech::Fusion, Tech::Warp]),
        CapabilitySet::from([Tech::Fusion, Tech::Warp, Tech::Cloaking]),
    ] {
        assert_eq!(cruiser().satisfies(&held), decoded.satisfies(&held));
    }
}

#[test]
fn a_hand_written_document_loads() {
    // The shape an author writes by hand, in JSON, RON, or YAML.
    let document = r#"
    {
      "All": [
        { "Has": "Fusion" },
        { "Any": [{ "Has": "Warp" }, { "Has": "Cloaking" }] },
        { "Not": { "Has": "Warp" } },
        { "AtLeast": { "count": 1, "requirements": [{ "Has": "Cloaking" }] } }
      ]
    }
    "#;

    let rule: Requirement<Tech> = serde_json::from_str(document).unwrap();
    assert_eq!(
        rule,
        Requirement::all([
            Requirement::has(Tech::Fusion),
            Requirement::any([
                Requirement::has(Tech::Warp),
                Requirement::has(Tech::Cloaking),
            ]),
            Requirement::not(Requirement::has(Tech::Warp)),
            Requirement::at_least(1, [Requirement::has(Tech::Cloaking)]),
        ])
    );

    assert!(rule.satisfies(&CapabilitySet::from([Tech::Fusion, Tech::Cloaking])));
    assert!(!rule.satisfies(&CapabilitySet::from([
        Tech::Fusion,
        Tech::Warp,
        Tech::Cloaking
    ])));
}

#[test]
fn string_keys_need_no_rust_vocabulary() {
    let document = r#"{ "All": [{ "Has": "survive-frozen" }, { "Has": "survive-toxic" }] }"#;
    let rule: Requirement<String> = serde_json::from_str(document).unwrap();

    let held: CapabilitySet<String> = ["survive-frozen".to_owned()].into_iter().collect();
    assert_eq!(
        rule.evaluate(&held).missing(),
        vec!["survive-toxic".to_owned()]
    );
}

#[test]
fn a_capability_set_round_trips() {
    let held = CapabilitySet::from([Tech::Fusion, Tech::Warp]);
    let encoded = serde_json::to_string(&held).unwrap();
    assert_eq!(
        encoded, r#"["Fusion","Warp"]"#,
        "a set encodes as a plain list"
    );

    let decoded: CapabilitySet<Tech> = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, held);
}

#[test]
fn an_evaluation_round_trips() {
    let result = cruiser().evaluate(&CapabilitySet::from([Tech::Fusion]));
    let encoded = serde_json::to_string(&result).unwrap();
    let decoded: Evaluation<Tech> = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded, result);
    assert_eq!(decoded.satisfied(), result.satisfied());
}

#[test]
fn a_whole_tree_round_trips() {
    let mut tree = Unlocks::new();
    tree.insert("fusion-power", Unlock::free().granting([Tech::Fusion]));
    tree.insert(
        "warp-drive",
        Unlock::new(Requirement::has(Tech::Fusion)).granting([Tech::Warp]),
    );
    tree.insert(
        "cloaking-field",
        Unlock::new(Requirement::any([
            Requirement::has(Tech::Warp),
            Requirement::has(Tech::Fusion),
        ]))
        .granting([Tech::Cloaking]),
    );

    let encoded = serde_json::to_string(&tree).unwrap();
    let decoded: Unlocks<String, Tech> = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded.len(), 3);
    assert_eq!(
        decoded.dependencies(&"warp-drive".to_owned()),
        vec![&"fusion-power".to_owned()]
    );
    assert_eq!(decoded.rank(&"warp-drive".to_owned()), Some(1));
    assert!(decoded.validate(&CapabilitySet::new()).is_empty());
}

#[test]
fn a_hand_written_tree_loads() {
    let document = r#"
    {
      "fusion-power": { "requires": { "All": [] }, "grants": ["Fusion"] },
      "warp-drive": { "requires": { "Has": "Fusion" }, "grants": ["Warp"] }
    }
    "#;

    let tree: Unlocks<String, Tech> = serde_json::from_str(document).unwrap();
    let mut held = CapabilitySet::new();

    assert!(tree.take(&"fusion-power".to_owned(), &mut held));
    assert!(tree.take(&"warp-drive".to_owned(), &mut held));
    assert_eq!(held, CapabilitySet::from([Tech::Fusion, Tech::Warp]));
}
