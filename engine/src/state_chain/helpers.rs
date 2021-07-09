//! ============ Helper state chain methods ==============

use anyhow::Result;
use sp_core::Pair;
use std::fs;

use super::runtime::StateChainRuntime;

use crate::settings;

use substrate_subxt::{Client, ClientBuilder, PairSigner};

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

pub fn get_signer_from_file(
    file_name: &str,
) -> PairSigner<StateChainRuntime, sp_core::sr25519::Pair> {
    let seed = fs::read_to_string(file_name).expect("Can't read in signing key");

    // remove the quotes that are in the file, as if entered from polkadot js
    let seed = seed.replace("\"", "");

    let pair = sp_core::sr25519::Pair::from_phrase(&seed, None).unwrap().0;
    let signer: PairSigner<StateChainRuntime, sp_core::sr25519::Pair> = PairSigner::new(pair);

    return signer;
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
