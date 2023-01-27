pub mod chain_data_witnesser;
pub mod contract_witnesser;
pub mod erc20_witnesser;
pub mod eth_block_witnessing;
mod http_safe_stream;
pub mod ingress_witnesser;
pub mod key_manager;
mod merged_block_stream;
pub mod stake_manager;

pub mod event;

mod ws_safe_stream;

pub mod rpc;

pub mod utils;
pub mod witnessing;

use anyhow::{anyhow, Context, Result};

use cf_primitives::EpochIndex;
use regex::Regex;

use crate::{
	common::read_clean_and_decode_hex_str_file,
	constants::ETH_BLOCK_SAFETY_MARGIN,
	eth::{
		http_safe_stream::{safe_polling_http_head_stream, HTTP_POLL_INTERVAL},
		merged_block_stream::merged_block_stream,
		rpc::{EthDualRpcClient, EthRpcApi, EthWsRpcApi},
		ws_safe_stream::safe_ws_head_stream,
	},
	logging::COMPONENT_KEY,
	settings,
	state_chain_observer::client::extrinsic_api::ExtrinsicApi,
	witnesser::{block_head_stream_from::block_head_stream_from, BlockNumberable},
};

use slog::o;
use std::{
	fmt::{self, Debug},
	pin::Pin,
	str::FromStr,
	sync::Arc,
};
use thiserror::Error;
use web3::{
	ethabi::{self, Address, Contract},
	signing::{Key, SecretKeyRef},
	types::{Block, Bytes, CallRequest, TransactionParameters, H160, H2048, H256, U256, U64},
};
use web3_secp256k1::SecretKey;

use tokio_stream::Stream;

use event::Event;

use async_trait::async_trait;

#[derive(Debug, PartialEq, Eq)]
pub struct EthNumberBloom {
	pub block_number: U64,
	pub logs_bloom: H2048,
	pub base_fee_per_gas: U256,
}

impl BlockNumberable for EthNumberBloom {
	type BlockNumber = u64;

	fn block_number(&self) -> Self::BlockNumber {
		self.block_number.as_u64()
	}
}

fn web3_u256(x: sp_core::U256) -> web3::types::U256 {
	web3::types::U256(x.0)
}

fn core_h256(h: web3::types::H256) -> sp_core::H256 {
	h.0.into()
}

fn web3_h160(h: sp_core::H160) -> web3::types::H160 {
	h.0.into()
}

pub fn core_h160(h: web3::types::H160) -> sp_core::H160 {
	h.0.into()
}

const EIP1559_TX_ID: u64 = 2;

#[derive(Error, Debug)]
pub enum EventParseError {
	#[error("Unexpected event signature in log subscription: {0:?}")]
	UnexpectedEvent(H256),
	#[error("Cannot decode missing parameter: '{0}'.")]
	MissingParam(String),
}

// The signature is recalculated on each Event::signature() call, so we use this structure to cache
// the signature
pub struct SignatureAndEvent {
	pub signature: H256,
	pub event: ethabi::Event,
}
impl SignatureAndEvent {
	pub fn new(contract: &Contract, name: &str) -> Result<Self> {
		let event = contract.event(name)?;
		Ok(Self { signature: event.signature(), event: event.clone() })
	}
}

/// Helper that generates a broadcast channel with multiple receivers.
pub fn build_broadcast_channel<T: Clone, const S: usize>(
	capacity: usize,
) -> (async_broadcast::Sender<T>, [async_broadcast::Receiver<T>; S]) {
	let (sender, receiver) = async_broadcast::broadcast(capacity);
	(sender, [0; S].map(|_| receiver.clone()))
}

impl TryFrom<Block<H256>> for EthNumberBloom {
	type Error = anyhow::Error;

	fn try_from(block: Block<H256>) -> Result<Self, Self::Error> {
		if block.number.is_none() || block.logs_bloom.is_none() || block.base_fee_per_gas.is_none()
		{
			Err(anyhow!(
                "Block<H256> did not contain necessary block number and/or logs bloom and/or base fee per gas",
            ))
		} else {
			Ok(EthNumberBloom {
				block_number: block.number.unwrap(),
				logs_bloom: block.logs_bloom.unwrap(),
				base_fee_per_gas: block.base_fee_per_gas.unwrap(),
			})
		}
	}
}

/// Enables ETH event streaming via the `Web3` client and signing & broadcasting of txs
#[derive(Clone)]
pub struct EthBroadcaster<EthRpc>
where
	EthRpc: EthRpcApi,
{
	eth_rpc: EthRpc,
	secret_key: SecretKey,
	pub address: Address,
	logger: slog::Logger,
}

impl<EthRpc> EthBroadcaster<EthRpc>
where
	EthRpc: EthRpcApi,
{
	pub fn new(
		eth_settings: &settings::Eth,
		eth_rpc: EthRpc,
		logger: &slog::Logger,
	) -> Result<Self> {
		let secret_key = read_clean_and_decode_hex_str_file(
			&eth_settings.private_key_file,
			"Ethereum Private Key",
			|key| SecretKey::from_str(key).map_err(anyhow::Error::new),
		)?;
		Ok(Self {
			eth_rpc,
			secret_key,
			address: SecretKeyRef::new(&secret_key).address(),
			logger: logger.new(o!(COMPONENT_KEY => "EthBroadcaster")),
		})
	}

	#[cfg(test)]
	pub fn new_test(eth_rpc: EthRpc, logger: &slog::Logger) -> Self {
		// just a fake key
		let secret_key =
			SecretKey::from_str("000000000000000000000000000000000000000000000000000000000000aaaa")
				.unwrap();
		Self {
			eth_rpc,
			secret_key,
			address: SecretKeyRef::new(&secret_key).address(),
			logger: logger.new(o!(COMPONENT_KEY => "EthBroadcaster")),
		}
	}

	/// Encode and sign a transaction.
	pub async fn encode_and_sign_tx(
		&self,
		unsigned_tx: cf_chains::eth::Transaction,
	) -> Result<Bytes> {
		let tx_params = TransactionParameters {
			to: Some(web3_h160(unsigned_tx.contract)),
			data: unsigned_tx.data.clone().into(),
			chain_id: Some(unsigned_tx.chain_id),
			value: web3_u256(unsigned_tx.value),
			max_fee_per_gas: unsigned_tx.max_fee_per_gas.map(web3_u256),
			max_priority_fee_per_gas: unsigned_tx.max_priority_fee_per_gas.map(web3_u256),
			transaction_type: Some(web3::types::U64::from(EIP1559_TX_ID)),
			gas: {
				let gas_estimate = match unsigned_tx.gas_limit {
					None => {
						// query for the gas estimate if the SC didn't provide it
						let zero = Some(U256::from(0u64));
						let call_request = CallRequest {
							from: None,
							to: Some(web3_h160(unsigned_tx.contract)),
							// Set the gas really high (~half gas in a block) for the estimate,
							// since the estimation call requires you to input at least as much gas
							// as the estimate will return
							gas: Some(U256::from(15_000_000u64)),
							gas_price: None,
							value: Some(web3_u256(unsigned_tx.value)),
							data: Some(unsigned_tx.data.clone().into()),
							transaction_type: Some(web3::types::U64::from(EIP1559_TX_ID)),
							// Set the gas prices to zero for the estimate, so we don't get
							// rejected for not having enough ETH
							max_fee_per_gas: zero,
							max_priority_fee_per_gas: zero,
							..Default::default()
						};

						self.eth_rpc
							.estimate_gas(call_request)
							.await
							.context("Failed to estimate gas")?
					},
					Some(gas_limit) => web3_u256(gas_limit),
				};
				// increase the estimate by 50%
				let gas = gas_estimate
					.saturating_mul(U256::from(3u64))
					.checked_div(U256::from(2u64))
					.unwrap();

				slog::debug!(
					self.logger,
					"Gas estimate for unsigned tx: {:?} is {}. Setting 50% higher at: {}",
					unsigned_tx,
					gas_estimate,
					gas
				);

				gas
			},
			..Default::default()
		};

		Ok(self
			.eth_rpc
			.sign_transaction(tx_params, &self.secret_key)
			.await
			.context("Failed to sign ETH transaction")?
			.raw_transaction)
	}

	/// Broadcast a transaction to the network
	pub async fn send(&self, raw_signed_tx: Vec<u8>) -> Result<H256> {
		self.eth_rpc
			.send_raw_transaction(raw_signed_tx.into())
			.await
			.context("Failed to broadcast ETH transaction to network")
	}
}

// Used to zip on the streams, so we know which stream is returning
#[derive(Clone, PartialEq, Eq, Debug, Copy)]
pub enum TransportProtocol {
	Http,
	Ws,
}

impl fmt::Display for TransportProtocol {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			TransportProtocol::Ws => write!(f, "WebSocket"),
			TransportProtocol::Http => write!(f, "HTTP"),
		}
	}
}

/// Contains empty vec when no interesting block items
/// Ok if *all* the relevant items of that block processed successfully, Error if the request
/// to retrieve the block items failed, or the processing failed.
#[derive(Debug)]
pub struct BlockWithProcessedItems<BlockItem: Debug> {
	pub block_number: u64,
	pub processed_block_items: Result<Vec<BlockItem>>,
}

/// Just contains an empty vec if there are no events
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct BlockWithItems<BlockItem: Debug> {
	pub block_number: u64,
	pub block_items: Vec<BlockItem>,
}

/// Same as [`safe_dual_block_subscription`] but prepends the stream with
/// historical blocks from a given block number.
pub async fn safe_dual_block_subscription_from(
	from_block: u64,
	eth_dual_rpc: EthDualRpcClient,
	logger: &slog::Logger,
) -> Result<Pin<Box<dyn Stream<Item = EthNumberBloom> + Send + 'static>>> {
	let safe_head_stream = safe_dual_block_subscription(eth_dual_rpc.clone(), logger).await?;

	block_head_stream_from(
		from_block,
		safe_head_stream,
		move |block_number| {
			let eth_rpc = eth_dual_rpc.clone();
			Box::pin(async move {
				eth_rpc.block(U64::from(block_number)).await.and_then(|block| {
					let number_bloom: Result<EthNumberBloom> =
						block.try_into().context("Failed to convert Block to EthNumberBloom");
					number_bloom
				})
			})
		},
		logger,
	)
	.await
}

/// Returns a safe stream of blocks from the latest block onward,
/// using a dual (WS/HTTP) rpc subscription.
async fn safe_dual_block_subscription(
	eth_dual_rpc: EthDualRpcClient,
	logger: &slog::Logger,
) -> Result<Pin<Box<dyn Stream<Item = EthNumberBloom> + Send + 'static>>>
where
{
	let safe_ws_head_stream = safe_ws_head_stream(
		eth_dual_rpc.ws_client.subscribe_new_heads().await?,
		ETH_BLOCK_SAFETY_MARGIN,
		logger,
	);

	let safe_http_head_stream = safe_polling_http_head_stream(
		eth_dual_rpc.http_client.clone(),
		HTTP_POLL_INTERVAL,
		ETH_BLOCK_SAFETY_MARGIN,
		logger,
	)
	.await;

	merged_block_stream(safe_ws_head_stream, safe_http_head_stream, logger.clone()).await
}

#[async_trait]
pub trait EthContractWitnesser {
	type EventParameters: Debug + Send + Sync + 'static;

	fn contract_name(&self) -> String;

	fn decode_log_closure(&self) -> Result<DecodeLogClosure<Self::EventParameters>>;

	async fn handle_block_events<StateChainClient, EthRpcClient>(
		&mut self,
		epoch: EpochIndex,
		block_number: u64,
		block: BlockWithItems<Event<Self::EventParameters>>,
		state_chain_client: Arc<StateChainClient>,
		eth_rpc: &EthRpcClient,
		logger: &slog::Logger,
	) -> anyhow::Result<()>
	where
		EthRpcClient: EthRpcApi + Sync + Send,
		StateChainClient: ExtrinsicApi + Send + Sync;

	fn contract_address(&self) -> H160;
}

pub type DecodeLogClosure<EventParameters> =
	Box<dyn Fn(H256, ethabi::RawLog) -> Result<EventParameters> + Send + Sync + 'static>;

const MAX_SECRET_CHARACTERS_REVEALED: usize = 3;
const SCHEMA_PADDING_LEN: usize = 3;

/// Partially redacts the secret in the url of the node endpoint.
///  eg: `wss://cdcd639308194d3f977a1a5a7ff0d545.rinkeby.ws.rivet.cloud/` ->
/// `wss://cdc****.rinkeby.ws.rivet.cloud/`
fn redact_secret_eth_node_endpoint(endpoint: &str) -> Result<String> {
	let re = Regex::new(r"[0-9a-fA-F]{32}").unwrap();
	if re.is_match(endpoint) {
		// A 32 character hex string was found, redact it
		let mut endpoint_redacted = endpoint.to_string();
		for capture in re.captures_iter(endpoint) {
			endpoint_redacted = endpoint_redacted.replace(
				&capture[0],
				&format!(
					"{}****",
					&capture[0].split_at(capture[0].len().min(MAX_SECRET_CHARACTERS_REVEALED)).0
				),
			);
		}
		Ok(endpoint_redacted)
	} else {
		// No secret was found, so just redact almost all of the url
		let url = url::Url::parse(endpoint)
			.map_err(anyhow::Error::msg)
			.with_context(|| "Failed to parse node endpoint into a URL")?;
		Ok(format!(
			"{}****",
			endpoint
				.split_at(usize::min(
					url.scheme().len() + SCHEMA_PADDING_LEN + MAX_SECRET_CHARACTERS_REVEALED,
					endpoint.len()
				))
				.0
		))
	}
}

#[cfg(test)]
mod tests {
	use crate::logging::test_utils::new_test_logger;

	use super::{rpc::MockEthRpcApi, *};
	#[test]
	fn cfg_test_create_eth_broadcaster_works() {
		let eth_rpc_api_mock = MockEthRpcApi::new();
		let logger = new_test_logger();
		EthBroadcaster::new_test(eth_rpc_api_mock, &logger);
	}

	#[test]
	fn test_secret_web_addresses() {
		assert_eq!(
			redact_secret_eth_node_endpoint(
				"wss://mainnet.infura.io/ws/v3/d52c362116b640b98a166d08d3170a42"
			)
			.unwrap(),
			"wss://mainnet.infura.io/ws/v3/d52****"
		);
		assert_eq!(
			redact_secret_eth_node_endpoint(
				"wss://cdcd639308194d3f977a1a5a7ff0d545.rinkeby.ws.rivet.cloud/"
			)
			.unwrap(),
			"wss://cdc****.rinkeby.ws.rivet.cloud/"
		);
		// same, but HTTP
		assert_eq!(
			redact_secret_eth_node_endpoint(
				"https://cdcd639308194d3f977a1a5a7ff0d545.rinkeby.rpc.rivet.cloud/"
			)
			.unwrap(),
			"https://cdc****.rinkeby.rpc.rivet.cloud/"
		);
		assert_eq!(
			redact_secret_eth_node_endpoint("wss://non_32hex_secret.rinkeby.ws.rivet.cloud/")
				.unwrap(),
			"wss://non****"
		);
		assert_eq!(redact_secret_eth_node_endpoint("wss://a").unwrap(), "wss://a****");
		// same, but HTTP
		assert_eq!(redact_secret_eth_node_endpoint("http://a").unwrap(), "http://a****");
		assert!(redact_secret_eth_node_endpoint("no.schema.com").is_err());
	}
}
