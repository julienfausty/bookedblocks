use crate::actions::Action;

use kraken_async_rs::wss::{
    BookSubscription, KrakenMessageStream, KrakenWSSClient, WS_KRAKEN, WS_KRAKEN_AUTH,
};
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
    // request id counter
    request_id: i64,
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
            request_id: 0,
        })
    }

    pub async fn listen(&self) -> Result<(), String> {
        Ok(())
    }

    pub async fn subscribe(&mut self, ticker: String, depth: usize) -> Result<(), String> {
        let mut subscription = BookSubscription::new(vec![ticker]);
        subscription.snapshot = Some(true);
        subscription.depth = Some(depth as i32);

        let subscription_message = Message::new_subscription(subscription, self.request_id.clone());
        self.request_id += 1;

        match self.connection.send(&subscription_message).await {
            Ok(_) => Ok(()),
            Err(message) => Err(format!("{:?}", message)),
        }
    }
}
