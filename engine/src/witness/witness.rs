use std::sync::Arc;

use std::sync::Mutex;

use crate::mq::IMQClient;

use super::sc::{self, sc_observer};

pub async fn start<M: IMQClient>(mq_client: Arc<M>) {
    // Start the state chain witness
    sc_observer::start(mq_client.clone()).await;

    // Start the other witness processes...
}
