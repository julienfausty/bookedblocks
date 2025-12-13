use tokio;
use tokio::sync::mpsc::{Receiver, Sender, channel};

mod actions;
use actions::Action;

mod feed;
use feed::Feed;

struct Dispatch {
    action_receiver: Receiver<Action>,
    action_sender: Sender<Action>,
    feed: Feed,
}

impl Dispatch {
    pub async fn new(
        buffer_size: usize,
        websocket_timeout_seconds: u64,
        book_depth: i32,
    ) -> Result<Dispatch, String> {
        let (sender, receiver) = channel::<Action>(buffer_size);

        let feed = match Feed::new(websocket_timeout_seconds, book_depth, sender.clone()).await {
            Ok(feed) => feed,
            Err(message) => return Err(message),
        };

        Ok(Dispatch {
            action_receiver: receiver,
            action_sender: sender,
            feed,
        })
    }

    pub async fn run(&mut self) -> Result<(), String> {
        while let Some(action) = self.action_receiver.recv().await {
            match action {
                Action::Inform(message) => println!("{}", message),
                Action::Launch => println!("Got launch action"),
                Action::SubscribeTicker(ticker) => match self.feed.subscribe(ticker).await {
                    Ok(()) => (),
                    Err(message) => match self.action_sender.send(Action::Warn(message)).await {
                        Ok(_) => (),
                        Err(message) => return Err(format!("{:?}", message)),
                    },
                },
                Action::UnsubscribeTicker(ticker) => match self.feed.unsubscribe(ticker).await {
                    Ok(()) => (),
                    Err(message) => match self.action_sender.send(Action::Warn(message)).await {
                        Ok(_) => (),
                        Err(message) => return Err(format!("{:?}", message)),
                    },
                },
                Action::Quit => println!("Got quit action"),
                Action::UpdateBook(update) => println!("{:?}", update),
                Action::UpdateTicker(update) => println!("{:?}", update),
                Action::Warn(message) => eprintln!("{}", message),
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
    let mut dispatch = match Dispatch::new(100, 200, 100).await {
        Ok(dispatch) => dispatch,
        Err(message) => return Err(message),
    };

    let sender = dispatch.sender();

    let running = dispatch.run();

    match sender.send(Action::Launch).await {
        Ok(_) => (),
        Err(message) => return Err(format!("{:?}", message)),
    };

    match sender
        .send(Action::SubscribeTicker("ETH/EUR".to_string()))
        .await
    {
        Ok(_) => (),
        Err(message) => return Err(format!("{:?}", message)),
    };

    match sender.send(Action::Quit).await {
        Ok(_) => (),
        Err(message) => return Err(format!("{:?}", message)),
    };

    running.await
}
