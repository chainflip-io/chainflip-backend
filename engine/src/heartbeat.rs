use substrate_subxt::{Client, PairSigner};

use crate::state_chain::runtime::StateChainRuntime;

use crate::state_chain::pallets::reputation::HeartbeatCallExt;

use sp_keyring::AccountKeyring;

/// Starts the CFE heartbeat. Submits an extrinsic to the SC every HeartbeatIntervalPeriod / 2 blocks
pub async fn start_heartbeat(subxt_client: Client<StateChainRuntime>) {
    println!("Starting heartbeat");

    let alice = AccountKeyring::Alice.pair();
    let pair_signer = PairSigner::new(alice);

    // submit a heartbeat on startup
    let heartbeat_result = subxt_client.heartbeat(&pair_signer).await;

    println!("Here's the heartbeat result: {:?}", heartbeat_result);

    // // TODO: Handle errors
    // let heartbeat_block_interval = subxt_client
    //     .metadata()
    //     .module("Reputation")
    //     .unwrap()
    //     .constant("HeartbeatBlockInterval")
    //     .unwrap()
    //     .value::<i32>()
    //     .unwrap();
    // // println!("The heartbeat interval is: {:?}", heartbeat_block_interval);

    // // temp set it to 2 for testing
    // let heartbeat_block_interval: i32 = 4;
    // let send_heartbeat_interval: i32 = heartbeat_block_interval / 2;

    // let mut blocks = subxt_client
    //     .subscribe_finalized_blocks()
    //     .await
    //     .expect("Should subscribe to finalised blocks");

    // let mut count: i32 = 0;
    // while let Some(_) = blocks.next().await {
    //     count += 1;
    //     if count % send_heartbeat_interval == 0 {
    //         // send a heartbeat
    //     }
    // }
}

#[cfg(test)]
mod tests {
    use substrate_subxt::ClientBuilder;

    use crate::settings;

    use super::*;

    #[tokio::test]
    #[ignore = "depends on sc"]
    async fn test_start_heartbeat() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        start_heartbeat(
            ClientBuilder::<StateChainRuntime>::new()
                .set_url(&settings.state_chain.ws_endpoint)
                .build()
                .await
                .expect("Should create subxt client"),
        )
        .await;
    }
}
