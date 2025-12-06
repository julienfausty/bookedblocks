use crate::actions::Action;

use kraken_async_rs::wss::{BidAsk, L2, Ticker};
use kraken_async_rs::wss::{
    BookSubscription, KrakenMessageStream, KrakenWSSClient, TickerSubscription, WS_KRAKEN,
    WS_KRAKEN_AUTH,
};
use kraken_async_rs::wss::{ChannelMessage, Message, WssMessage};

use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;
use tokio::task::{JoinHandle, spawn};
use tokio::time::{Duration, sleep, timeout};
use tokio_stream::StreamExt;

use num_traits::cast::ToPrimitive;

use chrono::Utc;

use std::sync::Arc;

macro_rules! decimal_to_f64 {
    ($value:expr) => {
        match $value.to_f64() {
            Some(val) => val,
            None => {
                return Err(format!("Failed to convert {:?} to f64", $value));
            }
        }
    };
}

#[derive(Debug)]
pub struct TickerState {
    pub ask: f64,
    pub ask_quantity: f64,
    pub bid: f64,
    pub bid_quantity: f64,
    pub change: f64,
    pub change_pct: f64,
    pub high: f64,
    pub last: f64,
    pub low: f64,
    pub symbol: String,
    pub volume: f64,
    pub vwap: f64,
}

impl TickerState {
    pub fn from_ticker(ticker: Ticker) -> Result<TickerState, String> {
        Ok(TickerState {
            ask: decimal_to_f64!(ticker.ask),
            ask_quantity: decimal_to_f64!(ticker.ask_quantity),
            bid: decimal_to_f64!(ticker.bid),
            bid_quantity: decimal_to_f64!(ticker.bid_quantity),
            change: decimal_to_f64!(ticker.change),
            change_pct: decimal_to_f64!(ticker.change_pct),
            high: decimal_to_f64!(ticker.change),
            last: decimal_to_f64!(ticker.last),
            low: decimal_to_f64!(ticker.low),
            symbol: ticker.symbol,
            volume: decimal_to_f64!(ticker.volume),
            vwap: decimal_to_f64!(ticker.vwap),
        })
    }
}

#[derive(Debug)]
pub struct Order {
    pub price: f64,
    pub quantity: f64,
}

impl Order {
    pub fn from_bid_ask(bid_ask: BidAsk) -> Result<Order, String> {
        Ok(Order {
            price: decimal_to_f64!(bid_ask.price),
            quantity: decimal_to_f64!(bid_ask.quantity),
        })
    }
}

#[derive(Debug)]
pub struct Booked {
    pub symbol: String,
    pub timestamp: String,
    pub bids: Vec<Order>,
    pub asks: Vec<Order>,
}

impl Booked {
    pub fn from_orderbook(book: L2) -> Result<Booked, String> {
        match book {
            L2::Orderbook(snapshot) => Ok(Booked {
                symbol: snapshot.symbol,
                timestamp: Utc::now().to_rfc3339(),
                bids: match snapshot
                    .bids
                    .into_iter()
                    .map(|bid_ask| Order::from_bid_ask(bid_ask))
                    .collect()
                {
                    Ok(bids) => bids,
                    Err(message) => return Err(message),
                },
                asks: match snapshot
                    .asks
                    .into_iter()
                    .map(|bid_ask| Order::from_bid_ask(bid_ask))
                    .collect()
                {
                    Ok(asks) => asks,
                    Err(message) => return Err(message),
                },
            }),
            L2::Update(update) => Ok(Booked {
                symbol: update.symbol,
                timestamp: update.timestamp,
                bids: match update
                    .bids
                    .into_iter()
                    .map(|bid_ask| Order::from_bid_ask(bid_ask))
                    .collect()
                {
                    Ok(bids) => bids,
                    Err(message) => return Err(message),
                },
                asks: match update
                    .asks
                    .into_iter()
                    .map(|bid_ask| Order::from_bid_ask(bid_ask))
                    .collect()
                {
                    Ok(asks) => asks,
                    Err(message) => return Err(message),
                },
            }),
        }
    }
}

pub struct Feed {
    // websocket connection to Kraken WS API
    connection: Arc<Mutex<KrakenMessageStream<WssMessage>>>,
    // the depth to request the book data
    depth: i32,
    // handle to websocket listener
    listener_handle: JoinHandle<Result<(), String>>,
    // request id counter
    request_id: i64,
}

async fn listen_to_connection(
    sender: Sender<Action>,
    connection: Arc<Mutex<KrakenMessageStream<WssMessage>>>,
    timeout_in_seconds: u64,
) -> Result<(), String> {
    loop {
        loop {
            let mut stream = connection.lock().await;
            match timeout(Duration::from_secs(timeout_in_seconds), stream.next()).await {
                Ok(Some(communication)) => {
                    let mut action = Action::Inform("".to_string());
                    match communication {
                        Ok(WssMessage::Channel(message)) => match message {
                            ChannelMessage::Heartbeat => break,
                            ChannelMessage::Orderbook(booked) => {
                                action =
                                    Action::UpdateBook(match Booked::from_orderbook(booked.data) {
                                        Ok(casted) => casted,
                                        Err(message) => return Err(message),
                                    })
                            }
                            ChannelMessage::Ticker(tick) => {
                                action = Action::UpdateTicker(
                                    match TickerState::from_ticker(tick.data) {
                                        Ok(casted) => casted,
                                        Err(message) => return Err(message),
                                    },
                                )
                            }
                            _ => action = Action::Inform(format!("{:?}", message)),
                        },
                        Ok(WssMessage::Method(information)) => {
                            action = Action::Inform(format!("{:?}", information));
                        }
                        Ok(WssMessage::Error(err)) => {
                            action = Action::WarningMessage(format!("{:?}", err));
                        }
                        Err(err) => {
                            action = Action::WarningMessage(format!("{:?}", err));
                        }
                    }
                    match sender.send(action).await {
                        Ok(_) => (),
                        Err(send_err) => return Err(format!("{:?}", send_err)),
                    };
                }
                Ok(None) => return Ok(()),
                Err(message) => return Err(format!("{:?}", message)),
            }
        }

        // allow some time for subscriptions on the connection
        sleep(Duration::from_millis(200)).await;
    }
}

impl Feed {
    pub async fn new(
        timeout_in_seconds: u64,
        depth: i32,
        sender: Sender<Action>,
    ) -> Result<Feed, String> {
        let mut client = KrakenWSSClient::new_with_urls(WS_KRAKEN, WS_KRAKEN_AUTH);
        let connection = match client.connect::<WssMessage>().await {
            Ok(connection) => Arc::new(Mutex::new(connection)),
            Err(message) => return Err(format!("{:?}", message)),
        };

        let cloned_connection = connection.clone();
        let listener_handle = spawn(async move {
            listen_to_connection(sender, cloned_connection, timeout_in_seconds).await
        });

        Ok(Feed {
            connection,
            depth,
            listener_handle,
            request_id: 0,
        })
    }

    pub async fn subscribe(&mut self, ticker: String) -> Result<(), String> {
        let mut book_subscription = BookSubscription::new(vec![ticker.clone()]);
        book_subscription.snapshot = Some(true);
        book_subscription.depth = Some(self.depth);

        let book_subscription_message =
            Message::new_subscription(book_subscription, self.request_id.clone());
        self.request_id += 1;

        let ticker_subscription = TickerSubscription::new(vec![ticker.clone()]);
        let ticker_subscription_message =
            Message::new_subscription(ticker_subscription, self.request_id.clone());
        self.request_id += 1;

        let mut writable = self.connection.lock().await;

        match writable.send(&ticker_subscription_message).await {
            Ok(_) => (),
            Err(message) => return Err(format!("{:?}", message)),
        };

        match writable.send(&book_subscription_message).await {
            Ok(_) => Ok(()),
            Err(message) => Err(format!("{:?}", message)),
        }
    }

    pub async fn check_listener(&self) -> Result<(), String> {
        Ok(())
    }
}
