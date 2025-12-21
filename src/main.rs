use clap::Parser;

use tokio;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::task::{JoinHandle, spawn};

use std::collections::HashMap;
use std::sync::Arc;

mod actions;
use actions::Action;

mod app;
use app::{App, State};

mod feed;
use feed::{Feed, TickerState};

mod pipeline;
use pipeline::{BookHistory, Pipeline};

mod splat;

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
    pipeline: Pipeline,
    app: App,
}

impl Dispatch {
    pub async fn new(
        buffer_size: usize,
        websocket_timeout_seconds: u64,
        book_depth: i32,
        time_cache_window_seconds: usize,
        time_visual_window_seconds: u64,
        time_resolution: usize,
        price_resolution: usize,
    ) -> Result<Dispatch, String> {
        let (sender, receiver) = channel::<Action>(buffer_size);

        let feed = match Feed::new(websocket_timeout_seconds, book_depth, sender.clone()).await {
            Ok(feed) => feed,
            Err(message) => return Err(message),
        };

        Ok(Dispatch {
            action_receiver: receiver,
            action_sender: sender.clone(),
            feed,
            tickers: HashMap::new(),
            books: BooksCache::new(time_cache_window_seconds),
            pipeline: Pipeline::new(
                time_visual_window_seconds,
                time_resolution,
                price_resolution,
            ),
            app: App::new(sender.clone()).await,
        })
    }

    async fn spawn_pipeline(
        history: BookHistory,
        pipeline: Pipeline,
        state: Arc<Mutex<State>>,
    ) -> JoinHandle<()> {
        spawn(async move {
            let buffer = pipeline.run(&history).await;
            let mut locked_state = state.lock().await;
            locked_state.depth = Some(buffer.0);
            locked_state.volumes = Some(buffer.1);
            locked_state.blocks = Some(buffer.2);
        })
    }

    pub async fn run(&mut self) -> Result<(), String> {
        while let Some(action) = self.action_receiver.recv().await {
            match action {
                Action::Inform(message) => (), // TODO: setup logs
                Action::SubscribeTicker(ticker) => {
                    self.tickers.insert(ticker.clone(), None);
                    self.books.cache.insert(
                        ticker.clone(),
                        BookHistory::new(self.books.time_cache_window_seconds.clone()),
                    );
                    self.app.set_current_ticker(ticker.clone()).await;

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
                Action::RunPipeline(ticker) => match self.books.cache.get(&ticker) {
                    Some(history) => {
                        let cloned_history = history.extract_window(0, i64::MAX).await;
                        Dispatch::spawn_pipeline(
                            cloned_history,
                            self.pipeline.clone(),
                            self.app.get_state(),
                        )
                        .await;
                    }
                    None => (),
                },
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
                Action::Quit => break,
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
                    match self.tickers.insert(symbol.clone(), Some(update.clone())) {
                        Some(_) => (),
                        None => {
                            return Err(format!(
                                "Got ticker update for {} while symbol was absent from cache.",
                                symbol
                            ));
                        }
                    }

                    self.app.get_state().lock().await.ticker_data = Some(update);
                }
                Action::Warn(message) => (), // TODO: setup warnings
            }
        }
        Ok(())
    }

    pub fn sender(&self) -> Sender<Action> {
        self.action_sender.clone()
    }
}

/// Visualizer of Kraken order books
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(required = true)]
    ticker: String,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let args = Args::parse();

    let mut dispatch = match Dispatch::new(1000, 200, 100, 5 * 60, 3 * 60, 370, 200).await {
        Ok(dispatch) => dispatch,
        Err(message) => return Err(message),
    };

    let sender = dispatch.sender();

    let running = dispatch.run();

    match sender.send(Action::SubscribeTicker(args.ticker)).await {
        Ok(_) => (),
        Err(message) => return Err(format!("{:?}", message)),
    };

    running.await
}
