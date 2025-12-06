use crate::actions::Action;

use kraken_async_rs::wss::{KrakenMessageStream, KrakenWSSClient, WS_KRAKEN, WS_KRAKEN_AUTH};
use kraken_async_rs::wss::{Message, WssMessage};

use tokio::sync::mpsc::Sender;
use tokio::time::timeout;

pub struct Feed {
    // timeout for the websocket connection
    timeout_in_seconds: u64,
    // websocket connection to Kraken WS API
    connection: KrakenMessageStream<WssMessage>,
    // object to forward actions to
    action_sender: Sender<Action>,
}

impl Feed {
    pub async fn new(timeout_in_seconds: u64, sender: Sender<Action>) -> Result<Feed, String> {
        let mut client = KrakenWSSClient::new_with_urls(WS_KRAKEN, WS_KRAKEN_AUTH);
        let connection = match client.connect::<WssMessage>().await {
            Ok(connection) => connection,
            Err(message) => return Err(format!("{:?}", message)),
        };

        Ok(Feed {
            timeout_in_seconds,
            connection,
            action_sender: sender,
        })
    }

    pub async fn listen(&self) -> Result<(), String> {
        Ok(())
    }

    pub async fn subscribe(&mut self, ticker: String) -> Result<(), String> {
        Ok(())
    }
}
