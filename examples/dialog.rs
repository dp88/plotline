//! A branching conversation: the thing this crate exists for.
//!
//! Run it with `cargo run --example dialog`.
//!
//! Note what is *not* here. No clock, no `async`, no engine. The conversation holds at a
//! `Completion` that this file signals when the "player" answers; in a game that handle
//! would belong to a dialog panel, and nothing else would change.

use core::task::Poll;

use plotline::{
    Completion, Library, Progress, Runner, RunnerEvent, Sequence, TypeMap, conditions, steps,
};

/// Stands in for a game's inventory and input. Steps reach it through the service
/// registry, so this crate never learns what an item is.
#[derive(Default)]
struct World {
    items: Vec<String>,
    said_yes: bool,
}

fn main() {
    let mut library = Library::new();

    // Inserted empty so the branches below can name it, then filled in afterwards.
    // A conversation is a cycle, and this is how you build one without `Rc`.
    let hub = library.insert(Sequence::new("hub"));

    let accepted = library.insert(
        Sequence::new("accepted")
            .with_step(steps::run("Take the ring", |ctx| {
                match ctx.services_mut().get_mut::<World>() {
                    Some(world) => world.items.retain(|item| item != "gold ring"),
                    // The house rule for a missing requirement: say so, do nothing.
                    None => ctx.note("no World service; the ring stayed put"),
                }
            }))
            .with_step(say("Elder", "You have my thanks.")),
    );

    // "Come back when you have it" — a real cycle, back to the hub.
    let refused = library.insert(
        Sequence::new("refused")
            .with_step(say("Elder", "Come back when you have."))
            .with_step(
                steps::run("Return to the hub", move |_ctx| Progress::Goto(Some(hub)))
                    .ends()
                    .delegating_to(hub),
            ),
    );

    let answered = Completion::new();
    let waiting_on = answered.clone();
    let hub_body = library.get_mut(hub).unwrap();
    hub_body.push(say("Elder", "Have you found my ring?"));
    hub_body.push(steps::run("Wait for the player", move |_ctx| {
        waiting_on.clone()
    }));
    hub_body.push(steps::run("Record the answer", |ctx| {
        let said_yes = ctx.services().get::<World>().is_some_and(|w| w.said_yes);
        ctx.set_flag("said_yes", said_yes);
    }));
    hub_body.push(steps::Branch {
        condition: Some(Box::new(conditions::Flag::is_set("said_yes"))),
        if_true: Some(accepted),
        if_false: Some(refused),
    });

    let mut services = TypeMap::new();
    services.insert(World {
        items: vec!["gold ring".to_owned(), "lantern".to_owned()],
        said_yes: false,
    });

    let mut runner = Runner::default();
    runner.start(hub, None).unwrap();

    // The host's loop. In a game this body runs once per frame.
    loop {
        let poll = runner.advance(&mut library, &mut services);
        drain(&mut runner);

        match poll {
            Poll::Pending => {
                // Somewhere outside this crate, as always, the player answers.
                println!("> Yes");
                services.get_mut::<World>().unwrap().said_yes = true;
                answered.signal();
            }
            Poll::Ready(outcome) => {
                println!("--- {outcome:?} ---");
                break;
            }
        }
    }

    println!("inventory: {:?}", services.get::<World>().unwrap().items);
}

/// One line of dialogue. In a real game this would show a panel and wait for dismissal.
fn say(who: &'static str, line: &'static str) -> impl plotline::Step {
    steps::run(format!("{who}: \"{line}\""), move |_ctx| {
        println!("{who}: {line}");
    })
}

/// The three lines that replace a logging dependency.
fn drain(runner: &mut Runner) {
    for event in runner.drain_events() {
        if let RunnerEvent::Note { message, .. } = event {
            eprintln!("[note] {message}");
        }
    }
}
