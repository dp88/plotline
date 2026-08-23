//! A branching conversation.

use core::task::Poll;

use plotline::{
    Completion, Library, Progress, Runner, RunnerEvent, Sequence, TypeMap, conditions, steps,
};

/// Host state used by the example.
#[derive(Default)]
struct World {
    items: Vec<String>,
    said_yes: bool,
}

fn main() {
    let mut library = Library::new();

    // Insert first so later steps can refer to the cycle.
    let hub = library.insert(Sequence::new("hub"));

    let accepted = library.insert(
        Sequence::new("accepted")
            .with_step(steps::run("Take the ring", |ctx| {
                match ctx.services_mut().get_mut::<World>() {
                    Some(world) => world.items.retain(|item| item != "gold ring"),
                    None => ctx.note("no World service; the ring stayed put"),
                }
            }))
            .with_step(say("Elder", "You have my thanks.")),
    );

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

    loop {
        let poll = runner.advance(&mut library, &mut services);
        drain(&mut runner);

        match poll {
            Poll::Pending => {
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

/// Creates a dialogue step.
fn say(who: &'static str, line: &'static str) -> impl plotline::Step {
    steps::run(format!("{who}: \"{line}\""), move |_ctx| {
        println!("{who}: {line}");
    })
}

/// Prints runner notes.
fn drain(runner: &mut Runner) {
    for event in runner.drain_events() {
        if let RunnerEvent::Note { message, .. } = event {
            eprintln!("[note] {message}");
        }
    }
}
