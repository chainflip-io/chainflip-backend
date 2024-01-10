#![feature(async_fn_in_trait)]
use async_trait::async_trait;
use cf_chains::{
	btc::BitcoinNetwork, evm::SchnorrVerificationComponents, AnyChain, Bitcoin, Chain, Ethereum,
	Polkadot,
};
use cf_primitives::{Asset, BroadcastId, ForeignChain};
use chainflip_engine::{
	settings::{insert_command_line_option, CfSettings, HttpBasicAuthEndpoint, WsHttpEndpoints},
	state_chain_observer::client::{
		chain_api::ChainApi, extrinsic_api::unsigned::UnsignedExtrinsicApi,
		storage_api::StorageApi, STATE_CHAIN_CONNECTION,
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
		self.con
			.rpush(&key, serde_json::to_string(storable).expect("failed to serialize redis value"))
			.await?;
		self.con.expire(key, Self::REDIS_EXPIRY_IN_SECONDS as i64).await?;

		Ok(())
	}

	async fn save_singleton<S: Storable>(&mut self, storable: &S) -> anyhow::Result<()> {
		self.con
			.set_ex(
				storable.get_key(),
				serde_json::to_string(storable).expect("failed to serialize redis value"),
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

#[derive(Serialize)]
#[serde(tag = "deposit_chain", rename_all = "snake_case")]
enum TransactionId {
	Bitcoin { hash: String },
	Ethereum { signature: SchnorrVerificationComponents },
	Polkadot { signature: String },
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WitnessInformation {
	Deposit {
		deposit_chain_block_height: <AnyChain as Chain>::ChainBlockNumber,
		deposit_address: String,
		amount: NumberOrHex,
		asset: WitnessAsset,
		chain: ForeignChain,
	},
	Broadcast {
		broadcast_id: BroadcastId,
		tx_out_id: TransactionId,
	},
}

impl Storable for WitnessInformation {
	fn get_key(&self) -> String {
		match self {
			Self::Deposit { deposit_address, chain, .. } => {
				let chain = serde_json::to_string(chain)
					.expect("failed to serialize string")
					.to_lowercase();

				format!("deposit:{chain}:{deposit_address}")
			},
			Self::Broadcast { broadcast_id, tx_out_id } => {
				let chain = match tx_out_id {
					TransactionId::Bitcoin { .. } => "bitcoin",
					TransactionId::Ethereum { .. } => "ethereum",
					TransactionId::Polkadot { .. } => "polkadot",
				};

				format!("broadcast:{chain}:{broadcast_id}")
			},
		}
	}
}

type DepositInfo<T> = (DepositWitness<T>, <T as Chain>::ChainBlockNumber);

impl From<DepositInfo<Ethereum>> for WitnessInformation {
	fn from((value, height): DepositInfo<Ethereum>) -> Self {
		Self::Deposit {
			deposit_chain_block_height: height,
			deposit_address: value.deposit_address.to_string(),
			amount: value.amount.into(),
			asset: value.asset.into(),
			chain: ForeignChain::Ethereum,
		}
	}
}

type BitcoinDepositInfo =
	(DepositWitness<Bitcoin>, <Bitcoin as Chain>::ChainBlockNumber, BitcoinNetwork);

impl From<BitcoinDepositInfo> for WitnessInformation {
	fn from((value, height, network): BitcoinDepositInfo) -> Self {
		Self::Deposit {
			deposit_chain_block_height: height,
			deposit_address: value.deposit_address.to_address(&network),
			amount: value.amount.into(),
			asset: value.asset.into(),
			chain: ForeignChain::Bitcoin,
		}
	}
}

impl From<DepositInfo<Polkadot>> for WitnessInformation {
	fn from((value, height): DepositInfo<Polkadot>) -> Self {
		Self::Deposit {
			deposit_chain_block_height: height as u64,
			deposit_address: format!("0x{}", hex::encode(value.deposit_address.aliased_ref())),
			amount: value.amount.into(),
			asset: value.asset.into(),
			chain: ForeignChain::Polkadot,
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
	StateChainClient: StorageApi + ChainApi + UnsignedExtrinsicApi + 'static + Send + Sync,
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
	con: &mut S,
	btc_network: BitcoinNetwork,
	state_chain_client: &StateChainClient,
) -> anyhow::Result<()>
where
	S: Store,
	StateChainClient: StorageApi + ChainApi + UnsignedExtrinsicApi + 'static + Send + Sync,
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
				WitnessInformation::from((witness, block_height)).save_to_array(con).await?;
			},
		BitcoinIngressEgress(IngressEgressCall::process_deposits {
			deposit_witnesses,
			block_height,
		}) =>
			for witness in deposit_witnesses as Vec<DepositWitness<Bitcoin>> {
				WitnessInformation::from((witness, block_height, btc_network))
					.save_to_array(con)
					.await?;
			},
		PolkadotIngressEgress(IngressEgressCall::process_deposits {
			deposit_witnesses,
			block_height,
		}) =>
			for witness in deposit_witnesses as Vec<DepositWitness<Polkadot>> {
				WitnessInformation::from((witness, block_height)).save_to_array(con).await?;
			},
		EthereumBroadcaster(BroadcastCall::transaction_succeeded { tx_out_id, .. }) => {
			let broadcast_id =
				get_broadcast_id::<Ethereum, StateChainClient>(&state_chain_client, &tx_out_id)
					.await;

			if let Some(broadcast_id) = broadcast_id {
				WitnessInformation::Broadcast {
					broadcast_id,
					tx_out_id: TransactionId::Ethereum { signature: tx_out_id },
				}
				.save_singleton(con)
				.await?;
			}
		},
		BitcoinBroadcaster(BroadcastCall::transaction_succeeded { tx_out_id, .. }) => {
			let broadcast_id =
				get_broadcast_id::<Bitcoin, StateChainClient>(&state_chain_client, &tx_out_id)
					.await;

			if let Some(broadcast_id) = broadcast_id {
				WitnessInformation::Broadcast {
					broadcast_id,
					tx_out_id: TransactionId::Bitcoin {
						hash: format!("0x{}", hex::encode(tx_out_id)),
					},
				}
				.save_singleton(con)
				.await?;
			}
		},
		PolkadotBroadcaster(BroadcastCall::transaction_succeeded { tx_out_id, .. }) => {
			let broadcast_id =
				get_broadcast_id::<Polkadot, StateChainClient>(&state_chain_client, &tx_out_id)
					.await;

			if let Some(broadcast_id) = broadcast_id {
				WitnessInformation::Broadcast {
					broadcast_id,
					tx_out_id: TransactionId::Polkadot {
						signature: format!("0x{}", hex::encode(tx_out_id.aliased_ref())),
					},
				}
				.save_singleton(con)
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
	let btc_network = env_params.btc_network;
	witnessing::btc_mempool::start(scope, settings.btc, con.clone(), btc_network);
	let mut witness_receiver = witness_sender.subscribe();

	while let Ok(call) = witness_receiver.recv().await {
		handle_call(call, &mut con, btc_network, state_chain_client.deref()).await?
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
