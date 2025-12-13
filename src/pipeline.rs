use crate::feed::Booked;

use tokio::sync::RwLock;

use rbtree::RBTree;

use std::cmp::Ordering;

#[derive(PartialOrd, PartialEq)]
pub struct Price {
    pub value: f64,
}

impl Eq for Price {}

impl Ord for Price {
    fn cmp(&self, other: &Price) -> Ordering {
        f64::total_cmp(&self.value, &other.value)
    }
}

pub struct BookHistory {
    pub time_window_in_seconds: usize,
    pub asks: RwLock<RBTree<usize, RBTree<Price, f64>>>,
    pub bids: RwLock<RBTree<usize, RBTree<Price, f64>>>,
}

impl BookHistory {
    pub fn new(time_window_in_seconds: usize) -> BookHistory {
        BookHistory {
            time_window_in_seconds,
            asks: RwLock::new(RBTree::new()),
            bids: RwLock::new(RBTree::new()),
        }
    }

    pub async fn update(&mut self, booked: Booked) -> Result<Option<RBTree<Price, f64>>, String> {
        Ok(None)
    }

    pub async fn get_last_book(&self) -> Option<(RBTree<Price, f64>, RBTree<Price, f64>)> {
        None
    }

    pub async fn integrate_window(
        &self,
        start: u64,
        end: u64,
    ) -> Option<(RBTree<u64, f64>, RBTree<u64, f64>)> {
        None
    }

    pub async fn extract_window(&self, start: u64, end: u64) -> Option<BookHistory> {
        None
    }
}
