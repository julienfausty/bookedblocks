use crate::feed::{Booked, TickerState};

#[derive(Debug)]
pub enum Action {
    Inform(String),
    Launch,
    SubscribeTicker(String),
    Quit,
    UnsubscribeTicker(String),
    UpdateBook(Booked),
    UpdateTicker(TickerState),
    Warn(String),
}
