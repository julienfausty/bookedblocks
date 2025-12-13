use crate::feed::{Booked, Order};

use tokio::sync::RwLock;

use chrono::DateTime;
use rbtree::RBTree;

use std::cmp::Ordering;

#[derive(Clone, PartialOrd, PartialEq)]
pub struct Price {
    pub value: f64,
}

impl Eq for Price {}

impl Ord for Price {
    fn cmp(&self, other: &Price) -> Ordering {
        f64::total_cmp(&self.value, &other.value)
    }
}

fn update_books(
    books: &mut RBTree<i64, RBTree<Price, f64>>,
    time_window: usize,
    incoming_time: i64,
    orders: Vec<Order>,
) -> Result<Option<(i64, RBTree<Price, f64>)>, String> {
    if books.is_empty() {
        books.insert(
            incoming_time,
            RBTree::from_iter(
                orders
                    .into_iter()
                    .map(|order| (Price { value: order.price }, order.quantity)),
            ),
        );

        Ok(None)
    } else {
        let start_time = match books.get_first() {
            Some((time, _)) => time.clone(),
            None => return Err("Could not find oldest ask in book history.".to_string()),
        };

        let mut latest = match books.get_last() {
            Some((_, latest)) => latest.clone(),
            None => return Err("Could not find latest ask in book history.".to_string()),
        };

        for order in orders.into_iter() {
            let _ = latest.replace_or_insert(Price { value: order.price }, order.quantity);
            latest = RBTree::from_iter(latest.into_iter().filter(|(_, value)| *value != 0.0));
        }

        books.insert(incoming_time.clone(), latest);

        if (incoming_time - start_time).abs() as usize > time_window {
            Ok(books.pop_first())
        } else {
            Ok(None)
        }
    }
}

pub struct BookHistory {
    pub time_window_in_seconds: usize,
    pub asks: RwLock<RBTree<i64, RBTree<Price, f64>>>,
    pub bids: RwLock<RBTree<i64, RBTree<Price, f64>>>,
}

impl BookHistory {
    pub fn new(time_window_in_seconds: usize) -> BookHistory {
        BookHistory {
            time_window_in_seconds,
            asks: RwLock::new(RBTree::new()),
            bids: RwLock::new(RBTree::new()),
        }
    }

    pub async fn update(
        &mut self,
        booked: Booked,
    ) -> Result<Option<((i64, RBTree<Price, f64>), (i64, RBTree<Price, f64>))>, String> {
        let incoming_time = match DateTime::parse_from_rfc3339(&booked.timestamp) {
            Ok(time) => time.timestamp(),
            Err(message) => return Err(format!("{:?}", message)),
        };

        let writable_asks = &mut self.asks.write().await;
        let writable_bids = &mut self.bids.write().await;

        match (
            update_books(
                writable_asks,
                self.time_window_in_seconds.clone(),
                incoming_time.clone(),
                booked.asks,
            ),
            update_books(
                writable_bids,
                self.time_window_in_seconds.clone(),
                incoming_time.clone(),
                booked.bids,
            ),
        ) {
            (Ok(Some(ret_asks)), Ok(Some(ret_bids))) => Ok(Some((ret_asks, ret_bids))),
            (Ok(Some(_)), Ok(None)) => {
                Err("Removed entry from asks during update but not bids.".to_string())
            }
            (Ok(None), Ok(Some(_))) => {
                Err("Removed entry from bids during update but not asks.".to_string())
            }
            (Ok(None), Ok(None)) => Ok(None),
            (Ok(_), Err(message)) => Err(message),
            (Err(message), Ok(_)) => Err(message),
            (Err(ask_message), Err(bid_message)) => Err(format!(
                "Ask update error: {}\nBid update error {}",
                ask_message, bid_message
            )),
        }
    }

    pub async fn get_lastest_book(&self) -> (RBTree<Price, f64>, RBTree<Price, f64>) {
        let readable_asks = self.asks.read().await;
        let readable_bids = self.bids.read().await;

        match (readable_asks.get_last(), readable_bids.get_last()) {
            (Some((_, asks)), Some((_, bids))) => (asks.clone(), bids.clone()),
            _ => (RBTree::new(), RBTree::new()),
        }
    }

    pub async fn integrate_window(
        &self,
        start: i64,
        end: i64,
    ) -> (RBTree<i64, f64>, RBTree<i64, f64>) {
        let integrate = |history: &RBTree<i64, RBTree<Price, f64>>| {
            RBTree::from_iter(
                history
                    .iter()
                    .filter(|(time, _)| (**time >= start) && (**time <= end))
                    .map(|(time, book)| {
                        (
                            time.clone(),
                            book.iter()
                                .fold(0.0, |accumulate, (_, quantity)| accumulate + quantity),
                        )
                    }),
            )
        };

        let readable_asks = self.asks.read().await;
        let readable_bids = self.bids.read().await;

        (integrate(&readable_asks), integrate(&readable_bids))
    }

    pub async fn extract_window(&self, start: i64, end: i64) -> BookHistory {
        let extract = |history: &RBTree<i64, RBTree<Price, f64>>| {
            RBTree::from_iter(
                history
                    .iter()
                    .filter(|(time, _)| (**time >= start) && (**time <= end))
                    .map(|(time, book)| (time.clone(), book.clone())),
            )
        };

        let readable_asks = self.asks.read().await;
        let readable_bids = self.bids.read().await;

        BookHistory {
            time_window_in_seconds: (end - start).abs() as usize,
            asks: RwLock::new(extract(&readable_asks)),
            bids: RwLock::new(extract(&readable_bids)),
        }
    }
}
