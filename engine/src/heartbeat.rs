use slog::o;
use substrate_subxt::{Client, PairSigner};

use crate::logging::COMPONENT_KEY;
use crate::state_chain::runtime::StateChainRuntime;

use crate::state_chain::pallets::reputation::HeartbeatCallExt;

/// Starts the CFE heartbeat.
/// Submits a heartbeat to the SC on start up and then every HeartbeatBlockInterval / 2 blocks
pub async fn start(
    subxt_client: Client<StateChainRuntime>,
    signer: PairSigner<StateChainRuntime, sp_core::sr25519::Pair>,
    logger: &slog::Logger,
) {
    let logger = logger.new(o!(COMPONENT_KEY => "Heartbeat"));
    slog::info!(logger, "Starting");

    subxt_client
        .heartbeat(&signer)
        .await
        .expect("Should send heartbeat on startup successfully");

    let heartbeat_block_interval = subxt_client
        .metadata()
        .module("Reputation")
        .expect("No module 'Reputation' in chain metadata")
        .constant("HeartbeatBlockInterval")
        .expect("No constant 'HeartbeatBlockInterval' in chain metadata for module 'Reputation'")
        .value::<i32>()
        .expect("Could not cast HeartbeatBlockInterval to i32");

    let send_heartbeat_interval: i32 = heartbeat_block_interval / 2;
    slog::info!(
        logger,
        "HeartbeatBlockInterval is {}. Sending heartbeat every {} blocks",
        heartbeat_block_interval,
        send_heartbeat_interval
    );

    let mut blocks = subxt_client
        .subscribe_finalized_blocks()
        .await
        .expect("Should subscribe to finalised blocks");

    let mut count: i32 = 0;
    while let Some(_) = blocks.next().await {
        count += 1;
        if count % send_heartbeat_interval == 0 {
            slog::info!(logger, "Sending heartbeat");
            if let Err(e) = subxt_client.heartbeat(&signer).await {
                slog::error!(logger, "Error submitting heartbeat: {:?}", e)
            }
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
        let pair_signer = PairSigner::new(alice);

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
