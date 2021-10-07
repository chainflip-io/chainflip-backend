use crate::state_chain::sc_observer::interface::StateChainClient;
use std::sync::Arc;

use anyhow::Result;
use futures::{Stream, StreamExt};
use slog::o;

use crate::logging::COMPONENT_KEY;

/// Starts the CFE heartbeat.
/// Submits a heartbeat to the SC on start up and then every HeartbeatBlockInterval / 2 blocks
pub async fn start<BlockStream>(
    state_chain_client: Arc<StateChainClient>,
    block_stream: BlockStream,
    logger: &slog::Logger,
) where
    BlockStream: Stream<Item = Result<state_chain_runtime::Header>>,
{
    let logger = logger.new(o!(COMPONENT_KEY => "Heartbeat"));
    slog::info!(logger, "Starting");

    // TODO: Could this a be a constant shared between the state chain and the cfe, to avoid needing to load it
    let heartbeat_block_interval = state_chain_client
        .metadata
        .module("Reputation")
        .expect("No module 'Reputation' in chain metadata")
        .constant("HeartbeatBlockInterval")
        .expect("No constant 'HeartbeatBlockInterval' in chain metadata for module 'Reputation'")
        .value::<u32>()
        .expect("Could not decode HeartbeatBlockInterval to u32");

    state_chain_client
        .submit_extrinsic(&logger, pallet_cf_reputation::Call::heartbeat())
        .await;

    slog::info!(
        logger,
        "Sending heartbeat every {} blocks",
        heartbeat_block_interval,
    );

    let mut block_stream = Box::pin(block_stream);
    while let Some(result_block_header) = block_stream.next().await {
        if let Ok(block_header) = result_block_header {
            // Target the middle of the heartbeat block interval so block drift is *very* unlikely to cause failure
            if (block_header.number + (heartbeat_block_interval / 2)) % heartbeat_block_interval
                == 0
            {
                slog::info!(
                    logger,
                    "Sending heartbeat at block: {}",
                    block_header.number
                );
                state_chain_client
                    .submit_extrinsic(&logger, pallet_cf_reputation::Call::heartbeat())
                    .await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{logging, settings, state_chain::sc_observer::interface::connect_to_state_chain};

    use super::*;

    #[tokio::test]
    #[ignore = "depends on sc"]
    async fn test_start_heartbeat() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let logger = logging::test_utils::create_test_logger();

        let (_account_id, state_chain_client, _event_stream, block_stream) = connect_to_state_chain(&settings).await.unwrap();

        start(
            state_chain_client,
            block_stream,
            &logger,
        )
        .await;
    }
}
