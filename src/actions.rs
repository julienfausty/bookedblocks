#[derive(Debug)]
pub enum Action {
    Launch,
    SubscribeTicker(String),
    Quit,
    WarningMessage(String),
}
