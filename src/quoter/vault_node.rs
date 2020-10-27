use crate::{
    common::api, side_chain::SideChainBlock, vault::api::v1::get_blocks::BlocksQueryResponse,
};
use reqwest::Client;

pub use crate::vault::api::v1::post_stake::StakeQuoteParams;
pub use crate::vault::api::v1::post_swap::SwapQuoteParams;

/// Configuration for the vault node api
#[derive(Debug, Copy, Clone)]
pub struct Config {}

/// An interface for interacting with the vault node.
#[async_trait]
pub trait VaultNodeInterface {
    /// Get blocks starting from index `start` with a limit of `limit`.
    ///
    /// This will return all block indexes from `start` to `start + limit - 1`.
    ///
    /// # Example
    ///
    /// ```ignore
    ///     let blocks = VaultNodeInterface.get_blocks(0, 50)?;
    /// ```
    /// The above code will return blocks 0 to 49.
    async fn get_blocks(&self, start: u32, limit: u32) -> Result<Vec<SideChainBlock>, String>;

    /// Submit a swap quote to the vault node
    async fn submit_swap(&self, params: SwapQuoteParams) -> Result<serde_json::Value, String>;

    /// Submit a stake quote to the vault node
    async fn submit_stake(&self, params: StakeQuoteParams) -> Result<serde_json::Value, String>;
}

/// A client for communicating with vault nodes via http requests.
#[derive(Debug, Clone)]
pub struct VaultNodeAPI {
    url: String,
    client: Client,
}

impl VaultNodeAPI {
    /// Returns the vault node api with the config given.
    pub fn new(url: &str) -> Self {
        VaultNodeAPI {
            url: url.to_owned(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl VaultNodeInterface for VaultNodeAPI {
    async fn get_blocks(&self, start: u32, limit: u32) -> Result<Vec<SideChainBlock>, String> {
        let url = format!("{}/v1/blocks", self.url);

        let res = self
            .client
            .get(&url)
            .query(&[("number", start), ("limit", limit)])
            .send()
            .await
            .map_err(|err| err.to_string())?;

        let res = res
            .json::<api::Response<BlocksQueryResponse>>()
            .await
            .map_err(|err| err.to_string())?;

        if let Some(err) = res.error {
            return Err(err.to_string());
        }

        match res.data {
            Some(data) => Ok(data.blocks),
            None => Err("Failed to get block data".to_string()),
        }
    }

    async fn submit_swap(&self, params: SwapQuoteParams) -> Result<serde_json::Value, String> {
        let url = format!("{}/v1/swap", self.url);

        let res = self
            .client
            .post(&url)
            .json(&params)
            .send()
            .await
            .map_err(|err| err.to_string())?;

        let res = res
            .json::<api::Response<serde_json::Value>>()
            .await
            .map_err(|err| err.to_string())?;

        if let Some(err) = res.error {
            return Err(err.to_string());
        }

        match res.data {
            Some(data) => Ok(data),
            None => Err("Failed to submit quote".to_string()),
        }
    }

    async fn submit_stake(&self, params: StakeQuoteParams) -> Result<serde_json::Value, String> {
        let url = format!("{}/v1/stake", self.url);

        let res = self
            .client
            .post(&url)
            .json(&params)
            .send()
            .await
            .map_err(|err| err.to_string())?;

        let res = res
            .json::<api::Response<serde_json::Value>>()
            .await
            .map_err(|err| err.to_string())?;

        if let Some(err) = res.error {
            return Err(err.to_string());
        }

        match res.data {
            Some(data) => Ok(data),
            None => Err("Failed to submit quote".to_string()),
        }
    }
}
