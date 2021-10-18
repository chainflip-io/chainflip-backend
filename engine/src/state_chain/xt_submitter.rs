use slog::o;
use state_chain_runtime::Call;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::logging::COMPONENT_KEY;

use super::client::StateChainClient;

/// Starts the extrinsic submitter, which accepts extriniscs through a channel
/// tracks the nonce, and submits using the correct nonce
pub async fn start(
    state_chain_client: Arc<StateChainClient>,
    mut xt_receiver: UnboundedReceiver<Call>,
    logger: &slog::Logger,
) {
    let logger = logger.new(o!(COMPONENT_KEY => "XtSubmitter"));
    while let Some(call) = xt_receiver.recv().await {
        slog::debug!(logger, "Submitting extrinsic: {:?}", call);
        // TODO: Handle this error

        let nonce: u32 = 1;
        state_chain_client
            .submit_extrinsic_with_nonce(nonce, call)
            .await
            .unwrap();
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
