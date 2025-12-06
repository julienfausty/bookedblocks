use tokio;
use tokio::sync::mpsc::{Receiver, Sender, channel};

mod actions;
use actions::Action;

mod feed;

struct Dispatch {
    action_receiver: Receiver<Action>,
    action_sender: Sender<Action>,
}

impl Dispatch {
    pub fn new(buffer_size: usize) -> Dispatch {
        let (sender, receiver) = channel::<Action>(buffer_size);
        Dispatch {
            action_receiver: receiver,
            action_sender: sender,
        }
    }

    pub async fn run(&mut self) -> Result<(), String> {
        while let Some(action) = self.action_receiver.recv().await {
            match action {
                Action::Launch => println!("Got launch action"),
                Action::SubscribeTicker(ticker) => (),
                Action::Quit => println!("Got quit action"),
            }
        }
        Ok(())
    }

    pub fn sender(&self) -> Sender<Action> {
        self.action_sender.clone()
    }
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut dispatch = Dispatch::new(100);

    let sender = dispatch.sender();

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
