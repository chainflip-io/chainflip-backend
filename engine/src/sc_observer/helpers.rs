//! ============ Helper methods ==============

use anyhow::Result;

use super::runtime::StateChainRuntime;

use crate::settings;

use substrate_subxt::{Client, ClientBuilder};

/// Create a substrate subxt client over the StateChainRuntime
pub async fn create_subxt_client(
    subxt_settings: settings::StateChain,
) -> Result<Client<StateChainRuntime>> {
    // ?: Can we use a particular set of keys here? or at least point to the keys we want to use
    // or is the signing done when the extrinsic is submitted?
    let client = ClientBuilder::<StateChainRuntime>::new()
        .set_url(format!(
            "ws://{}:{}",
            subxt_settings.hostname, subxt_settings.port
        ))
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
