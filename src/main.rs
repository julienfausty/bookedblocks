use tokio;
use tokio::sync::mpsc::{Receiver, Sender, channel};

use std::collections::HashMap;

mod actions;
use actions::Action;

mod feed;
use feed::{Feed, TickerState};

mod pipeline;
use pipeline::BookHistory;

struct BooksCache {
    time_cache_window_seconds: usize,
    cache: HashMap<String, BookHistory>,
}

impl BooksCache {
    pub fn new(time_cache_window_seconds: usize) -> BooksCache {
        BooksCache {
            time_cache_window_seconds,
            cache: HashMap::new(),
        }
    }
}

struct Dispatch {
    action_receiver: Receiver<Action>,
    action_sender: Sender<Action>,
    feed: Feed,
    tickers: HashMap<String, Option<TickerState>>,
    books: BooksCache,
}

impl Dispatch {
    pub async fn new(
        buffer_size: usize,
        websocket_timeout_seconds: u64,
        book_depth: i32,
        time_cache_window_seconds: usize,
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
            tickers: HashMap::new(),
            books: BooksCache::new(time_cache_window_seconds),
        })
    }

    pub async fn run(&mut self) -> Result<(), String> {
        while let Some(action) = self.action_receiver.recv().await {
            match action {
                Action::Inform(message) => println!("{}", message),
                Action::Launch => println!("Got launch action"),
                Action::SubscribeTicker(ticker) => {
                    self.tickers.insert(ticker.clone(), None);
                    self.books.cache.insert(
                        ticker.clone(),
                        BookHistory::new(self.books.time_cache_window_seconds.clone()),
                    );

                    match self.feed.subscribe(ticker).await {
                        Ok(()) => (),
                        Err(message) => {
                            match self.action_sender.send(Action::Warn(message)).await {
                                Ok(_) => (),
                                Err(message) => return Err(format!("{:?}", message)),
                            }
                        }
                    }
                }
                Action::UnsubscribeTicker(ticker) => {
                    match self.feed.unsubscribe(ticker.clone()).await {
                        Ok(()) => (),
                        Err(message) => {
                            match self.action_sender.send(Action::Warn(message)).await {
                                Ok(_) => (),
                                Err(message) => return Err(format!("{:?}", message)),
                            }
                        }
                    }

                    self.tickers.remove(&ticker);
                    self.books.cache.remove(&ticker);
                }
                Action::Quit => println!("Got quit action"),
                Action::UpdateBook(update) => {
                    let symbol = update.symbol.clone();
                    match self.books.cache.get_mut(&symbol) {
                        Some(history) => {
                            history.update(update).await?;
                        }
                        None => {
                            return Err(format!(
                                "Got book update for {} while symbol was absent from cache.",
                                symbol
                            ));
                        }
                    }
                }
                Action::UpdateTicker(update) => {
                    let symbol = update.symbol.clone();
                    match self.tickers.insert(symbol.clone(), Some(update)) {
                        Some(_) => (),
                        None => {
                            return Err(format!(
                                "Got ticker update for {} while symbol was absent from cache.",
                                symbol
                            ));
                        }
                    }
                }
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
    let mut dispatch = match Dispatch::new(100, 200, 100, 60 * 60).await {
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
