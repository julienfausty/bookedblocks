use crate::feed::{Booked, Order};
use crate::splat::{splat_1d, splat_2d};

use tokio::sync::RwLock;

use chrono::{DateTime, Utc};
use ndarray::Array2;
use rbtree::RBTree;

use std::cmp::{Ordering, max, min};
use std::iter::zip;

#[derive(Clone, Debug, PartialOrd, PartialEq)]
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

#[derive(Debug)]
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

    pub async fn get_latest_book(&self) -> ((i64, RBTree<Price, f64>), (i64, RBTree<Price, f64>)) {
        let readable_asks = self.asks.read().await;
        let readable_bids = self.bids.read().await;

        match (readable_asks.get_last(), readable_bids.get_last()) {
            (Some((time_ask, asks)), Some((time_bid, bids))) => (
                (time_ask.clone(), asks.clone()),
                (time_bid.clone(), bids.clone()),
            ),
            _ => ((0, RBTree::new()), (0, RBTree::new())),
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

#[derive(Clone, Debug)]
pub struct RenderGrid {
    pub number_time_values: usize,
    pub time_range: (i64, i64),
    pub number_price_values: usize,
    pub price_range: (f64, f64),
}

pub struct GenerateGrid {
    time_window_in_seconds: u64,
    number_time_values: usize,
    number_price_values: usize,
}

impl GenerateGrid {
    pub async fn grid(&self, history: &BookHistory) -> RenderGrid {
        let readable_asks = history.asks.read().await;
        let readable_bids = history.bids.read().await;

        let latest_time = match (readable_asks.get_last(), readable_bids.get_last()) {
            (Some((time_asks, _)), Some((time_bids, _))) => max(time_asks, time_bids).clone(),
            (Some((time_asks, _)), None) => time_asks.clone(),
            (None, Some((time_bids, _))) => time_bids.clone(),
            (None, None) => Utc::now().timestamp(),
        };

        let time_range = (
            latest_time - (self.time_window_in_seconds as i64),
            latest_time,
        );

        let minimal_bid = readable_bids
            .iter()
            .filter(|(time, _)| *time >= &time_range.0 || *time <= &time_range.1)
            .map(|(_, prices)| {
                prices
                    .get_first()
                    .and_then(|(price, _)| Some(price.clone()))
                    .get_or_insert(Price { value: f64::MAX })
                    .clone()
            })
            .fold(Price { value: f64::MAX }, |minimal, price| {
                min(minimal, price.clone())
            })
            .value
            .clone();

        let minimal_bid = if minimal_bid == f64::MAX {
            0.0
        } else {
            minimal_bid
        };

        let maximal_ask = readable_asks
            .iter()
            .filter(|(time, _)| *time >= &time_range.0 || *time <= &time_range.1)
            .map(|(_, prices)| {
                prices
                    .get_last()
                    .and_then(|(price, _)| Some(price.clone()))
                    .get_or_insert(Price { value: 0.0 })
                    .clone()
            })
            .fold(Price { value: 0.0 }, |maximal, price| {
                max(maximal, price.clone())
            })
            .value
            .clone();

        RenderGrid {
            number_time_values: self.number_time_values.clone(),
            time_range: time_range,
            number_price_values: self.number_price_values.clone(),
            price_range: (minimal_bid, maximal_ask),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SplattedDepth {
    pub price_range: (f64, f64),
    pub volumes: Vec<f64>,
}

pub struct SplatDepth {}

impl SplatDepth {
    pub async fn splat(grid: &RenderGrid, history: &BookHistory) -> SplattedDepth {
        let ((_, latest_asks), (_, latest_bids)) = history.get_latest_book().await;
        let ask_support = splat_1d(
            &grid.price_range,
            grid.number_price_values,
            latest_asks
                .into_iter()
                .map(|(price, volume)| (price.value, volume))
                .collect(),
        );

        let bid_support = splat_1d(
            &grid.price_range,
            grid.number_price_values,
            latest_bids
                .into_iter()
                .map(|(price, volume)| (price.value, volume))
                .collect(),
        );

        SplattedDepth {
            price_range: grid.price_range.clone(),
            volumes: zip(ask_support, bid_support)
                .map(|(ask, bid)| ask - bid)
                .collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SplattedVolumes {
    pub time_range: (i64, i64),
    pub ask_volumes: Vec<f64>,
    pub bid_volumes: Vec<f64>,
}

pub struct SplatVolume {}

impl SplatVolume {
    pub async fn splat(grid: &RenderGrid, history: &BookHistory) -> SplattedVolumes {
        let (ask_volumes, bid_volumes) = history
            .integrate_window(grid.time_range.0, grid.time_range.1)
            .await;

        let ask_support = splat_1d(
            &(grid.time_range.0 as f64, grid.time_range.1 as f64),
            grid.number_time_values,
            ask_volumes
                .into_iter()
                .map(|(time, volume)| (time as f64, volume))
                .collect(),
        );

        let bid_support = splat_1d(
            &(grid.time_range.0 as f64, grid.time_range.1 as f64),
            grid.number_time_values,
            bid_volumes
                .into_iter()
                .map(|(time, volume)| (time as f64, volume))
                .collect(),
        );

        SplattedVolumes {
            time_range: grid.time_range.clone(),
            ask_volumes: ask_support,
            bid_volumes: bid_support,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SplattedBlocks {
    pub grid: RenderGrid,
    pub volumes: Array2<f64>,
}

pub struct SplatBlocks {}

impl SplatBlocks {
    pub async fn splat(grid: &RenderGrid, history: &BookHistory) -> SplattedBlocks {
        let extract = history
            .extract_window(grid.time_range.0, grid.time_range.1)
            .await;

        let mut source = Vec::new();
        {
            let readable_asks = extract.asks.read().await;
            for (time, state) in readable_asks.iter() {
                for (price, volume) in state.iter() {
                    source.push((time.clone() as f64, price.value.clone(), volume.clone()));
                }
            }
        }

        let ask_support = splat_2d(
            (
                &(grid.time_range.0 as f64, grid.time_range.1 as f64),
                &grid.price_range,
            ),
            (grid.number_time_values, grid.number_price_values),
            source,
        );

        let mut source = Vec::new();
        {
            let readable_bids = extract.bids.read().await;
            for (time, state) in readable_bids.iter() {
                for (price, volume) in state.iter() {
                    source.push((time.clone() as f64, price.value.clone(), volume.clone()));
                }
            }
        }

        let bid_support = splat_2d(
            (
                &(grid.time_range.0 as f64, grid.time_range.1 as f64),
                &grid.price_range,
            ),
            (grid.number_time_values, grid.number_price_values),
            source,
        );

        SplattedBlocks {
            grid: grid.clone(),
            volumes: ask_support - bid_support,
        }
    }
}

pub struct Pipeline {
    grid_generator: GenerateGrid,
}

impl Pipeline {
    pub fn new(
        time_window_in_seconds: u64,
        number_time_values: usize,
        number_price_values: usize,
    ) -> Pipeline {
        Pipeline {
            grid_generator: GenerateGrid {
                time_window_in_seconds,
                number_time_values,
                number_price_values,
            },
        }
    }
    pub async fn run(
        &self,
        history: &BookHistory,
    ) -> (SplattedDepth, SplattedVolumes, SplattedBlocks) {
        let grid = self.grid_generator.grid(history).await;

        (
            SplatDepth::splat(&grid, history).await,
            SplatVolume::splat(&grid, history).await,
            SplatBlocks::splat(&grid, history).await,
        )
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use chrono::DateTime;

    fn generic_booked_case() -> Booked {
        Booked {
            symbol: "ETH/EUR".to_string(),
            timestamp: DateTime::from_timestamp(0, 0).unwrap().to_rfc3339(),
            asks: vec![
                Order {
                    price: 5.0,
                    quantity: 6.0,
                },
                Order {
                    price: 7.0,
                    quantity: 8.0,
                },
            ],
            bids: vec![
                Order {
                    price: 1.0,
                    quantity: 2.0,
                },
                Order {
                    price: 3.0,
                    quantity: 4.0,
                },
            ],
        }
    }

    #[tokio::test]
    async fn test_empty_history() {
        let history = BookHistory::new(60);

        let latest = history.get_latest_book().await;
        assert_eq!(latest.0.0, 0);
        assert_eq!(latest.1.0, 0);

        assert_eq!(latest.0.1.len(), 0);
        assert_eq!(latest.1.1.len(), 0);

        let integrated = history.integrate_window(0, 60).await;
        assert_eq!(integrated.0.len(), 0);
        assert_eq!(integrated.1.len(), 0);

        let extracted = history.extract_window(0, 45).await;
        assert_eq!(extracted.time_window_in_seconds, 45);

        let readable_asks = extracted.asks.read().await;
        let readable_bids = extracted.bids.read().await;
        assert_eq!(readable_asks.len(), 0);
        assert_eq!(readable_bids.len(), 0);
    }

    #[tokio::test]
    async fn test_book_updates() {
        let mut history = BookHistory::new(60);

        let updated = history.update(generic_booked_case()).await;
        assert!(updated.is_ok());
        assert!(!updated.unwrap().is_some());

        let readable_asks = history.asks.read().await;
        let readable_bids = history.bids.read().await;

        assert_eq!(readable_asks.len(), 1);
        assert_eq!(readable_bids.len(), 1);

        let first_asks = readable_asks.get_first();
        let first_bids = readable_bids.get_first();

        assert!(first_asks.is_some());
        assert!(first_bids.is_some());

        let (asks_time, asks) = first_asks.unwrap();
        let (bids_time, bids) = first_bids.unwrap();

        assert_eq!(*asks_time, 0);
        assert_eq!(*bids_time, 0);

        assert_eq!(asks.len(), 2);
        assert_eq!(bids.len(), 2);

        itertools::assert_equal(
            asks.clone().into_iter(),
            vec![(Price { value: 5.0 }, 6.0), (Price { value: 7.0 }, 8.0)].into_iter(),
        );

        itertools::assert_equal(
            bids.clone().into_iter(),
            vec![(Price { value: 1.0 }, 2.0), (Price { value: 3.0 }, 4.0)].into_iter(),
        );
    }

    #[tokio::test]
    async fn test_bad_timestamped_update() {
        let mut history = BookHistory::new(60);

        let mut booked = generic_booked_case();
        booked.timestamp = "Bad Timestamp".to_string();

        let updated = history.update(booked).await;
        assert!(updated.is_err());
    }

    #[tokio::test]
    async fn test_latest_book() {
        let mut history = BookHistory::new(60);

        let _ = history.update(generic_booked_case()).await;

        let ((asks_time, asks), (bids_time, bids)) = history.get_latest_book().await;

        assert_eq!(asks_time, 0);
        assert_eq!(bids_time, 0);

        assert_eq!(asks.len(), 2);
        assert_eq!(bids.len(), 2);

        itertools::assert_equal(
            asks.clone().into_iter(),
            vec![(Price { value: 5.0 }, 6.0), (Price { value: 7.0 }, 8.0)].into_iter(),
        );

        itertools::assert_equal(
            bids.clone().into_iter(),
            vec![(Price { value: 1.0 }, 2.0), (Price { value: 3.0 }, 4.0)].into_iter(),
        );
    }

    #[tokio::test]
    async fn test_book_multiple_book_updates() {
        let mut history = BookHistory::new(60);

        let updated = history.update(generic_booked_case()).await;
        assert!(updated.is_ok());
        assert!(!updated.unwrap().is_some());

        {
            let readable_asks = history.asks.read().await;
            let readable_bids = history.bids.read().await;

            assert_eq!(readable_asks.len(), 1);
            assert_eq!(readable_bids.len(), 1);
        }

        let mut booked = generic_booked_case();
        booked.timestamp = DateTime::from_timestamp(60, 0).unwrap().to_rfc3339();
        let updated = history.update(booked).await;
        assert!(updated.is_ok());
        assert!(!updated.unwrap().is_some());

        {
            let readable_asks = history.asks.read().await;
            let readable_bids = history.bids.read().await;

            assert_eq!(readable_asks.len(), 2);
            assert_eq!(readable_bids.len(), 2);
        }

        let mut booked = generic_booked_case();
        booked.timestamp = DateTime::from_timestamp(61, 0).unwrap().to_rfc3339();
        let updated = history.update(booked).await;
        assert!(updated.is_ok());
        let updated_option = updated.unwrap();
        assert!(updated_option.is_some());

        let ((time_asks, _), (time_bids, _)) = updated_option.unwrap();
        assert_eq!(time_asks, 0);
        assert_eq!(time_bids, 0);

        {
            let readable_asks = history.asks.read().await;
            let readable_bids = history.bids.read().await;

            assert_eq!(readable_asks.len(), 2);
            assert_eq!(readable_bids.len(), 2);
        }
    }

    #[tokio::test]
    async fn test_integrate_window() {
        let mut history = BookHistory::new(60);

        for i_time in 0..60 {
            let mut booked = generic_booked_case();
            booked.timestamp = DateTime::from_timestamp(i_time, 0).unwrap().to_rfc3339();
            let updated = history.update(booked).await;
            assert!(updated.is_ok());
            assert!(!updated.unwrap().is_some());
        }

        let latest = history.get_latest_book().await;
        assert_eq!(latest.0.0, 59);
        assert_eq!(latest.1.0, 59);

        let (integrated_asks, integrated_bids) = history.integrate_window(10, 40).await;

        itertools::assert_equal(
            integrated_asks.into_iter(),
            (10..41).map(|time| (time, 14.0)),
        );
        itertools::assert_equal(
            integrated_bids.into_iter(),
            (10..41).map(|time| (time, 6.0)),
        );

        let extracted = history.extract_window(15, 35).await;

        assert_eq!(extracted.time_window_in_seconds, 20);

        let extracted_asks = extracted.asks.read().await;
        let extracted_bids = extracted.bids.read().await;

        itertools::assert_equal(
            extracted_asks.clone().into_iter().map(|(time, _)| time),
            15..36,
        );
        itertools::assert_equal(
            extracted_bids.clone().into_iter().map(|(time, _)| time),
            15..36,
        );
    }
}
