use std::sync::Arc;
use tokio::sync::Mutex;

use slog::o;
use substrate_subxt::{Client, PairSigner};

use crate::logging::COMPONENT_KEY;
use crate::state_chain::runtime::StateChainRuntime;

use crate::state_chain::pallets::reputation::HeartbeatCallExt;

/// Starts the CFE heartbeat.
/// Submits a heartbeat to the SC on start up and then every HeartbeatBlockInterval / 2 blocks
pub async fn start(
    subxt_client: Client<StateChainRuntime>,
    signer: Arc<Mutex<PairSigner<StateChainRuntime, sp_core::sr25519::Pair>>>,
    logger: &slog::Logger,
) {
    let logger = logger.new(o!(COMPONENT_KEY => "Heartbeat"));
    slog::info!(logger, "Starting");

    let heartbeat_block_interval = subxt_client
        .metadata()
        .module("Reputation")
        .expect("No module 'Reputation' in chain metadata")
        .constant("HeartbeatBlockInterval")
        .expect("No constant 'HeartbeatBlockInterval' in chain metadata for module 'Reputation'")
        .value::<u32>()
        .expect("Could not decode HeartbeatBlockInterval to u32");

    async fn submit_heartbeat(
        subxt_client: &Client<StateChainRuntime>,
        signer: Arc<Mutex<PairSigner<StateChainRuntime, sp_core::sr25519::Pair>>>,
        logger: &slog::Logger,
    ) {
        let mut signer = signer.lock().await;
        match subxt_client.heartbeat(&*signer).await {
            Ok(_) => {
                slog::info!(logger, "Sent heartbeat successfully");
                signer.increment_nonce();
            }
            Err(e) => {
                slog::error!(logger, "Failed to submit heartbeat: {:?}", e);
            }
        }
    }

    submit_heartbeat(&subxt_client, signer.clone(), &logger).await;

    slog::info!(
        logger,
        "Sending heartbeat every {} blocks",
        heartbeat_block_interval,
    );

    let mut blocks = subxt_client
        .subscribe_finalized_blocks()
        .await
        .expect("Should subscribe to finalised blocks");

    while let Some(block_header) = blocks.next().await {
        // Target the middle of the heartbeat block interval so block drift is *very* unlikely to cause failure
        if (block_header.number + (heartbeat_block_interval / 2)) % heartbeat_block_interval == 0 {
            slog::info!(logger, "Sending heartbeat");
            submit_heartbeat(&subxt_client, signer.clone(), &logger).await
        }
    }
}

#[cfg(test)]
mod tests {
    use sp_keyring::AccountKeyring;
    use substrate_subxt::ClientBuilder;

    use crate::{logging, settings};

    use super::*;

    #[tokio::test]
    #[ignore = "depends on sc"]
    async fn test_start_heartbeat() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let logger = logging::test_utils::create_test_logger();

        let alice = AccountKeyring::Alice.pair();
        let pair_signer = Arc::new(Mutex::new(PairSigner::new(alice)));

        start(
            ClientBuilder::<StateChainRuntime>::new()
                .set_url(&settings.state_chain.ws_endpoint)
                .build()
                .await
                .expect("Should create subxt client"),
            pair_signer,
            &logger,
        )
        .await;
    }
}
