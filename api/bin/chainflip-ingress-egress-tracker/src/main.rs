#![feature(async_fn_in_trait)]
use async_trait::async_trait;
use cf_chains::{
	address::ToHumanreadableAddress, evm::SchnorrVerificationComponents, AnyChain, Bitcoin, Chain,
	Ethereum, Polkadot,
};
use cf_primitives::{Asset, BroadcastId, ForeignChain, NetworkEnvironment};
use chainflip_engine::{
	settings::{insert_command_line_option, CfSettings, HttpBasicAuthEndpoint, WsHttpEndpoints},
	state_chain_observer::client::{
		chain_api::ChainApi, storage_api::StorageApi, STATE_CHAIN_CONNECTION,
	},
};
use clap::Parser;
use config::{Config, ConfigBuilder, ConfigError, Environment, Map, Source, Value};
use futures::FutureExt;
use pallet_cf_broadcast::TransactionOutIdFor;
use pallet_cf_ingress_egress::DepositWitness;
use redis::{aio::MultiplexedConnection, AsyncCommands};
use serde::{Deserialize, Serialize};
use state_chain_runtime::PalletInstanceAlias;
use std::{collections::HashMap, env, ops::Deref};
use tracing::log;
use utilities::{rpc::NumberOrHex, task_scope};

mod witnessing;

#[async_trait]
pub trait Store: Sync + Send + 'static {
	type Output: Sync + Send + 'static;

	async fn save_to_array<S: Storable>(&mut self, storable: &S) -> anyhow::Result<Self::Output>;
	async fn save_singleton<S: Storable>(&mut self, storable: &S) -> anyhow::Result<Self::Output>;
}

#[derive(Clone)]
struct RedisStore {
	con: MultiplexedConnection,
}

impl RedisStore {
	const REDIS_EXPIRY_IN_SECONDS: u64 = 3600;

	fn new(con: MultiplexedConnection) -> Self {
		Self { con }
	}
}

#[async_trait]
impl Store for RedisStore {
	type Output = ();

	async fn save_to_array<S: Storable>(&mut self, storable: &S) -> anyhow::Result<()> {
		let key = storable.get_key();
		self.con.rpush(&key, serde_json::to_string(storable)?).await?;
		self.con.expire(key, Self::REDIS_EXPIRY_IN_SECONDS as i64).await?;

		Ok(())
	}

	async fn save_singleton<S: Storable>(&mut self, storable: &S) -> anyhow::Result<()> {
		self.con
			.set_ex(
				storable.get_key(),
				serde_json::to_string(storable)?,
				Self::REDIS_EXPIRY_IN_SECONDS,
			)
			.await?;

		Ok(())
	}
}

#[async_trait]
pub trait Storable: Serialize + Sized + Sync + 'static {
	fn get_key(&self) -> String;

	async fn save_to_array<S: Store>(&self, store: &mut S) -> anyhow::Result<S::Output> {
		store.save_to_array(self).await
	}

	async fn save_singleton<S: Store>(&self, store: &mut S) -> anyhow::Result<S::Output> {
		store.save_singleton(self).await
	}
}

#[derive(Serialize)]
struct WitnessAsset {
	chain: ForeignChain,
	asset: Asset,
}

impl From<cf_chains::assets::eth::Asset> for WitnessAsset {
	fn from(asset: cf_chains::assets::eth::Asset) -> Self {
		match asset {
			cf_chains::assets::eth::Asset::Eth |
			cf_chains::assets::eth::Asset::Flip |
			cf_chains::assets::eth::Asset::Usdc =>
				Self { chain: ForeignChain::Ethereum, asset: asset.into() },
		}
	}
}

impl From<cf_chains::assets::dot::Asset> for WitnessAsset {
	fn from(asset: cf_chains::assets::dot::Asset) -> Self {
		match asset {
			cf_chains::assets::dot::Asset::Dot =>
				Self { chain: ForeignChain::Polkadot, asset: asset.into() },
		}
	}
}

impl From<cf_chains::assets::btc::Asset> for WitnessAsset {
	fn from(asset: cf_chains::assets::btc::Asset) -> Self {
		match asset {
			cf_chains::assets::btc::Asset::Btc =>
				Self { chain: ForeignChain::Bitcoin, asset: asset.into() },
		}
	}
}

trait ToChainStr {
	fn to_chain_str(&self) -> &'static str;
}

impl ToChainStr for ForeignChain {
	fn to_chain_str(&self) -> &'static str {
		match self {
			ForeignChain::Ethereum => "ethereum",
			ForeignChain::Polkadot => "polkadot",
			ForeignChain::Bitcoin => "bitcoin",
		}
	}
}

trait ToForeignChain {
	fn to_foreign_chain(&self) -> ForeignChain;
}

#[derive(Serialize)]
#[serde(tag = "deposit_chain")]
enum TransactionId {
	Bitcoin { hash: String },
	Ethereum { signature: SchnorrVerificationComponents },
	Polkadot { signature: String },
}

impl ToForeignChain for TransactionId {
	fn to_foreign_chain(&self) -> ForeignChain {
		match self {
			Self::Bitcoin { .. } => ForeignChain::Bitcoin,
			Self::Ethereum { .. } => ForeignChain::Ethereum,
			Self::Polkadot { .. } => ForeignChain::Polkadot,
		}
	}
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WitnessInformation {
	Deposit {
		deposit_chain_block_height: <AnyChain as Chain>::ChainBlockNumber,
		#[serde(skip_serializing)]
		deposit_address: String,
		amount: NumberOrHex,
		asset: WitnessAsset,
	},
	Broadcast {
		broadcast_id: BroadcastId,
		tx_out_id: TransactionId,
	},
}

impl ToForeignChain for WitnessInformation {
	fn to_foreign_chain(&self) -> ForeignChain {
		match self {
			Self::Deposit { asset, .. } => asset.chain,
			Self::Broadcast { tx_out_id, .. } => tx_out_id.to_foreign_chain(),
		}
	}
}

impl Storable for WitnessInformation {
	fn get_key(&self) -> String {
		let chain = self.to_foreign_chain().to_chain_str();

		match self {
			Self::Deposit { deposit_address, .. } => {
				format!("deposit:{chain}:{deposit_address}")
			},
			Self::Broadcast { broadcast_id, .. } => {
				format!("broadcast:{chain}:{broadcast_id}")
			},
		}
	}
}

fn hex_encode_bytes(bytes: &[u8]) -> String {
	format!("0x{}", hex::encode(bytes))
}

type DepositInfo<T> = (DepositWitness<T>, <T as Chain>::ChainBlockNumber, NetworkEnvironment);

impl From<DepositInfo<Ethereum>> for WitnessInformation {
	fn from((value, height, _): DepositInfo<Ethereum>) -> Self {
		Self::Deposit {
			deposit_chain_block_height: height,
			deposit_address: hex_encode_bytes(value.deposit_address.as_bytes()),
			amount: value.amount.into(),
			asset: value.asset.into(),
		}
	}
}

impl From<DepositInfo<Bitcoin>> for WitnessInformation {
	fn from((value, height, network): DepositInfo<Bitcoin>) -> Self {
		Self::Deposit {
			deposit_chain_block_height: height,
			deposit_address: value.deposit_address.to_humanreadable(network),
			amount: value.amount.into(),
			asset: value.asset.into(),
		}
	}
}

impl From<DepositInfo<Polkadot>> for WitnessInformation {
	fn from((value, height, _): DepositInfo<Polkadot>) -> Self {
		Self::Deposit {
			deposit_chain_block_height: height as u64,
			deposit_address: hex_encode_bytes(value.deposit_address.aliased_ref()),
			amount: value.amount.into(),
			asset: value.asset.into(),
		}
	}
}

async fn get_broadcast_id<I, StateChainClient>(
	state_chain_client: &StateChainClient,
	tx_out_id: &TransactionOutIdFor<state_chain_runtime::Runtime, I::Instance>,
) -> Option<BroadcastId>
where
	state_chain_runtime::Runtime: pallet_cf_broadcast::Config<I::Instance>,
	I: PalletInstanceAlias + 'static,
	StateChainClient: StorageApi + ChainApi + 'static + Send + Sync,
{
	let id = state_chain_client
		.storage_map_entry::<pallet_cf_broadcast::TransactionOutIdToBroadcastId<
			state_chain_runtime::Runtime,
			I::Instance,
		>>(state_chain_client.latest_unfinalized_block().hash, tx_out_id)
		.await
		.expect(STATE_CHAIN_CONNECTION)
		.map(|(broadcast_id, _)| broadcast_id);

	if id.is_none() {
		log::warn!("Broadcast ID not found for {:?}", tx_out_id);
	}

	id
}

#[derive(Clone, Deserialize, Debug)]
pub struct DepositTrackerSettings {
	eth: WsHttpEndpoints,
	dot: WsHttpEndpoints,
	state_chain_ws_endpoint: String,
	btc: HttpBasicAuthEndpoint,
	redis_url: String,
}

#[derive(Parser, Debug, Clone, Default)]
#[clap(version = env!("SUBSTRATE_CLI_IMPL_VERSION"), version_short = 'v')]
pub struct TrackerOptions {
	#[clap(long = "eth.rpc.ws_endpoint")]
	eth_ws_endpoint: Option<String>,
	#[clap(long = "eth.rpc.http_endpoint")]
	eth_http_endpoint: Option<String>,
	#[clap(long = "dot.rpc.ws_endpoint")]
	dot_ws_endpoint: Option<String>,
	#[clap(long = "dot.rpc.http_endpoint")]
	dot_http_endpoint: Option<String>,
	#[clap(long = "state_chain.ws_endpoint")]
	state_chain_ws_endpoint: Option<String>,
	#[clap(long = "btc.rpc.http_endpoint")]
	btc_endpoint: Option<String>,
	#[clap(long = "btc.rpc.basic_auth_user")]
	btc_username: Option<String>,
	#[clap(long = "btc.rpc.basic_auth_password")]
	btc_password: Option<String>,
	#[clap(long = "redis_url")]
	redis_url: Option<String>,
}

impl CfSettings for DepositTrackerSettings {
	type CommandLineOptions = TrackerOptions;

	fn load_settings_from_all_sources(
		config_root: String,
		_settings_dir: &str,
		opts: Self::CommandLineOptions,
	) -> Result<Self, ConfigError> {
		Self::set_defaults(Config::builder(), &config_root)?
			.add_source(Environment::default().separator("__"))
			.add_source(opts)
			.build()?
			.try_deserialize()
	}

	fn set_defaults(
		config_builder: ConfigBuilder<config::builder::DefaultState>,
		_config_root: &str,
	) -> Result<ConfigBuilder<config::builder::DefaultState>, ConfigError> {
		// These defaults are for a localnet setup
		config_builder
			.set_default("eth.ws_endpoint", "ws://localhost:8546")?
			.set_default("eth.http_endpoint", "http://localhost:8545")?
			.set_default("dot.ws_endpoint", "ws://localhost:9947")?
			.set_default("dot.http_endpoint", "http://localhost:9947")?
			.set_default("state_chain_ws_endpoint", "ws://localhost:9944")?
			.set_default("btc.http_endpoint", "http://127.0.0.1:8332")?
			.set_default("btc.basic_auth_user", "flip")?
			.set_default("btc.basic_auth_password", "flip")?
			.set_default("redis_url", "http://127.0.0.1:6379")
	}

	fn validate_settings(
		&mut self,
		_config_root: &std::path::Path,
	) -> anyhow::Result<(), ConfigError> {
		Ok(())
	}
}

impl Source for TrackerOptions {
	fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
		Box::new((*self).clone())
	}

	fn collect(&self) -> std::result::Result<Map<String, Value>, ConfigError> {
		let mut map: HashMap<String, Value> = HashMap::new();

		insert_command_line_option(&mut map, "eth.ws_endpoint", &self.eth_ws_endpoint);
		insert_command_line_option(&mut map, "eth.http_endpoint", &self.eth_http_endpoint);
		insert_command_line_option(&mut map, "dot.ws_endpoint", &self.dot_ws_endpoint);
		insert_command_line_option(&mut map, "dot.http_endpoint", &self.dot_http_endpoint);
		insert_command_line_option(
			&mut map,
			"state_chain_ws_endpoint",
			&self.state_chain_ws_endpoint,
		);
		insert_command_line_option(&mut map, "btc.http_endpoint", &self.btc_endpoint);
		insert_command_line_option(&mut map, "btc.basic_auth_user", &self.btc_username);
		insert_command_line_option(&mut map, "btc.basic_auth_password", &self.btc_password);
		insert_command_line_option(&mut map, "redis_url", &self.redis_url);

		Ok(map)
	}
}

async fn handle_call<S, StateChainClient>(
	call: state_chain_runtime::RuntimeCall,
	store: &mut S,
	chainflip_network: NetworkEnvironment,
	state_chain_client: &StateChainClient,
) -> anyhow::Result<()>
where
	S: Store,
	StateChainClient: StorageApi + ChainApi + 'static + Send + Sync,
{
	use pallet_cf_broadcast::Call as BroadcastCall;
	use pallet_cf_ingress_egress::Call as IngressEgressCall;
	use state_chain_runtime::RuntimeCall::*;

	match call {
		EthereumIngressEgress(IngressEgressCall::process_deposits {
			deposit_witnesses,
			block_height,
		}) =>
			for witness in deposit_witnesses as Vec<DepositWitness<Ethereum>> {
				WitnessInformation::from((witness, block_height, chainflip_network))
					.save_to_array(store)
					.await?;
			},
		BitcoinIngressEgress(IngressEgressCall::process_deposits {
			deposit_witnesses,
			block_height,
		}) =>
			for witness in deposit_witnesses as Vec<DepositWitness<Bitcoin>> {
				WitnessInformation::from((witness, block_height, chainflip_network))
					.save_to_array(store)
					.await?;
			},
		PolkadotIngressEgress(IngressEgressCall::process_deposits {
			deposit_witnesses,
			block_height,
		}) =>
			for witness in deposit_witnesses as Vec<DepositWitness<Polkadot>> {
				WitnessInformation::from((witness, block_height, chainflip_network))
					.save_to_array(store)
					.await?;
			},
		EthereumBroadcaster(BroadcastCall::transaction_succeeded { tx_out_id, .. }) => {
			let broadcast_id =
				get_broadcast_id::<Ethereum, StateChainClient>(state_chain_client, &tx_out_id)
					.await;

			if let Some(broadcast_id) = broadcast_id {
				WitnessInformation::Broadcast {
					broadcast_id,
					tx_out_id: TransactionId::Ethereum { signature: tx_out_id },
				}
				.save_singleton(store)
				.await?;
			}
		},
		BitcoinBroadcaster(BroadcastCall::transaction_succeeded { tx_out_id, .. }) => {
			let broadcast_id =
				get_broadcast_id::<Bitcoin, StateChainClient>(state_chain_client, &tx_out_id).await;

			if let Some(broadcast_id) = broadcast_id {
				WitnessInformation::Broadcast {
					broadcast_id,
					tx_out_id: TransactionId::Bitcoin {
						hash: format!("0x{}", hex::encode(tx_out_id)),
					},
				}
				.save_singleton(store)
				.await?;
			}
		},
		PolkadotBroadcaster(BroadcastCall::transaction_succeeded { tx_out_id, .. }) => {
			let broadcast_id =
				get_broadcast_id::<Polkadot, StateChainClient>(state_chain_client, &tx_out_id)
					.await;

			if let Some(broadcast_id) = broadcast_id {
				WitnessInformation::Broadcast {
					broadcast_id,
					tx_out_id: TransactionId::Polkadot {
						signature: format!("0x{}", hex::encode(tx_out_id.aliased_ref())),
					},
				}
				.save_singleton(store)
				.await?;
			}
		},

		EthereumIngressEgress(_) |
		BitcoinIngressEgress(_) |
		PolkadotIngressEgress(_) |
		System(_) |
		Timestamp(_) |
		Environment(_) |
		Flip(_) |
		Emissions(_) |
		Funding(_) |
		AccountRoles(_) |
		Witnesser(_) |
		Validator(_) |
		Session(_) |
		Grandpa(_) |
		Governance(_) |
		Reputation(_) |
		TokenholderGovernance(_) |
		EthereumChainTracking(_) |
		BitcoinChainTracking(_) |
		PolkadotChainTracking(_) |
		EthereumVault(_) |
		PolkadotVault(_) |
		BitcoinVault(_) |
		EthereumThresholdSigner(_) |
		PolkadotThresholdSigner(_) |
		BitcoinThresholdSigner(_) |
		EthereumBroadcaster(_) |
		PolkadotBroadcaster(_) |
		BitcoinBroadcaster(_) |
		Swapping(_) |
		LiquidityProvider(_) |
		LiquidityPools(_) => {},
	};

	Ok(())
}

async fn start(
	scope: &task_scope::Scope<'_, anyhow::Error>,
	settings: DepositTrackerSettings,
) -> anyhow::Result<()> {
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");

	let client = redis::Client::open(settings.redis_url.clone()).unwrap();
	let mut con = RedisStore::new(client.get_multiplexed_tokio_connection().await?);

	// Broadcast channel will drop old messages when the buffer is full to
	// avoid "memory leaks" due to slow receivers.
	const EVENT_BUFFER_SIZE: usize = 1024;
	let (witness_sender, _) =
		tokio::sync::broadcast::channel::<state_chain_runtime::RuntimeCall>(EVENT_BUFFER_SIZE);

	let (state_chain_client, env_params) =
		witnessing::start(scope, settings.clone(), witness_sender.clone()).await?;
	let btc_network = env_params.chainflip_network.into();
	witnessing::btc_mempool::start(scope, settings.btc, con.clone(), btc_network);
	let mut witness_receiver = witness_sender.subscribe();

	while let Ok(call) = witness_receiver.recv().await {
		handle_call(call, &mut con, env_params.chainflip_network, state_chain_client.deref())
			.await?
	}

	Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let settings = DepositTrackerSettings::load_settings_from_all_sources(
		// Not using the config root or settings dir.
		"".to_string(),
		"",
		TrackerOptions::parse(),
	)?;

	task_scope::task_scope(|scope| async move { start(scope, settings).await }.boxed()).await
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::anyhow;
	use async_trait::async_trait;
	use cf_chains::{
		dot::PolkadotAccountId,
		evm::{EvmTransactionMetadata, TransactionFee},
	};
	use chainflip_engine::state_chain_observer::client::{
		storage_api, BlockInfo, StateChainStreamApi,
	};
	use frame_support::storage::types::QueryKindTrait;
	use jsonrpsee::core::RpcResult;
	use mockall::mock;
	use sp_core::{storage::StorageKey, H160};
	use std::collections::HashMap;

	#[derive(Default)]
	struct MockStore {
		storage: HashMap<String, serde_json::Value>,
	}

	#[async_trait]
	impl Store for MockStore {
		type Output = ();

		async fn save_to_array<S: Storable>(
			&mut self,
			storable: &S,
		) -> anyhow::Result<Self::Output> {
			let key = storable.get_key();
			let value = serde_json::to_value(storable)?;

			let array = self.storage.entry(key).or_insert(serde_json::Value::Array(vec![]));

			array.as_array_mut().ok_or(anyhow!("expect array"))?.push(value);

			Ok(())
		}

		async fn save_singleton<S: Storable>(
			&mut self,
			storable: &S,
		) -> anyhow::Result<Self::Output> {
			let key = storable.get_key();

			let value = serde_json::to_value(storable)?;

			self.storage.insert(key, value);

			Ok(())
		}
	}

	mock! {
		pub StateChainClient {}
		#[async_trait]
		impl ChainApi for StateChainClient {
			fn latest_finalized_block(&self) -> BlockInfo;
			fn latest_unfinalized_block(&self) -> BlockInfo;

			async fn finalized_block_stream(&self) -> Box<dyn StateChainStreamApi>;
			async fn unfinalized_block_stream(&self) -> Box<dyn StateChainStreamApi<false>>;

			async fn block(&self, block_hash: state_chain_runtime::Hash) -> RpcResult<BlockInfo>;
		}

		#[async_trait]
		impl StorageApi for StateChainClient {
			async fn storage_item<
				Value: codec::FullCodec + 'static,
				OnEmpty: 'static,
				QueryKind: QueryKindTrait<Value, OnEmpty> + 'static,
			>(
				&self,
				storage_key: StorageKey,
				block_hash: state_chain_runtime::Hash,
			) -> RpcResult<<QueryKind as QueryKindTrait<Value, OnEmpty>>::Query>;

			async fn storage_value<StorageValue: storage_api::StorageValueAssociatedTypes + 'static>(
				&self,
				block_hash: state_chain_runtime::Hash,
			) -> RpcResult<<StorageValue::QueryKind as QueryKindTrait<StorageValue::Value, StorageValue::OnEmpty>>::Query>;

			async fn storage_map_entry<StorageMap: storage_api::StorageMapAssociatedTypes + 'static>(
				&self,
				block_hash: state_chain_runtime::Hash,
				key: &StorageMap::Key,
			) -> RpcResult<
				<StorageMap::QueryKind as QueryKindTrait<StorageMap::Value, StorageMap::OnEmpty>>::Query,
			>
			where
				StorageMap::Key: Sync;

			async fn storage_double_map_entry<StorageDoubleMap: storage_api::StorageDoubleMapAssociatedTypes + 'static>(
				&self,
				block_hash: state_chain_runtime::Hash,
				key1: &StorageDoubleMap::Key1,
				key2: &StorageDoubleMap::Key2,
			) -> RpcResult<
				<StorageDoubleMap::QueryKind as QueryKindTrait<
					StorageDoubleMap::Value,
					StorageDoubleMap::OnEmpty,
				>>::Query,
			>
			where
				StorageDoubleMap::Key1: Sync,
				StorageDoubleMap::Key2: Sync;

			async fn storage_map<StorageMap: storage_api::StorageMapAssociatedTypes + 'static, ReturnedIter: FromIterator<(<StorageMap as storage_api::StorageMapAssociatedTypes>::Key, StorageMap::Value)> + 'static>(
				&self,
				block_hash: state_chain_runtime::Hash,
			) -> RpcResult<ReturnedIter>;
		}
	}

	fn create_client<I>(
		result: Option<(
			BroadcastId,
			<<state_chain_runtime::Runtime as pallet_cf_broadcast::Config<I::Instance>>::TargetChain as Chain>::ChainBlockNumber,
		)>,
	) -> MockStateChainClient
	where
		state_chain_runtime::Runtime: pallet_cf_broadcast::Config<I::Instance>,
		I: PalletInstanceAlias + 'static,
	{
		let mut client = MockStateChainClient::new();

		client
			.expect_storage_map_entry::<pallet_cf_broadcast::TransactionOutIdToBroadcastId<
				state_chain_runtime::Runtime,
				I::Instance,
			>>()
			.return_once(move |_, _| Ok(result));

		client.expect_latest_unfinalized_block().returning(|| BlockInfo {
			parent_hash: state_chain_runtime::Hash::default(),
			hash: state_chain_runtime::Hash::default(),
			number: 1,
		});

		client
	}

	fn parse_eth_address(address: &'static str) -> (H160, &'static str) {
		let mut eth_address_bytes = [0; 20];

		for (index, byte) in hex::decode(&address[2..]).unwrap().into_iter().enumerate() {
			eth_address_bytes[index] = byte;
		}

		(H160::from(eth_address_bytes), address)
	}

	#[tokio::test]
	async fn test_handle_deposit_calls() {
		let polkadot_address = "14JWPRWMkEyLLdrN2k3teBd446sKKJuwU2DDKw4Ev4dYcHeF";
		let polkadot_account_id = polkadot_address.parse::<PolkadotAccountId>().unwrap();

		let (eth_address1, eth_address_str1) =
			parse_eth_address("0x541f563237A309B3A61E33BDf07a8930Bdba8D99");

		let (eth_address2, eth_address_str2) =
			parse_eth_address("0xa56A6be23b6Cf39D9448FF6e897C29c41c8fbDFF");

		let client = MockStateChainClient::new();
		let mut store = MockStore::default();
		handle_call(
			state_chain_runtime::RuntimeCall::EthereumIngressEgress(
				pallet_cf_ingress_egress::Call::process_deposits {
					deposit_witnesses: vec![DepositWitness {
						deposit_address: eth_address1,
						amount: 100u128,
						asset: cf_chains::assets::eth::Asset::Eth,
						deposit_details: (),
					}],
					block_height: 1,
				},
			),
			&mut store,
			NetworkEnvironment::Testnet,
			&client,
		)
		.await
		.expect("failed to handle call");
		handle_call(
			state_chain_runtime::RuntimeCall::PolkadotIngressEgress(
				pallet_cf_ingress_egress::Call::process_deposits {
					deposit_witnesses: vec![DepositWitness {
						deposit_address: polkadot_account_id,
						amount: 100u128,
						asset: cf_chains::assets::dot::Asset::Dot,
						deposit_details: (),
					}],
					block_height: 1,
				},
			),
			&mut store,
			NetworkEnvironment::Testnet,
			&client,
		)
		.await
		.expect("failed to handle call");
		handle_call(
			state_chain_runtime::RuntimeCall::EthereumIngressEgress(
				pallet_cf_ingress_egress::Call::process_deposits {
					deposit_witnesses: vec![DepositWitness {
						deposit_address: eth_address2,
						amount: 100u128,
						asset: cf_chains::assets::eth::Asset::Eth,
						deposit_details: (),
					}],
					block_height: 1,
				},
			),
			&mut store,
			NetworkEnvironment::Testnet,
			&client,
		)
		.await
		.expect("failed to handle call");

		assert_eq!(store.storage.len(), 3);
		println!("{:?}", store.storage);
		insta::assert_display_snapshot!(store
			.storage
			.get(format!("deposit:ethereum:{}", eth_address_str1.to_lowercase()).as_str())
			.unwrap());
		insta::assert_display_snapshot!(store
			.storage
			.get(
				format!(
					"deposit:polkadot:{}",
					format!("0x{}", hex::encode(polkadot_account_id.aliased_ref()))
				)
				.as_str()
			)
			.unwrap());
		insta::assert_display_snapshot!(store
			.storage
			.get(format!("deposit:ethereum:{}", eth_address_str2.to_lowercase()).as_str())
			.unwrap());

		handle_call(
			state_chain_runtime::RuntimeCall::EthereumIngressEgress(
				pallet_cf_ingress_egress::Call::process_deposits {
					deposit_witnesses: vec![DepositWitness {
						deposit_address: eth_address1,
						amount: 2_000_000u128,
						asset: cf_chains::assets::eth::Asset::Eth,
						deposit_details: (),
					}],
					block_height: 1,
				},
			),
			&mut store,
			NetworkEnvironment::Testnet,
			&client,
		)
		.await
		.expect("failed to handle call");
		assert_eq!(store.storage.len(), 3);
		insta::assert_display_snapshot!(store
			.storage
			.get(format!("deposit:ethereum:{}", eth_address_str1.to_lowercase()).as_str())
			.unwrap());
	}

	#[tokio::test]
	async fn test_handle_broadcast_calls() {
		let (eth_address, _) = parse_eth_address("0x541f563237A309B3A61E33BDf07a8930Bdba8D99");

		let tx_out_id = SchnorrVerificationComponents { s: [0; 32], k_times_g_address: [0; 20] };

		let client = create_client::<Ethereum>(Some((1, 2)));
		let mut store = MockStore::default();
		handle_call(
			state_chain_runtime::RuntimeCall::EthereumBroadcaster(
				pallet_cf_broadcast::Call::transaction_succeeded {
					tx_out_id,
					signer_id: eth_address,
					tx_fee: TransactionFee { gas_used: 0, effective_gas_price: 0 },
					tx_metadata: EvmTransactionMetadata {
						max_fee_per_gas: None,
						max_priority_fee_per_gas: None,
						contract: H160::from([0; 20]),
						gas_limit: None,
					},
				},
			),
			&mut store,
			NetworkEnvironment::Testnet,
			&client,
		)
		.await
		.expect("failed to handle call");

		assert_eq!(store.storage.len(), 1);
		insta::assert_display_snapshot!(store.storage.get("broadcast:ethereum:1").unwrap());
	}
}
