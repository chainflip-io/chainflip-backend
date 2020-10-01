use crate::side_chain::SideChainBlock;
use crate::transactions::QuoteTx;
use serde::{Deserialize, Serialize};

/// Parameters for `submitQuote` endpoint
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteParams {
    /// The input coin
    pub input_coin: String,
    /// The input amount
    pub input_amount: String,
    /// The input address id
    pub input_address_id: String,
    /// The input return address
    pub input_return_address: Option<String>,
    /// The output address
    pub output_address: String,
    /// The slippage limit
    pub slippage_limit: u32,
}

/// Configuration for the vault node api
#[derive(Debug, Copy, Clone)]
pub struct Config {}

/// An interface for interacting with the vault node.
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
    fn get_blocks(&self, start: u32, limit: u32) -> Result<Vec<SideChainBlock>, String>;

    /// Submit a quote to the vault node
    fn submit_quote(&self, params: QuoteParams) -> Result<QuoteTx, String>; // TODO: Change Result type to a QuoteResponse?
}

/// A client for communicating with vault nodes via http requests.
#[derive(Debug)]
pub struct VaultNodeAPI {
    url: String,
}

impl VaultNodeAPI {
    /// Returns the vault node api with the config given.
    pub fn new(url: &str) -> Self {
        VaultNodeAPI {
            url: url.to_owned(),
        }
    }
}

impl VaultNodeInterface for VaultNodeAPI {
    fn get_blocks(&self, _start: u32, _limit: u32) -> Result<Vec<SideChainBlock>, String> {
        todo!()
    }

    fn submit_quote(&self, params: QuoteParams) -> Result<QuoteTx, String> {
        todo!()
    }
}
