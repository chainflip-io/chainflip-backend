use secp256k1::SecretKey;
use sp_core::{H256, U256};
use web3::{
    api::SubscriptionStream,
    signing::SecretKeyRef,
    types::{
        Block, BlockHeader, BlockNumber, Bytes, CallRequest, Filter, Log, SignedTransaction,
        SyncState, Transaction, TransactionId, TransactionParameters, U64,
    },
    Web3,
};

use futures::{future::select_ok, TryFutureExt};

use anyhow::{Context, Result};

use crate::{
    constants::{
        ETH_DUAL_REQUEST_TIMEOUT, ETH_NODE_CONNECTION_TIMEOUT, SYNC_POLL_INTERVAL,
        WEB3_REQUEST_TIMEOUT,
    },
    settings,
};

use super::{redact_and_log_node_endpoint, TransportProtocol};

use async_trait::async_trait;

#[cfg(test)]
use mockall::automock;

pub type EthHttpRpcClient = EthRpcClient<web3::transports::Http>;
pub type EthWsRpcClient = EthRpcClient<web3::transports::WebSocket>;

#[derive(Clone)]
pub struct EthRpcClient<T: EthTransport> {
    web3: Web3<T>,
}

pub trait EthTransport: web3::Transport {
    fn transport_protocol() -> TransportProtocol;
}

impl EthTransport for web3::transports::WebSocket {
    fn transport_protocol() -> TransportProtocol {
        TransportProtocol::Ws
    }
}

impl EthTransport for web3::transports::Http {
    fn transport_protocol() -> TransportProtocol {
        TransportProtocol::Http
    }
}

// We use a trait so we can inject a mock in the tests
#[cfg_attr(test, automock)]
#[async_trait]
pub trait EthRpcApi: Send + Sync {
    async fn estimate_gas(&self, req: CallRequest, block: Option<BlockNumber>) -> Result<U256>;

    async fn sign_transaction(
        &self,
        tx: TransactionParameters,
        key: &SecretKey,
    ) -> Result<SignedTransaction>;

    async fn send_raw_transaction(&self, rlp: Bytes) -> Result<H256>;

    async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>>;

    async fn chain_id(&self) -> Result<U256>;

    async fn transaction(&self, tx_hash: H256) -> Result<Transaction>;

    /// Gets block, returning error when either:
    /// - Request fails
    /// - Request succeeds, but doesn't return a block
    async fn block(&self, block_number: U64) -> Result<Block<H256>>;
}

#[async_trait]
impl<T> EthRpcApi for EthRpcClient<T>
where
    T: Send + Sync + EthTransport,
    T::Out: Send,
{
    async fn estimate_gas(&self, req: CallRequest, block: Option<BlockNumber>) -> Result<U256> {
        self.web3
            .eth()
            .estimate_gas(req, block)
            .await
            .context(format!(
                "{} client: Failed to estimate gas",
                T::transport_protocol()
            ))
    }

    async fn sign_transaction(
        &self,
        tx: TransactionParameters,
        key: &SecretKey,
    ) -> Result<SignedTransaction> {
        self.web3
            .accounts()
            .sign_transaction(tx, SecretKeyRef::from(key))
            .await
            .context(format!(
                "{} client: Failed to sign transaction",
                T::transport_protocol()
            ))
    }

    async fn send_raw_transaction(&self, rlp: Bytes) -> Result<H256> {
        self.web3
            .eth()
            .send_raw_transaction(rlp)
            .await
            .context(format!(
                "{} client: Failed to send raw transaction",
                T::transport_protocol()
            ))
    }

    async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>> {
        let request_fut = self.web3.eth().logs(filter);

        // NOTE: if this does time out we will most likely have a
        // "memory leak" associated with rust-web3's state for this
        // request not getting properly cleaned up
        tokio::time::timeout(WEB3_REQUEST_TIMEOUT, request_fut)
            .await
            .context(format!(
                "{} client: get_logs request timeout",
                T::transport_protocol()
            ))?
            .context(format!(
                "{} client: Failed to fetch ETH logs",
                T::transport_protocol()
            ))
    }

    async fn chain_id(&self) -> Result<U256> {
        self.web3.eth().chain_id().await.context(format!(
            "{} client: Failed to fetch ETH ChainId",
            T::transport_protocol()
        ))
    }

    async fn transaction(&self, tx_hash: H256) -> Result<Transaction> {
        self.web3
            .eth()
            .transaction(TransactionId::Hash(tx_hash))
            .await
            .context(format!(
                "{} client: Failed to fetch ETH transaction",
                T::transport_protocol()
            ))
            .and_then(|opt_block| {
                opt_block.ok_or_else(|| {
                    anyhow::Error::msg(format!(
                        "{} client: Getting ETH transaction with tx_hash {} returned None",
                        T::transport_protocol(),
                        tx_hash
                    ))
                })
            })
    }

    async fn block(&self, block_number: U64) -> Result<Block<H256>> {
        self.web3
            .eth()
            .block(block_number.into())
            .await
            .context(format!(
                "{} client: Failed to fetch block",
                T::transport_protocol()
            ))
            .and_then(|opt_block| {
                opt_block.ok_or_else(|| {
                    anyhow::Error::msg(format!(
                        "{} client: Getting ETH block for block number {} returned None",
                        T::transport_protocol(),
                        block_number,
                    ))
                })
            })
    }
}

impl EthWsRpcClient {
    pub async fn new(eth_settings: &settings::Eth, logger: &slog::Logger) -> Result<Self> {
        let ws_node_endpoint = &eth_settings.ws_node_endpoint;
        redact_and_log_node_endpoint(
            ws_node_endpoint,
            web3::transports::WebSocket::transport_protocol(),
            logger,
        );
        let web3 = tokio::time::timeout(ETH_NODE_CONNECTION_TIMEOUT, async {
            Ok(web3::Web3::new(
                web3::transports::WebSocket::new(ws_node_endpoint)
                    .await
                    .context(here!())?,
            ))
        })
        // Flatten the Result<Result<>> returned by timeout()
        .map_err(anyhow::Error::new)
        .and_then(|x| async { x })
        // Make sure the eth node is fully synced
        .and_then(|web3| async {
            while let SyncState::Syncing(info) = web3
                .eth()
                .syncing()
                .await
                .context("Failure while syncing EthRpcClient client")?
            {
                slog::info!(
                    logger,
                    "Waiting for ETH node to sync. Sync state is: {:?}. Checking again in {:?} ...",
                    info,
                    SYNC_POLL_INTERVAL
                );
                tokio::time::sleep(SYNC_POLL_INTERVAL).await;
            }
            slog::info!(logger, "ETH node is synced.");
            Ok(web3)
        })
        .await?;

        Ok(Self { web3 })
    }
}

#[async_trait]
pub trait EthWsRpcApi {
    async fn subscribe_new_heads(
        &self,
    ) -> Result<SubscriptionStream<web3::transports::WebSocket, BlockHeader>>;
}

#[async_trait]
impl EthWsRpcApi for EthWsRpcClient {
    async fn subscribe_new_heads(
        &self,
    ) -> Result<SubscriptionStream<web3::transports::WebSocket, BlockHeader>> {
        self.web3
            .eth_subscribe()
            .subscribe_new_heads()
            .await
            .context("Failed to subscribe to new heads with WS Client")
    }
}

impl EthHttpRpcClient {
    pub fn new(eth_settings: &settings::Eth, logger: &slog::Logger) -> Result<Self> {
        let http_node_endpoint = &eth_settings.http_node_endpoint;
        redact_and_log_node_endpoint(
            http_node_endpoint,
            web3::transports::Http::transport_protocol(),
            logger,
        );
        let web3 = web3::Web3::new(
            web3::transports::Http::new(http_node_endpoint)
                .context("Failed to create HTTP Transport for web3 client")?,
        );

        Ok(Self { web3 })
    }
}

#[async_trait]
pub trait EthHttpRpcApi {
    async fn block_number(&self) -> Result<U64>;
}

#[async_trait]
impl EthHttpRpcApi for EthHttpRpcClient {
    async fn block_number(&self) -> Result<U64> {
        self.web3
            .eth()
            .block_number()
            .await
            .context("Failed to fetch block number with HTTP client")
    }
}

#[derive(Clone)]
pub struct EthDualRpcClient {
    ws_client: EthWsRpcClient,
    http_client: EthHttpRpcClient,
}

impl EthDualRpcClient {
    pub fn new(ws_client: EthWsRpcClient, http_client: EthHttpRpcClient) -> Self {
        Self {
            ws_client,
            http_client,
        }
    }
}

macro_rules! dual_call_rpc {
    ($eth_dual:expr, $method:ident, $($arg:expr),*) => {
        {
            let ws_request = $eth_dual.ws_client.$method($($arg.clone()),*);
            let http_request = $eth_dual.http_client.$method($($arg.clone()),*);

            tokio::time::timeout(ETH_DUAL_REQUEST_TIMEOUT, select_ok([ws_request, http_request]))
                .await
                .context("ETH Dual RPC request timed out")?
                .context("ETH Dual RPC request failed")
                .map(|x| x.0)
        }
    };
}

#[async_trait]
impl EthRpcApi for EthDualRpcClient {
    async fn estimate_gas(&self, req: CallRequest, block: Option<BlockNumber>) -> Result<U256> {
        dual_call_rpc!(self, estimate_gas, req, block)
    }

    async fn sign_transaction(
        &self,
        tx: TransactionParameters,
        key: &SecretKey,
    ) -> Result<SignedTransaction> {
        dual_call_rpc!(self, sign_transaction, tx, &key)
    }

    async fn send_raw_transaction(&self, rlp: Bytes) -> Result<H256> {
        dual_call_rpc!(self, send_raw_transaction, rlp)
    }

    async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>> {
        dual_call_rpc!(self, get_logs, filter)
    }

    async fn chain_id(&self) -> Result<U256> {
        dual_call_rpc!(self, chain_id,)
    }

    async fn transaction(&self, tx_hash: H256) -> Result<Transaction> {
        dual_call_rpc!(self, transaction, tx_hash)
    }

    async fn block(&self, block_number: U64) -> Result<Block<H256>> {
        dual_call_rpc!(self, block, block_number)
    }
}

#[cfg(test)]
pub mod mocks {
    use super::*;

    use mockall::mock;
    use sp_core::H256;
    use web3::types::{Block, Bytes, Filter, Log, Transaction};

    mock!(

        // becomes MockEthHttpRpcClient
        pub EthHttpRpcClient {}

        #[async_trait]
        impl EthHttpRpcApi for EthHttpRpcClient {
            async fn block_number(&self) -> Result<U64>;
        }

        #[async_trait]
        impl EthRpcApi for EthHttpRpcClient {
            async fn estimate_gas(&self, req: CallRequest, block: Option<BlockNumber>) -> Result<U256>;

            async fn sign_transaction(
                &self,
                tx: TransactionParameters,
                key: &SecretKey,
            ) -> Result<SignedTransaction>;

            async fn send_raw_transaction(&self, rlp: Bytes) -> Result<H256>;

            async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>>;

            async fn chain_id(&self) -> Result<U256>;

            async fn transaction(&self, tx_hash: H256) -> Result<Transaction>;

            async fn block(&self, block_number: U64) -> Result<Block<H256>>;
        }
    );
}
