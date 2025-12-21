use crate::feed::{Booked, TickerState};

/// Enum encapsulating different actions that can be performed by application
#[derive(Debug)]
pub enum Action {
    /// Provide log message
    Inform(String),
    /// Subscribe a new ticker to feed
    SubscribeTicker(String),
    /// Quit the application
    Quit,
    /// Run processign pipeline to update given ticker
    RunPipeline(String),
    /// Unsubscribe existing ticker
    UnsubscribeTicker(String),
    /// Update order book cache with new information
    UpdateBook(Booked),
    /// Update ticker data with latest information
    UpdateTicker(TickerState),
    // Provide a log warning
    Warn(String),
}
