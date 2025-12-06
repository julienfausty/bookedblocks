use crate::actions::Action;

use kraken_async_rs::wss::{
    BookSubscription, KrakenMessageStream, KrakenWSSClient, WS_KRAKEN, WS_KRAKEN_AUTH,
};
use kraken_async_rs::wss::{Message, WssMessage};

use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;
use tokio::task::{JoinHandle, spawn};
use tokio::time::{Duration, sleep, timeout};
use tokio_stream::StreamExt;

use std::sync::Arc;

pub struct Feed {
    // websocket connection to Kraken WS API
    connection: Arc<Mutex<KrakenMessageStream<WssMessage>>>,
    // the depth to request the book data
    depth: i32,
    // handle to websocket listener
    listener_handle: JoinHandle<Result<(), String>>,
    // request id counter
    request_id: i64,
}

async fn listen_to_connection(
    sender: Sender<Action>,
    connection: Arc<Mutex<KrakenMessageStream<WssMessage>>>,
    timeout_in_seconds: u64,
) -> Result<(), String> {
    loop {
        loop {
            let mut stream = connection.lock().await;
            match timeout(Duration::from_secs(timeout_in_seconds), stream.next()).await {
                Ok(Some(communication)) => {
                    // TODO: implement logic here: if heartbeat break to release lock / if data
                    // treat it and send to dispatch
                    break;
                }
                Ok(None) => return Ok(()),
                Err(message) => return Err(format!("{:?}", message)),
            }
        }
        // allow some time for subscriptions on the connection
        sleep(Duration::from_millis(200)).await;
    }
}

impl Feed {
    pub async fn new(
        timeout_in_seconds: u64,
        depth: i32,
        sender: Sender<Action>,
    ) -> Result<Feed, String> {
        let mut client = KrakenWSSClient::new_with_urls(WS_KRAKEN, WS_KRAKEN_AUTH);
        let connection = match client.connect::<WssMessage>().await {
            Ok(connection) => Arc::new(Mutex::new(connection)),
            Err(message) => return Err(format!("{:?}", message)),
        };

        let cloned_connection = connection.clone();
        let listener_handle = tokio::spawn(async move {
            listen_to_connection(sender, cloned_connection, timeout_in_seconds).await
        });

        Ok(Feed {
            connection,
            depth,
            listener_handle,
            request_id: 0,
        })
    }

    pub async fn subscribe(&mut self, ticker: String) -> Result<(), String> {
        let mut subscription = BookSubscription::new(vec![ticker]);
        subscription.snapshot = Some(true);
        subscription.depth = Some(self.depth);

        let subscription_message = Message::new_subscription(subscription, self.request_id.clone());
        self.request_id += 1;

        let mut writable = self.connection.lock().await;

        match writable.send(&subscription_message).await {
            Ok(_) => Ok(()),
            Err(message) => Err(format!("{:?}", message)),
        }
    }

    pub async fn check_listener(&self) -> Result<(), String> {
        Ok(())
    }
}
