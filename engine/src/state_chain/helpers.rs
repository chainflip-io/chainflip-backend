//! ============ Helper methods ==============

use anyhow::Result;

use super::runtime::StateChainRuntime;

use crate::settings;

use substrate_subxt::{Client, ClientBuilder};

/// Create a substrate subxt client over the StateChainRuntime
pub async fn create_subxt_client(
    state_chain_settings: settings::StateChain,
) -> Result<Client<StateChainRuntime>> {
    let client = ClientBuilder::<StateChainRuntime>::new()
        .set_url(format!(
            "ws://{}:{}",
            state_chain_settings.hostname, state_chain_settings.ws_port
        ))
        .skip_type_sizes_check()
        .build()
        .await?;

    Ok(client)
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    #[ignore = "requires running state chain to connect to"]
    async fn can_create_subxt_client() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let client = create_subxt_client(settings.state_chain).await;
        assert!(client.is_ok());
    }
}
