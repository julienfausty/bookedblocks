use tokio;
use tokio::sync::mpsc::{Receiver, channel};

enum Action {
    Launch,
    Quit,
}

struct Dispatch {
    action_receiver: Receiver<Action>,
}

impl Dispatch {
    pub fn new(action_receiver: Receiver<Action>) -> Dispatch {
        Dispatch { action_receiver }
    }

    pub async fn run(&mut self) -> Result<(), String> {
        while let Some(action) = self.action_receiver.recv().await {
            match action {
                Action::Launch => println!("Got launch action"),
                Action::Quit => println!("Got quit action"),
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let (sender, receiver) = channel::<Action>(100);

    let mut dispatch = Dispatch::new(receiver);
    let running = dispatch.run();

    match sender.send(Action::Launch).await {
        Ok(_) => (),
        Err(message) => return Err(format!("{:?}", message)),
    };

    match sender.send(Action::Quit).await {
        Ok(_) => (),
        Err(message) => return Err(format!("{:?}", message)),
    };

    running.await
}
