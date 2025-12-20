use crate::feed::{Booked, TickerState};

#[derive(Debug)]
pub enum Action {
    Inform(String),
    SubscribeTicker(String),
    Quit,
    RunPipeline(String),
    UnsubscribeTicker(String),
    UpdateBook(Booked),
    UpdateTicker(TickerState),
    Warn(String),
}
