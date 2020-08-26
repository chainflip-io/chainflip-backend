use super::EthereumClient;
use crate::common::ethereum;
use async_trait::async_trait;
use web3::{
    transports,
    types::{self, Block, BlockId, BlockNumber, Transaction, U64},
    Web3,
};

/// A Web3 ethereum client
pub struct Web3Client {
    web3: Web3<transports::Http>,
}

impl Web3Client {
    /// Create a new web3 ethereum client with the given transport
    pub fn new(transport: transports::Http) -> Self {
        let web3 = Web3::new(transport);
        Web3Client { web3 }
    }

    /// Create a web3 ethereum http client from a url
    pub fn url(url: &str) -> Result<Self, String> {
        let transport = transports::Http::new(url).map_err(|err| format!("{}", err))?;
        Ok(Web3Client::new(transport))
    }
}

#[async_trait]
impl EthereumClient for Web3Client {
    async fn get_latest_block_number(&self) -> Result<u64, String> {
        match self.web3.eth().block_number().await {
            Ok(result) => Ok(result.as_u64()),
            Err(err) => Err(format!("{}", err)),
        }
    }

    async fn get_transactions(&self, block_number: u64) -> Option<Vec<ethereum::Transaction>> {
        let block_number = BlockNumber::Number(U64([block_number]));
        let block: Option<Block<Transaction>> = match self
            .web3
            .eth()
            .block_with_txs(BlockId::Number(block_number))
            .await
        {
            Ok(result) => result,
            Err(error) => {
                debug!("[Web3] Failed to get block with txs: {}", error);
                return None;
            }
        };

        if let Some(block) = block {
            let txs = block
                .transactions
                .iter()
                .map(|tx| ethereum::Transaction {
                    hash: tx.hash.into(),
                    index: tx.transaction_index.unwrap().as_u64(),
                    block_number: tx.block_number.unwrap().as_u64(),
                    from: tx.from.into(),
                    to: tx.to.map(|val| val.into()),
                    value: tx.value.as_u128(),
                })
                .collect();
            return Some(txs);
        }

        None
    }
}

impl Into<ethereum::Hash> for types::H256 {
    fn into(self) -> ethereum::Hash {
        ethereum::Hash(self.to_fixed_bytes())
    }
}

impl Into<ethereum::Address> for types::H160 {
    fn into(self) -> ethereum::Address {
        ethereum::Address(self.to_fixed_bytes())
    }
}
