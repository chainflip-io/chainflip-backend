use std::sync::Arc;

use tokio::sync::Mutex;

use crate::mq::IMQClient;

pub async fn start<M: 'static + IMQClient + Send + Sync>(mq_client: Arc<Mutex<M>>) {
    // Start the witness processes...
    ()
}
