use cf_chains::{btc::BitcoinNetwork, AnyChain, Bitcoin, Chain, Ethereum, Polkadot};
use cf_primitives::{Asset, ForeignChain};
use chainflip_engine::settings::{
	insert_command_line_option, CfSettings, HttpBasicAuthEndpoint, WsHttpEndpoints,
};
use clap::Parser;
use codec::Encode;
use config::{Config, ConfigBuilder, ConfigError, Environment, Map, Source, Value};
use futures::FutureExt;
use jsonrpsee::{core::Error, server::ServerBuilder, RpcModule};
use pallet_cf_ingress_egress::DepositWitness;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env, net::SocketAddr};
use tracing::log;
use utilities::{rpc::NumberOrHex, task_scope};

mod witnessing;

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
#[serde(tag = "type", rename_all = "snake_case")]
enum WitnessInformation {
	Deposit {
		src_chain_block_height: <AnyChain as Chain>::ChainBlockNumber,
		deposit_address: String,
		amount: NumberOrHex,
		asset: WitnessAsset,
	},
}

type EthereumDepositInfo = (DepositWitness<Ethereum>, <Ethereum as Chain>::ChainBlockNumber);

impl From<EthereumDepositInfo> for WitnessInformation {
	fn from((value, height): EthereumDepositInfo) -> Self {
		Self::Deposit {
			src_chain_block_height: height,
			deposit_address: value.deposit_address.to_string(),
			amount: value.amount.into(),
			asset: value.asset.into(),
		}
	}
}

type BitcoinDepositInfo =
	(DepositWitness<Bitcoin>, <Bitcoin as Chain>::ChainBlockNumber, BitcoinNetwork);

impl From<BitcoinDepositInfo> for WitnessInformation {
	fn from((value, height, network): BitcoinDepositInfo) -> Self {
		Self::Deposit {
			src_chain_block_height: height,
			deposit_address: value.deposit_address.to_address(&network),
			amount: value.amount.into(),
			asset: value.asset.into(),
		}
	}
}

type PolkadotDepositInfo = (DepositWitness<Polkadot>, <Polkadot as Chain>::ChainBlockNumber);

impl From<PolkadotDepositInfo> for WitnessInformation {
	fn from((value, height): PolkadotDepositInfo) -> Self {
		Self::Deposit {
			src_chain_block_height: height as u64,
			deposit_address: format!("0x{}", hex::encode(value.deposit_address.aliased_ref())),
			amount: value.amount.into(),
			asset: value.asset.into(),
		}
	}
}

#[derive(Clone, Deserialize, Debug)]
pub struct DepositTrackerSettings {
	eth: WsHttpEndpoints,
	dot: WsHttpEndpoints,
	state_chain_ws_endpoint: String,
	btc: HttpBasicAuthEndpoint,
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
			.set_default("btc.basic_auth_password", "flip")
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

		Ok(map)
	}
}

async fn start(
	scope: &task_scope::Scope<'_, anyhow::Error>,
	settings: DepositTrackerSettings,
) -> anyhow::Result<()> {
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");
	let mut module = RpcModule::new(());

	let btc_tracker = witnessing::btc_mempool::start(scope, settings.btc.clone()).await;

	module.register_async_method("status", move |arguments, _context| {
		let btc_tracker = btc_tracker.clone();
		async move {
			arguments.parse::<Vec<String>>().map_err(Error::Call).and_then(|addresses| {
				btc_tracker
					.lookup_transactions(&addresses)
					.map_err(|err| jsonrpsee::core::Error::Custom(err.to_string()))
			})
		}
	})?;

	// Broadcast channel will drop old messages when the buffer is full to
	// avoid "memory leaks" due to slow receivers.
	const EVENT_BUFFER_SIZE: usize = 1024;
	let (witness_sender, _) =
		tokio::sync::broadcast::channel::<state_chain_runtime::RuntimeCall>(EVENT_BUFFER_SIZE);

	let env_params = witnessing::start(scope, settings, witness_sender.clone()).await?;
	let btc_network = env_params.btc_network;

	module.register_subscription(
		"subscribe_witnessing",
		"s_witnessing",
		"unsubscribe_witnessing",
		move |_params, mut sink, _context| {
			let mut witness_receiver = witness_sender.subscribe();

			tokio::spawn(async move {
				while let Ok(event) = witness_receiver.recv().await {
					use pallet_cf_broadcast::Call as BroadcastCall;
					use pallet_cf_ingress_egress::Call as IngressEgressCall;
					use state_chain_runtime::RuntimeCall::*;

					macro_rules! send {
						($value:expr) => {
							if let Ok(false) = sink.send($value) {
								log::debug!("Subscription is closed");
								return
							}
						};
					}

					match event {
						EthereumIngressEgress(IngressEgressCall::process_deposits {
							deposit_witnesses,
							block_height,
						}) =>
							for witness in deposit_witnesses as Vec<DepositWitness<Ethereum>> {
								let info = WitnessInformation::from((witness, block_height));
								send!(&info);
							},
						BitcoinIngressEgress(IngressEgressCall::process_deposits {
							deposit_witnesses,
							block_height,
						}) =>
							for witness in deposit_witnesses as Vec<DepositWitness<Bitcoin>> {
								let info =
									WitnessInformation::from((witness, block_height, btc_network));
								send!(&info);
							},
						PolkadotIngressEgress(IngressEgressCall::process_deposits {
							deposit_witnesses,
							block_height,
						}) =>
							for witness in deposit_witnesses as Vec<DepositWitness<Polkadot>> {
								let info = WitnessInformation::from((witness, block_height));
								send!(&info);
							},
						EthereumBroadcaster(BroadcastCall::transaction_succeeded { .. }) => {
							log::info!("received EthereumBroadcaster transaction_succeeded call")
						},
						BitcoinBroadcaster(BroadcastCall::transaction_succeeded { .. }) => {
							log::info!("received BitcoinBroadcaster transaction_succeeded call")
						},
						PolkadotBroadcaster(BroadcastCall::transaction_succeeded { .. }) => {
							log::info!("received PolkadotBroadcaster transaction_succeeded call")
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
				}
			});
			Ok(())
		},
	)?;

	scope.spawn(async {
		let server = ServerBuilder::default().build("0.0.0.0:13337".parse::<SocketAddr>()?).await?;
		let addr = server.local_addr()?;
		log::info!("Listening on http://{}", addr);
		server.start(module)?.stopped().await;
		// If the server stops for some reason, we return
		// error to terminate other tasks and the process.
		Err(anyhow::anyhow!("RPC server stopped"))
	});

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
