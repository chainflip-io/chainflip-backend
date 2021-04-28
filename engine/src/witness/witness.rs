use std::sync::Arc;

use tokio::sync::Mutex;

use crate::mq::IMQClient;

use super::sc::{self, sc_observer};

pub async fn start<M: 'static + IMQClient + Send + Sync>(mq_client: Arc<Mutex<M>>) {
    // Start the state chain witness
    sc_observer::start(mq_client.clone()).await;

    // Start the other witness processes...
}
