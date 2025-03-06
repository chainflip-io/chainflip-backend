use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use bitcoin::{BlockHash, Txid};
use futures_core::future::LocalBoxFuture;
use crate::btc::rpc::BtcRpcApi;
use tokio::sync::RwLock;
use cf_chains::btc::{BlockNumber, BtcAmount};
use cf_utilities::future_map::FutureMap;
use crate::btc::rpc::{BlockHeader, BtcRpcClient, VerboseBlock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum RequestKey {
    BlockHash(u64),
    Block(BlockHash),
}

#[derive(Debug, Clone)]
enum ResponseValue {
    BlockHash(BlockHash),
    Block(VerboseBlock),
}
#[derive(Clone)]
pub(crate) struct BtcCachingClient {
    client: BtcRpcClient,
    cache: Arc<RwLock<HashMap<RequestKey, ResponseValue>>>,
    in_flight: Arc<RwLock<FutureMap<RequestKey, LocalBoxFuture<'static, Result<ResponseValue, anyhow::Error>>>>>,
}

impl BtcCachingClient {
    pub fn new(client: BtcRpcClient) -> Self {
        Self {
            client,
            cache: Arc::new(RwLock::new(HashMap::new())),
            in_flight: Arc::new(RwLock::new(FutureMap::default())),
        }
    }

    async fn get(&self, key: RequestKey) -> Result<ResponseValue, anyhow::Error> {
        // First, check the cache
        {
            let cache = self.cache.read().await;
            if let Some(value) = cache.get(&key) {
                return Ok(value.clone());
            }
        }

        // Check if we need to create a new request
        let should_create_request = {
            let in_flight = self.in_flight.read().await;
            !in_flight.contains_key(&key)
        };

        // Create a new request if needed
        if should_create_request {
            let cache_clone = Arc::clone(&self.cache);
            let in_flight_clone = Arc::clone(&self.in_flight);
            let key_clone = key;
            let client_clone = self.client.clone();

            let request_future: LocalBoxFuture<'static, Result<ResponseValue, anyhow::Error>> = match key {
                RequestKey::BlockHash(number) => {
                    Box::pin(async move {
                        let block_hash = client_clone.block_hash(number).await;
                        let response = ResponseValue::BlockHash(block_hash.expect("Missing blockhash"));

                        // Store in cache
                        {
                            let mut cache = cache_clone.write().await;
                            cache.insert(key_clone, response.clone());
                        }
                        // Remove from in_flight
                        {
                            let mut in_flight = in_flight_clone.write().await;
                            in_flight.remove(key_clone);
                        }
                        Ok(response)
                    })
                },
                RequestKey::Block(hash) => {
                    Box::pin(async move {
                        let block = client_clone.block(hash).await?;
                        let response = ResponseValue::Block(block);

                        // Store in cache
                        {
                            let mut cache = cache_clone.write().await;
                            cache.insert(key_clone, response.clone());
                        }
                        // Remove from in_flight
                        {
                            let mut in_flight = in_flight_clone.write().await;
                            in_flight.remove(key_clone);
                        }
                        Ok(response)
                    })
                },
                // Add other cases as needed
            };

            // Now add it to in_flight with a separate lock acquisition
            let mut in_flight = self.in_flight.write().await;

            // Double-check that no other thread added it while we were creating the future
            if !in_flight.contains_key(&key) {
                in_flight.insert(key, request_future);
            }
        }

        // Wait for the result
        self.wait_for_result(key).await
    }

    async fn wait_for_result(&self, key: RequestKey) -> Result<ResponseValue, anyhow::Error> {
        let cache = Arc::clone(&self.cache);
        let in_flight = Arc::clone(&self.in_flight);

        loop {
            // Check if the result is already in cache
            {
                let cache_read = cache.read().await;
                if let Some(value) = cache_read.get(&key) {
                    return Ok(value.clone());
                }
            }

            // Check if we need to continue waiting
            let still_in_flight = {
                let in_flight_read = in_flight.read().await;
                in_flight_read.contains_key(&key)
            };

            if !still_in_flight {
                // The request is no longer in-flight, but the result isn't in the cache.
                return Err(anyhow::anyhow!("Request completed but result not in cache"));
            }

            // The request is still in-flight, yield to allow it to progress
            tokio::task::yield_now().await;
        }
    }

    async fn block(&self, block_hash: BlockHash) -> anyhow::Result<VerboseBlock> {
        let key = RequestKey::Block(block_hash);
        match self.get(key).await? {
            ResponseValue::Block(block) => Ok(block),
            _ => Err(anyhow::anyhow!("Unexpected response type")),
        }
    }

    async fn block_hash(&self, block_number: BlockNumber) -> BlockHash {
        let key = RequestKey::BlockHash(block_number);
        match self.get(key).await.expect("Failed to get block hash") {
            ResponseValue::BlockHash(hash) => hash,
            _ => panic!("Unexpected response type"),
        }
    }

    // Directly pass through non-cached methods to the underlying client
    async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> anyhow::Result<Txid> {
        self.client.send_raw_transaction(transaction_bytes).await
    }

    async fn next_block_fee_rate(&self) -> Option<BtcAmount> {
        self.client.next_block_fee_rate().await.expect("No fee available")
    }

    async fn average_block_fee_rate(&self, block_hash: BlockHash) -> BtcAmount {
        self.client.average_block_fee_rate(block_hash).await.expect("No fee available")
    }

    async fn best_block_header(&self) -> anyhow::Result<BlockHeader> {
        let best_block_hash = self.client.best_block_hash().await?;
        self.client.block_header(best_block_hash).await
    }

    async fn block_header(&self, block_number: BlockNumber) -> anyhow::Result<BlockHeader> {
        let hash = self.client.block_hash(block_number).await.expect("No blockHash");
        self.client.block_header(hash).await
    }

}
