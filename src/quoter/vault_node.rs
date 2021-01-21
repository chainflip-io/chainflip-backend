use crate::{
    common::api,
    local_store::LocalEvent,
    vault::api::v1::{
        post_deposit::DepositQuoteParams, post_swap::SwapQuoteParams,
        post_withdraw::WithdrawParams, PortionsParams,
    },
};
use reqwest::Client;

/// Configuration for the vault node api
#[derive(Debug, Copy, Clone)]
pub struct Config {}

/// An interface for interacting with the vault node.
#[async_trait]
pub trait VaultNodeInterface {
    /// Get events starting from index `start` with a limit of `limit`.
    ///
    /// This will return all events indexes from `start` to `start + limit - 1`.
    ///
    /// # Example
    ///
    /// ```ignore
    ///     let events = VaultNodeInterface.get_events(0, 50)?;
    /// ```
    /// The above code will return events 0 to 49.
    async fn get_events(&self, start: u64, limit: u64) -> Result<Vec<LocalEvent>, String>;

    /// Get portions associated with staker_id
    async fn get_portions(&self, params: PortionsParams) -> Result<serde_json::Value, String>;

    /// Submit a swap quote to the vault node
    async fn submit_swap(&self, params: SwapQuoteParams) -> Result<serde_json::Value, String>;

    /// Submit a deposit quote to the vault node
    async fn submit_deposit(&self, params: DepositQuoteParams)
        -> Result<serde_json::Value, String>;

    /// Submit an withdraw request to the vault node
    async fn submit_withdraw(&self, params: WithdrawParams) -> Result<serde_json::Value, String>;
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
    async fn get_events(&self, start: u64, limit: u64) -> Result<Vec<LocalEvent>, String> {
        todo!("get the events");
    }

    async fn get_portions(&self, params: PortionsParams) -> Result<serde_json::Value, String> {
        let url = format!("{}/v1/portions", self.url);

        let res = self
            .client
            .get(&url)
            .query(&[
                ("stakerId", params.staker_id),
                ("pool", params.pool.to_string()),
            ])
            .send()
            .await
            .map_err(|err| err.to_string())?;

        let res = res
            .json::<api::Response<serde_json::Value>>()
            .await
            .map_err(|err| err.to_string())?;

        if let Some(err) = res.error {
            return Err(err.message);
        }

        match res.data {
            Some(data) => Ok(data),
            None => Err("Failed to get portions".to_string()),
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
            return Err(err.message);
        }

        match res.data {
            Some(data) => Ok(data),
            None => Err("Failed to submit quote".to_string()),
        }
    }

    async fn submit_deposit(
        &self,
        params: DepositQuoteParams,
    ) -> Result<serde_json::Value, String> {
        let url = format!("{}/v1/deposit", self.url);

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
            return Err(err.message);
        }

        match res.data {
            Some(data) => Ok(data),
            None => Err("Failed to submit quote".to_string()),
        }
    }

    async fn submit_withdraw(&self, params: WithdrawParams) -> Result<serde_json::Value, String> {
        let url = format!("{}/v1/withdraw", self.url);

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
            return Err(err.message);
        }

        match res.data {
            Some(data) => Ok(data),
            None => Err("Failed to submit withdraw request".to_string()),
        }
    }
}
