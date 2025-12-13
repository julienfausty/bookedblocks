use crate::feed::{Booked, TickerState};

#[derive(Debug)]
pub enum Action {
    Inform(String),
    Launch,
    SubscribeTicker(String),
    Quit,
    UpdateBook(Booked),
    UpdateTicker(TickerState),
    Warn(String),
}
