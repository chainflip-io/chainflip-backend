use crate::{p2p::ValidatorId, settings};
use reqwest::header;

use serde::Deserialize;

use anyhow::Result;

#[derive(Deserialize, Debug)]
struct PeerId {
    // this will be the peer id in base58
    result: String,
}

/// Get the peer id from the state chain via RPC
/// and return as ValidatorId type
pub async fn get_peer_id(state_chain_settings: settings::StateChain) -> Result<ValidatorId> {
    const PEER_ID_RPC: &'static str = "system_localPeerId";

    let state_chain_peer_rpc = format!(
        "http://{}:{}/{}",
        state_chain_settings.hostname, state_chain_settings.rpc_port, PEER_ID_RPC
    );
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );

    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .expect("Client should be constructed");
    let resp = client.post(state_chain_peer_rpc).send().await?;

    let peer_id = resp.json::<PeerId>().await?;

    let validator_id = ValidatorId::from_base58(&peer_id.result)?;
    return Ok(validator_id);
}

#[cfg(test)]
mod tests {
    use crate::settings;

    use super::*;

    #[tokio::test]
    #[ignore = "depends on running state chain"]
    async fn test_get_peer_id() {
        let test_settings = settings::test_utils::new_test_settings().unwrap();

        get_peer_id(test_settings.state_chain).await.unwrap();
    }
}
