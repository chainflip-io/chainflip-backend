use slog::o;
use state_chain_runtime::Call;
use std::{convert::TryInto, sync::Arc};
use tokio::sync::mpsc::UnboundedReceiver;

use super::client::StateChainClient;

/// Starts the extrinsic submitter, which accepts extriniscs through a channel
/// tracks the nonce, and submits using the correct nonce
pub async fn start(
    state_chain_client: Arc<StateChainClient>,
    mut xt_receiver: UnboundedReceiver<Call>,
    logger: &slog::Logger,
) {
    while let Some(call) = xt_receiver.recv().await {
        state_chain_client.submit_extrinsic(logger, call).await;
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_xt_submitter() {
        todo!()
    }
}
