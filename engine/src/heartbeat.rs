use substrate_subxt::Client;

use crate::state_chain::runtime::StateChainRuntime;

/// Starts the CFE heartbeat. Submits an extrinsic to the SC every HeartbeatIntervalPeriod / 2 blocks
pub async fn start_heartbeat(subxt_client: Client<StateChainRuntime>) {
    println!("Starting heartbeat");
    // First get the HeartbeatIntervalPeriod
}

#[cfg(test)]
mod tests {
    use substrate_subxt::ClientBuilder;

    use crate::settings;

    use super::*;

    #[tokio::test]
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
