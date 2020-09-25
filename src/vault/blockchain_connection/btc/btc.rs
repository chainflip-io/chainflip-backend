use super::BitcoinClient;
use async_trait::async_trait;
use bitcoin::Transaction;
use bitcoincore_rpc::RpcApi;
use bitcoincore_rpc::{self, Auth};

use std::sync::Arc;

/// Wraps the BTC RPC Client
pub struct BtcClient {
    rpc_client: Arc<bitcoincore_rpc::Client>,
}

impl BtcClient {
    /// create BtcClient from a daemon url and an rpc Auth enum
    pub fn new_from_url_auth(url: &str, auth: Auth) -> Result<Self, String> {
        let rpc_client = bitcoincore_rpc::Client::new(String::from(url), auth)
            .map_err(|err| format!("{}", err))?;
        let rpc_client_arc = Arc::new(rpc_client);
        Ok(BtcClient {
            rpc_client: rpc_client_arc,
        })
    }
}

#[async_trait]
impl BitcoinClient for BtcClient {
    async fn get_latest_block_number(&self) -> Result<u64, String> {
        match self.rpc_client.get_block_count() {
            Ok(block_number) => Ok(block_number as u64),
            Err(err) => Err(format!("{}", err)),
        }
    }

    async fn get_transactions(&self, block_number: u64) -> Option<Vec<Transaction>> {
        let block_hash = match self.rpc_client.get_block_hash(block_number) {
            Ok(block_hash) => block_hash,
            Err(error) => {
                debug!(
                    "Failed to get block hash for block {}, {}",
                    block_number, error
                );
                return None;
            }
        };

        match self.rpc_client.get_block(&block_hash) {
            Ok(block) => Some(block.txdata),
            Err(error) => {
                debug!("Could not fetch block, {}", error);
                None
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn returns_latest_block_number() {
        let auth = Auth::UserPass(String::from("bitcoinrpc"), String::from("Password123"));
        let client = BtcClient::new_from_url_auth("http://127.0.0.1:18332", auth).unwrap();
        assert!(client.get_latest_block_number().await.is_ok());
    }

    #[tokio::test]
    #[ignore]
    async fn returns_transactions() {
        // This tested block is:
        // https://live.blockcypher.com/btc-testnet/block/00000000000000b4e5c133075b925face5b22dccb53112e4c7bf95313e0cf7f2/
        let test_block_number = 1834585;
        // Bitcoind needs to be configured with a bitcoin.conf containing rpcuser and rpcpassword
        let auth = Auth::UserPass(String::from("bitcoinrpc"), String::from("Password123"));
        let client = BtcClient::new_from_url_auth("http://127.0.0.1:18332", auth).unwrap();
        let transactions = client
            .get_transactions(test_block_number)
            .await
            .expect("Expected to get valid transactions");
        assert_eq!(transactions.len(), 11);

        let first = transactions
            .first()
            .expect("Expected to get a valid first transaction");

        assert_eq!(first.version, 1);
    }
}
