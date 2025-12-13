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
            high: decimal_to_f64!(ticker.high),
            last: decimal_to_f64!(ticker.last),
            low: decimal_to_f64!(ticker.low),
            symbol: ticker.symbol,
            volume: decimal_to_f64!(ticker.volume),
            vwap: decimal_to_f64!(ticker.vwap),
        })
    }
}

#[derive(Debug, PartialEq)]
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
                bids: snapshot
                    .bids
                    .into_iter()
                    .map(Order::from_bid_ask)
                    .collect::<Result<Vec<_>, String>>()?,
                asks: snapshot
                    .asks
                    .into_iter()
                    .map(Order::from_bid_ask)
                    .collect::<Result<Vec<_>, String>>()?,
            }),
            L2::Update(update) => Ok(Booked {
                symbol: update.symbol,
                timestamp: update.timestamp,
                bids: update
                    .bids
                    .into_iter()
                    .map(Order::from_bid_ask)
                    .collect::<Result<Vec<_>, String>>()?,
                asks: update
                    .asks
                    .into_iter()
                    .map(Order::from_bid_ask)
                    .collect::<Result<Vec<_>, String>>()?,
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
                    let action: Action;
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
                            action = Action::Warn(format!("{:?}", err));
                        }
                        Err(err) => {
                            action = Action::Warn(format!("{:?}", err));
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
            Message::new_subscription(book_subscription, self.request_id);
        self.request_id += 1;

        let ticker_subscription = TickerSubscription::new(vec![ticker.clone()]);
        let ticker_subscription_message =
            Message::new_subscription(ticker_subscription, self.request_id);
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

    pub async fn unsubscribe(&mut self, ticker: String) -> Result<(), String> {
        let mut book_subscription = BookSubscription::new(vec![ticker.clone()]);
        book_subscription.depth = Some(self.depth);

        let mut book_subscription_message =
            Message::new_subscription(book_subscription, self.request_id);
        self.request_id += 1;
        book_subscription_message.method = "unsubscribe".to_string();

        let ticker_subscription = TickerSubscription::new(vec![ticker.clone()]);
        let mut ticker_subscription_message =
            Message::new_subscription(ticker_subscription, self.request_id);
        self.request_id += 1;
        ticker_subscription_message.method = "unsubscribe".to_string();

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

    pub async fn check_listener(self) -> Result<Option<Feed>, String> {
        if self.listener_handle.is_finished() {
            return match self.listener_handle.await {
                Ok(val) => match val {
                    Ok(()) => Ok(None),
                    Err(message) => Err(message),
                },
                Err(message) => Err(format!("{:?}", message)),
            };
        }

        Ok(Some(Feed {
            connection: self.connection.clone(),
            depth: self.depth,
            request_id: self.request_id,
            listener_handle: self.listener_handle,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use kraken_async_rs::wss::{BidAsk, L2, Orderbook, OrderbookUpdate, Ticker};

    use tokio::sync::mpsc::channel;
    use tokio::time::{Duration, timeout};

    use regex::Regex;
    use rust_decimal::Decimal;

    fn zero_ticker_case() -> Ticker {
        Ticker {
            ask: Decimal::ZERO,
            ask_quantity: Decimal::ZERO,
            bid: Decimal::ZERO,
            bid_quantity: Decimal::ZERO,
            change: Decimal::ZERO,
            change_pct: Decimal::ZERO,
            high: Decimal::ZERO,
            last: Decimal::ZERO,
            low: Decimal::ZERO,
            symbol: "Ticker/Symbol".to_string(),
            volume: Decimal::ZERO,
            vwap: Decimal::ZERO,
        }
    }

    fn zero_bid_ask_case() -> BidAsk {
        BidAsk {
            price: Decimal::ZERO,
            quantity: Decimal::ZERO,
        }
    }

    fn zero_orderbook_case() -> L2 {
        L2::Orderbook(Orderbook {
            symbol: "Ticker/Symbol".to_string(),
            checksum: 0,
            bids: (0..10).map(|_| zero_bid_ask_case()).collect(),
            asks: (0..10).map(|_| zero_bid_ask_case()).collect(),
        })
    }

    fn ascending_orderbook_case() -> L2 {
        L2::Orderbook(Orderbook {
            symbol: "Ticker/Symbol".to_string(),
            checksum: 0,
            bids: (0..10)
                .map(|i| BidAsk {
                    price: Decimal::new(i, 0),
                    quantity: Decimal::ZERO,
                })
                .collect(),
            asks: (0..10)
                .map(|i| BidAsk {
                    price: Decimal::new(-i, 0),
                    quantity: Decimal::ZERO,
                })
                .collect(),
        })
    }

    fn ascending_orderbook_update_case() -> L2 {
        L2::Update(OrderbookUpdate {
            symbol: "Ticker/Symbol".to_string(),
            checksum: 0,
            timestamp: "Mocked Timestamp".to_string(),
            bids: (0..10)
                .map(|i| BidAsk {
                    price: Decimal::new(i, 0),
                    quantity: Decimal::ZERO,
                })
                .collect(),
            asks: (0..10)
                .map(|i| BidAsk {
                    price: Decimal::new(-i, 0),
                    quantity: Decimal::ZERO,
                })
                .collect(),
        })
    }

    #[tokio::test]
    async fn test_zero_ticker_transfer() {
        let ticker = zero_ticker_case();
        let outcome = TickerState::from_ticker(ticker);

        assert!(outcome.is_ok());

        let state = outcome.unwrap();
        assert!(state.ask == 0.0);
        assert!(state.ask_quantity == 0.0);
        assert!(state.bid == 0.0);
        assert!(state.bid_quantity == 0.0);
        assert!(state.change == 0.0);
        assert!(state.change_pct == 0.0);
        assert!(state.high == 0.0);
        assert!(state.last == 0.0);
        assert!(state.low == 0.0);
        assert!(state.symbol == "Ticker/Symbol".to_string());
        assert!(state.volume == 0.0);
        assert!(state.vwap == 0.0);
    }

    #[tokio::test]
    async fn test_values_ticker_transfer() {
        let mut ticker = zero_ticker_case();
        ticker.volume = Decimal::ONE;
        ticker.vwap = Decimal::ONE_THOUSAND;
        ticker.change = Decimal::ONE_HUNDRED;
        let outcome = TickerState::from_ticker(ticker);

        assert!(outcome.is_ok());

        let state = outcome.unwrap();
        assert!(state.ask == 0.0);
        assert!(state.ask_quantity == 0.0);
        assert!(state.bid == 0.0);
        assert!(state.bid_quantity == 0.0);
        assert!(state.change == 100.0);
        assert!(state.change_pct == 0.0);
        assert!(state.high == 0.0);
        assert!(state.last == 0.0);
        assert!(state.low == 0.0);
        assert!(state.symbol == "Ticker/Symbol".to_string());
        assert!(state.volume == 1.0);
        assert!(state.vwap == 1000.0);
    }

    #[tokio::test]
    async fn test_zero_bid_ask_transfer() {
        let bid_ask = zero_bid_ask_case();

        let outcome = Order::from_bid_ask(bid_ask);

        assert!(outcome.is_ok());

        let order = outcome.unwrap();
        assert!(order.price == 0.0);
        assert!(order.quantity == 0.0);
    }

    #[tokio::test]
    async fn test_values_bid_ask_transfer() {
        let mut bid_ask = zero_bid_ask_case();
        bid_ask.price = Decimal::ONE;
        bid_ask.quantity = Decimal::ONE_HUNDRED;

        let outcome = Order::from_bid_ask(bid_ask);

        assert!(outcome.is_ok());

        let order = outcome.unwrap();
        assert!(order.price == 1.0);
        assert!(order.quantity == 100.0);
    }

    #[tokio::test]
    async fn test_booked_zeros_transfer() {
        let l2 = zero_orderbook_case();

        let outcome = Booked::from_orderbook(l2);

        assert!(outcome.is_ok());

        let booked = outcome.unwrap();

        assert!(booked.symbol == "Ticker/Symbol".to_string());

        for order in booked.bids {
            assert!(order.price == 0.0);
            assert!(order.quantity == 0.0);
        }

        for order in booked.asks {
            assert!(order.price == 0.0);
            assert!(order.quantity == 0.0);
        }
    }

    #[tokio::test]
    async fn test_booked_ascending_transfer() {
        let l2 = ascending_orderbook_case();

        let outcome = Booked::from_orderbook(l2);

        assert!(outcome.is_ok());

        let booked = outcome.unwrap();

        assert!(booked.symbol == "Ticker/Symbol".to_string());

        for i in 0..10 {
            assert!(booked.bids[i].price == i as f64);
            assert!(booked.bids[i].quantity == 0.0);
        }

        for i in 0..10 {
            assert!(booked.asks[i].price == -(i as f64));
            assert!(booked.asks[i].quantity == 0.0);
        }
    }

    #[tokio::test]
    async fn test_booked_update_ascending_transfer() {
        let l2 = ascending_orderbook_update_case();

        let outcome = Booked::from_orderbook(l2);

        assert!(outcome.is_ok());

        let booked = outcome.unwrap();

        assert!(booked.symbol == "Ticker/Symbol".to_string());
        assert!(booked.timestamp == "Mocked Timestamp".to_string());

        for i in 0..10 {
            assert!(booked.bids[i].price == i as f64);
            assert!(booked.bids[i].quantity == 0.0);
        }

        for i in 0..10 {
            assert!(booked.asks[i].price == -(i as f64));
            assert!(booked.asks[i].quantity == 0.0);
        }
    }

    #[tokio::test]
    async fn construct_feed() {
        let (sender, mut receiver) = channel::<Action>(10);
        let outcome = Feed::new(2, 10, sender).await;

        assert!(outcome.is_ok());

        let feed = outcome.unwrap();

        let mut number_of_actions = 0;
        while let Some(_) = receiver.recv().await {
            number_of_actions += 1;
        }

        assert!(number_of_actions == 1);
        assert!(feed.check_listener().await.is_err());
    }

    #[tokio::test]
    async fn feed_10_actions() {
        let (sender, mut receiver) = channel::<Action>(10);
        let outcome = Feed::new(20, 10, sender).await;

        assert!(outcome.is_ok());

        let mut feed = outcome.unwrap();

        assert!(feed.subscribe("ETH/EUR".to_string()).await.is_ok());

        for _ in 0..10 {
            let maybe_action = timeout(Duration::from_secs(5), receiver.recv())
                .await
                .unwrap();
            assert!(maybe_action.is_some());
        }

        assert!(feed.check_listener().await.is_ok());
    }

    #[tokio::test]
    async fn feed_subscribe_wrong_ticker() {
        let (sender, mut receiver) = channel::<Action>(10);
        let outcome = Feed::new(5, 10, sender).await;

        assert!(outcome.is_ok());

        let mut feed = outcome.unwrap();

        assert!(feed.subscribe("Non/Existent".to_string()).await.is_ok());

        let mut output = String::new();
        while let Some(action) = receiver.recv().await {
            output = format!("{}{:?}", output, action);
        }

        let pattern =
            Regex::new(r##"Some\(\\\"Currency pair not supported Non\/Existent\\\"\)"##).unwrap();
        assert!(pattern.is_match(&output));
    }

    #[tokio::test]
    async fn feed_unsubscribe() {
        let (sender, mut receiver) = channel::<Action>(10);
        let outcome = Feed::new(2, 10, sender).await;

        assert!(outcome.is_ok());

        let mut feed = outcome.unwrap();

        assert!(feed.subscribe("ETH/EUR".to_string()).await.is_ok());
        assert!(feed.unsubscribe("ETH/EUR".to_string()).await.is_ok());

        let mut output = String::new();
        while let Ok(Some(action)) = timeout(Duration::from_secs(2), receiver.recv()).await {
            output = format!("{}{:?}", output, action);
        }

        let pattern = Regex::new(r##"Some\(Ticker\(TickerSubscriptionResponse \{ symbol: \\\"ETH\/EUR\\\", event_trigger: Some\(Trades\), snapshot: Some\(true\) \}\)\)"##).unwrap();
        assert!(pattern.is_match(&output));

        let pattern = Regex::new(r##"Some\(Book\(BookSubscriptionResponse \{ symbol: \\\"ETH\/EUR\\\", depth: Some\(10\), snapshot: Some\(true\), warnings: None \}\)\)"##).unwrap();
        assert!(pattern.is_match(&output));

        // cannot check for unsubscribe because the kraken_async_rs does not support the response

        // timeout does not get triggered thanks to heartbeat
        assert!(feed.check_listener().await.is_ok());
    }

    #[tokio::test]
    async fn feed_unsubscribe_not_previously_subscribed() {
        let (sender, mut receiver) = channel::<Action>(10);
        let outcome = Feed::new(2, 10, sender).await;

        assert!(outcome.is_ok());

        let mut feed = outcome.unwrap();

        assert!(feed.unsubscribe("ETH/EUR".to_string()).await.is_ok());

        let mut output = String::new();
        while let Ok(Some(action)) = timeout(Duration::from_secs(2), receiver.recv()).await {
            output = format!("{}{:?}", output, action);
        }

        let pattern = Regex::new(r##"Some\(\\\"Subscription Not Found\\\"\)"##).unwrap();
        assert!(pattern.is_match(&output));
    }
}
