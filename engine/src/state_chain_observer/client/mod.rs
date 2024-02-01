pub mod base_rpc_api;
pub mod chain_api;
pub mod error_decoder;
pub mod extrinsic_api;
pub mod storage_api;
pub mod stream_api;
pub mod subxt_state_chain_config;

use async_trait::async_trait;

use anyhow::{anyhow, bail, Context, Result};
use cf_primitives::{AccountRole, SemVer};
use futures::{StreamExt, TryStreamExt};

use futures_core::future::BoxFuture;
use futures_util::FutureExt;
use jsonrpsee::core::RpcResult;
use sp_core::{Pair, H256};
use state_chain_runtime::AccountId;
use std::{pin::Pin, sync::Arc, time::Duration};
use subxt::{backend::rpc::RpcClient, config::DefaultExtrinsicParamsBuilder};
use tokio::sync::watch;
use tracing::{info, warn};

use utilities::{
	cached_stream::{CachedStream, MakeCachedStream},
	loop_select, make_periodic_tick, read_clean_and_decode_hex_str_file, spmc,
	task_scope::{Scope, UnwrapOrCancel},
	try_cached_stream::{MakeTryCachedStream, TryCachedStream},
};

use crate::{
	constants::SIGNED_EXTRINSIC_LIFETIME,
	state_chain_observer::client::{
		base_rpc_api::SubxtInterface, storage_api::CheckBlockCompatibility,
	},
};

use self::{
	base_rpc_api::BaseRpcClient,
	chain_api::ChainApi,
	extrinsic_api::{
		signed::{signer, SignedExtrinsicApi, WaitFor, WaitForResult},
		unsigned,
	},
	storage_api::StorageApi,
	stream_api::{StateChainStream, StreamApi, FINALIZED, UNFINALIZED},
};

pub const STATE_CHAIN_CONNECTION: &str = "State Chain client connection failed"; // TODO Replace with infallible SCC requests

pub const STATE_CHAIN_BEHAVIOUR: &str = "State Chain client behavioural assumption not upheld";

/// For expressing an expectation regarding substrate's behaviour (Not our chain though)
const SUBSTRATE_BEHAVIOUR: &str = "Unexpected state chain node behaviour";

const SYNC_POLL_INTERVAL: Duration = Duration::from_secs(4);

/// Enough time for a state chain transaction to make it into a (unfinalised) block
const CFE_VERSION_SUBMIT_TIMEOUT: Duration = Duration::from_secs(30);

lazy_static::lazy_static! {
	static ref CFE_VERSION: SemVer = SemVer {
		major: env!("CARGO_PKG_VERSION_MAJOR").parse::<u8>().unwrap(),
		minor: env!("CARGO_PKG_VERSION_MINOR").parse::<u8>().unwrap(),
		patch: env!("CARGO_PKG_VERSION_PATCH").parse::<u8>().unwrap(),
	};
}

#[derive(Copy, Clone)]
pub struct BlockInfo {
	pub parent_hash: state_chain_runtime::Hash,
	pub hash: state_chain_runtime::Hash,
	pub number: state_chain_runtime::BlockNumber,
}
impl From<state_chain_runtime::Header> for BlockInfo {
	fn from(value: state_chain_runtime::Header) -> Self {
		Self { parent_hash: value.parent_hash, hash: value.hash(), number: value.number }
	}
}

pub type DefaultRpcClient = base_rpc_api::BaseRpcClient<jsonrpsee::ws_client::WsClient>;

impl DefaultRpcClient {
	pub async fn connect(ws_endpoint: &str) -> Result<Self> {
		Ok(BaseRpcClient::new(
			jsonrpsee::ws_client::WsClientBuilder::default()
				.build(ws_endpoint)
				.await
				.with_context(|| {
					format!("Failed to establish rpc connection to substrate node '{ws_endpoint}'")
				})?,
		))
	}
}

pub struct StateChainClient<
	SignedExtrinsicClient = extrinsic_api::signed::SignedExtrinsicClient,
	BaseRpcClient = DefaultRpcClient,
> {
	genesis_hash: state_chain_runtime::Hash,
	signed_extrinsic_client: SignedExtrinsicClient,
	unsigned_extrinsic_client: unsigned::UnsignedExtrinsicClient,
	pub base_rpc_client: Arc<BaseRpcClient>,
	finalized_block_stream_request_sender:
		tokio::sync::mpsc::Sender<tokio::sync::oneshot::Sender<Box<dyn StreamApi<FINALIZED>>>>,
	unfinalized_block_stream_request_sender:
		tokio::sync::mpsc::Sender<tokio::sync::oneshot::Sender<Box<dyn StreamApi<UNFINALIZED>>>>,
	latest_finalized_block_watcher: tokio::sync::watch::Receiver<BlockInfo>,
	latest_unfinalized_block_watcher: tokio::sync::watch::Receiver<BlockInfo>,
}

impl StateChainClient<extrinsic_api::signed::SignedExtrinsicClient> {
	pub async fn connect_with_account<'a>(
		scope: &Scope<'a, anyhow::Error>,
		ws_endpoint: &str,
		signing_key_file: &std::path::Path,
		required_role: AccountRole,
		wait_for_required_role: bool,
		wait_for_required_version: bool,
		submit_cfe_version: bool,
	) -> Result<(impl StreamApi<FINALIZED> + Clone, impl StreamApi<UNFINALIZED> + Clone, Arc<Self>)>
	{
		Self::new_with_account(
			scope,
			DefaultRpcClient::connect(ws_endpoint).await?.into(),
			signing_key_file,
			required_role,
			wait_for_required_role,
			wait_for_required_version,
			submit_cfe_version,
		)
		.await
	}
}

impl StateChainClient<()> {
	pub async fn connect_without_account<'a>(
		scope: &Scope<'a, anyhow::Error>,
		ws_endpoint: &str,
		wait_for_required_version: bool,
	) -> Result<(impl StreamApi<FINALIZED> + Clone, impl StreamApi<UNFINALIZED> + Clone, Arc<Self>)>
	{
		Self::new_without_account(
			scope,
			DefaultRpcClient::connect(ws_endpoint).await?.into(),
			wait_for_required_version,
		)
		.await
	}
}

impl<BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static>
	StateChainClient<extrinsic_api::signed::SignedExtrinsicClient, BaseRpcClient>
{
	pub async fn new_with_account<'a>(
		scope: &Scope<'a, anyhow::Error>,
		base_rpc_client: Arc<BaseRpcClient>,
		signing_key_file: &std::path::Path,
		required_role: AccountRole,
		wait_for_required_role: bool,
		wait_for_required_version: bool,
		submit_cfe_version: bool,
	) -> Result<(impl StreamApi<FINALIZED> + Clone, impl StreamApi<UNFINALIZED> + Clone, Arc<Self>)>
	{
		Self::new(
			scope,
			base_rpc_client,
			SignedExtrinsicClientBuilder {
				nonce_and_signer: None,
				signing_key_file: signing_key_file.to_owned(), /* I have to take a clone here
				                                                * because of a compiler issue it
				                                                * seems */
				required_role,
				wait_for_required_role,
				submit_cfe_version,
			},
			wait_for_required_version,
		)
		.await
	}
}

impl<BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static>
	StateChainClient<(), BaseRpcClient>
{
	pub async fn new_without_account<'a>(
		scope: &Scope<'a, anyhow::Error>,
		base_rpc_client: Arc<BaseRpcClient>,
		wait_for_required_version: bool,
	) -> Result<(impl StreamApi<FINALIZED> + Clone, impl StreamApi<UNFINALIZED> + Clone, Arc<Self>)>
	{
		Self::new(scope, base_rpc_client, (), wait_for_required_version).await
	}
}

impl<BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static, SignedExtrinsicClient>
	StateChainClient<SignedExtrinsicClient, BaseRpcClient>
{
	async fn create_block_stream<
		'a,
		const IS_FINALIZED: bool,
		NewStreamFn,
		ProcessStreamFn,
		ProcessStreamFut,
		ProcessedStream,
	>(
		scope: &Scope<'_, anyhow::Error>,
		base_rpc_client: Arc<BaseRpcClient>,
		wait_for_required_version: bool,
		error_on_incompatible_block: bool,
		new_stream_fn: NewStreamFn,
		process_stream_fn: ProcessStreamFn,
	) -> Result<(
		watch::Receiver<BlockInfo>,
		StateChainStream<
			IS_FINALIZED,
			utilities::cached_stream::InnerCachedStream<spmc::Receiver<BlockInfo>>,
		>,
		tokio::sync::mpsc::Sender<tokio::sync::oneshot::Sender<Box<dyn StreamApi<IS_FINALIZED>>>>,
	)>
	where
		NewStreamFn: for<'b> FnOnce(
			&'b BaseRpcClient,
		) -> BoxFuture<
			'b,
			RpcResult<jsonrpsee::core::client::Subscription<state_chain_runtime::Header>>,
		>,
		ProcessStreamFn: FnOnce(
				Pin<
					Box<
						dyn TryCachedStream<
								Ok = BlockInfo,
								Error = anyhow::Error,
								Item = Result<BlockInfo, anyhow::Error>,
							> + Send
							+ 'static,
					>,
				>,
			) -> ProcessStreamFut
			+ 'a,
		ProcessStreamFut: futures::Future<Output = Result<ProcessedStream, anyhow::Error>> + 'a,
		ProcessedStream: TryCachedStream<
				Ok = BlockInfo,
				Error = anyhow::Error,
				Item = Result<BlockInfo, anyhow::Error>,
			> + Send
			+ Unpin
			+ 'static,
	{
		let mut block_stream = new_stream_fn(&base_rpc_client)
			.await?
			.map_err(Into::into)
			.map_ok(|header| -> BlockInfo { header.into() })
			.chain(futures::stream::once(std::future::ready(Err(anyhow::anyhow!(
				STATE_CHAIN_CONNECTION
			)))))
			// Keep a copy of the base_rpc_client with the stream, as the subscription will end if
			// the client is dropped
			.map({
				let base_rpc_client = base_rpc_client.clone();
				move |i| {
					let _ = &base_rpc_client;
					i
				}
			});

		let incompatible_block_message = |block_version: SemVer, block: BlockInfo| {
			lazy_format::lazy_format!(
				"This version '{}' is incompatible with the current release '{block_version}' at block {}: {:?}.",
				*CFE_VERSION,
				block.number,
				block.hash,
			)
		};

		let mut processed_stream = {
			let latest_block: BlockInfo = block_stream.next().await.unwrap()?;
			let mut block_stream =
				process_stream_fn(Box::pin(block_stream.make_try_cached(latest_block))).await?;

			let latest_block = *block_stream.cache();
			if wait_for_required_version {
				let incompatible_blocks =
					futures::stream::once(futures::future::ready(Ok::<_, anyhow::Error>(
						latest_block,
					)))
					.chain(block_stream.by_ref())
					.try_take_while(|block_info| {
						let base_rpc_client = base_rpc_client.clone();
						let block_info = *block_info;
						async move {
							Ok(
								if let Err(block_version) = base_rpc_client
									.check_block_compatibility(block_info.hash)
									.await?
								{
									info!(
										"{} WAITING for a compatible release version.",
										incompatible_block_message(block_version, block_info)
									);
									true
								} else {
									false
								},
							)
						}
					})
					.boxed();

				// Note underlying stream ends in Error, therefore it is guaranteed try_for_each
				// will not exit successfully until a compatible block is found
				incompatible_blocks
					.try_for_each(move |_| futures::future::ready(Ok::<_, anyhow::Error>(())))
					.await?;
			} else {
				base_rpc_client.check_block_compatibility(latest_block.hash).await?.map_err(
					|latest_block_version| {
						anyhow::anyhow!(
							"{}",
							incompatible_block_message(latest_block_version, latest_block)
						)
					},
				)?;
			}

			block_stream
		};

		const BLOCK_CAPACITY: usize = 10;
		let (mut block_sender, block_receiver) = spmc::channel::<BlockInfo>(BLOCK_CAPACITY);

		let latest_block = *processed_stream.cache();
		let (latest_block_sender, latest_block_watcher) =
			tokio::sync::watch::channel::<BlockInfo>(latest_block);

		let (block_stream_request_sender, block_stream_request_receiver) =
			tokio::sync::mpsc::channel::<
				tokio::sync::oneshot::Sender<Box<dyn StreamApi<IS_FINALIZED>>>,
			>(1);

		scope.spawn({
			let mut block_stream_request_receiver: tokio_stream::wrappers::ReceiverStream<_> =
				tokio_stream::wrappers::ReceiverStream::new(block_stream_request_receiver);
			let mut latest_block = latest_block;
			async move {
				loop_select!(
					let result_block = processed_stream.next().map(|option| option.unwrap()) => {
						let block = result_block?;

						match base_rpc_client.check_block_compatibility(block.hash).await? {
							Ok(()) => {
								latest_block = block;
								block_sender.send(block).await;
								let _result = latest_block_sender.send(block);
							},
							Err(block_version) => {
								if error_on_incompatible_block {
									break Err(anyhow!("{}", incompatible_block_message(block_version, block)));
								}
							}
						}
					},
					if let Some(block_stream_request) = block_stream_request_receiver.next() => {
						let _result = block_stream_request.send(Box::new(StateChainStream::new(block_sender.receiver().make_cached(latest_block))));
					} else break Ok(()),
				)
			}
		});

		Ok((
			latest_block_watcher,
			StateChainStream::new(block_receiver.make_cached(latest_block)),
			block_stream_request_sender,
		))
	}

	async fn inject_intervening_headers<'a>(
		sparse_block_stream: impl TryCachedStream<
				Ok = BlockInfo,
				Error = anyhow::Error,
				Item = Result<BlockInfo, anyhow::Error>,
			> + Send
			+ 'a,
		base_rpc_client: Arc<BaseRpcClient>,
	) -> utilities::try_cached_stream::InnerTryCachedStream<
		impl futures::Stream<Item = Result<BlockInfo, anyhow::Error>> + Send + 'a,
	> {
		let latest_finalized_block: BlockInfo = *sparse_block_stream.cache();

		sparse_block_stream
			.and_then({
				let mut latest_finalized_block = latest_finalized_block;
				move |next_finalized_block| {
					assert!(
						latest_finalized_block.number < next_finalized_block.number,
						"{SUBSTRATE_BEHAVIOUR}",
					);

					let prev_finalized_block =
						std::mem::replace(&mut latest_finalized_block, next_finalized_block);

					let base_rpc_client = base_rpc_client.clone();
					async move {
						let base_rpc_client = &base_rpc_client;
						let intervening_blocks: Vec<_> = futures::stream::iter(
							prev_finalized_block.number + 1..next_finalized_block.number,
						)
						.then(|block_number| async move {
							let block_hash = base_rpc_client
								.block_hash(block_number)
								.await?
								.expect(SUBSTRATE_BEHAVIOUR);
							let block: BlockInfo =
								base_rpc_client.block_header(block_hash).await?.into();
							assert_eq!(block.hash, block_hash, "{SUBSTRATE_BEHAVIOUR}");
							assert_eq!(block.number, block_number, "{SUBSTRATE_BEHAVIOUR}",);
							Result::<_, anyhow::Error>::Ok(block)
						})
						.try_collect()
						.await?;

						for (previous_block, next_block) in Iterator::zip(
							std::iter::once(&prev_finalized_block).chain(intervening_blocks.iter()),
							intervening_blocks.iter().chain(std::iter::once(&next_finalized_block)),
						) {
							assert_eq!(previous_block.hash, next_block.parent_hash);
						}

						Result::<_, anyhow::Error>::Ok(futures::stream::iter(
							intervening_blocks
								.into_iter()
								.chain(std::iter::once(next_finalized_block))
								.map(Result::<_, anyhow::Error>::Ok),
						))
					}
				}
			})
			.try_flatten()
			.make_try_cached(latest_finalized_block)
	}
}

impl<BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static, SignedExtrinsicClient>
	StateChainClient<SignedExtrinsicClient, BaseRpcClient>
{
	async fn new<
		'a,
		SignedExtrinsicClientBuilder: SignedExtrinsicClientBuilderTrait<Client = SignedExtrinsicClient>,
	>(
		scope: &Scope<'a, anyhow::Error>,
		base_rpc_client: Arc<BaseRpcClient>,
		mut signed_extrinsic_client_builder: SignedExtrinsicClientBuilder,
		wait_for_required_version: bool,
	) -> Result<(impl StreamApi<FINALIZED> + Clone, impl StreamApi<UNFINALIZED> + Clone, Arc<Self>)>
	{
		{
			let mut poll_interval = make_periodic_tick(SYNC_POLL_INTERVAL, false);
			while base_rpc_client.health().await?.is_syncing {
				info!(
					"Waiting for Chainflip node to sync. Checking again in {:?} ...",
					poll_interval.period(),
				);
				poll_interval.tick().await;
			}
		}

		let genesis_hash = base_rpc_client.block_hash(0).await?.expect(SUBSTRATE_BEHAVIOUR);

		let (
			latest_finalized_block_watcher,
			mut finalized_state_chain_stream,
			finalized_block_stream_request_sender,
		) = Self::create_block_stream(
			scope,
			base_rpc_client.clone(),
			wait_for_required_version,
			true,
			|base_rpc_client| base_rpc_client.subscribe_finalized_block_headers(),
			|sparse_finalized_block_stream| {
				let base_rpc_client = base_rpc_client.clone();
				let signed_extrinsic_client_builder = &mut signed_extrinsic_client_builder;
				async move {
					let mut finalized_block_stream = Box::pin(
						Self::inject_intervening_headers(
							sparse_finalized_block_stream,
							base_rpc_client.clone(),
						)
						.await,
					);

					// Often `finalized_header` returns a significantly newer latest block than the
					// stream returns so we move the stream forward to this block
					{
						let finalised_header_hash =
							base_rpc_client.latest_finalized_block_hash().await?;
						let finalised_header =
							base_rpc_client.block_header(finalised_header_hash).await?;
						if finalized_block_stream.cache().number < finalised_header.number {
							let blocks_to_skip =
								finalized_block_stream.cache().number + 1..=finalised_header.number;
							for block_number in blocks_to_skip {
								assert_eq!(
									finalized_block_stream.next().await.unwrap()?.number,
									block_number,
									"{SUBSTRATE_BEHAVIOUR}"
								);
							}
						}
					}

					signed_extrinsic_client_builder
						.pre_compatibility(base_rpc_client.clone(), &mut finalized_block_stream)
						.await?;

					Ok(finalized_block_stream)
				}
			},
		)
		.await?;

		let (
			latest_unfinalized_block_watcher,
			unfinalized_state_chain_stream,
			unfinalized_block_stream_request_sender,
		) = Self::create_block_stream(
			scope,
			base_rpc_client.clone(),
			wait_for_required_version,
			false,
			|base_rpc_client| base_rpc_client.subscribe_unfinalized_block_headers(),
			|block_stream| futures::future::ready(Ok(block_stream)),
		)
		.await?;

		let state_chain_client = Arc::new(StateChainClient {
			genesis_hash,
			signed_extrinsic_client: signed_extrinsic_client_builder
				.build(
					scope,
					base_rpc_client.clone(),
					genesis_hash,
					&mut finalized_state_chain_stream,
				)
				.await?,
			unsigned_extrinsic_client: unsigned::UnsignedExtrinsicClient::new(
				scope,
				base_rpc_client.clone(),
			),
			finalized_block_stream_request_sender,
			unfinalized_block_stream_request_sender,
			base_rpc_client,
			latest_finalized_block_watcher,
			latest_unfinalized_block_watcher,
		});

		info!(
			"Initialised StateChainClient at block `{}`; block hash: `{:#x}`",
			finalized_state_chain_stream.cache().number,
			finalized_state_chain_stream.cache().hash
		);

		Ok((finalized_state_chain_stream, unfinalized_state_chain_stream, state_chain_client))
	}

	pub fn genesis_hash(&self) -> state_chain_runtime::Hash {
		self.genesis_hash
	}
}

#[async_trait]
trait SignedExtrinsicClientBuilderTrait {
	type Client;

	async fn pre_compatibility<
		BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		FinalizedBlockStream: TryCachedStream<Ok = BlockInfo, Error = anyhow::Error> + Send + Unpin,
	>(
		&mut self,
		base_rpc_client: Arc<BaseRpcClient>,
		finalized_block_stream: &mut FinalizedBlockStream,
	) -> Result<()>;

	async fn build<
		'a,
		BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		BlockStream: StreamApi<FINALIZED> + Clone,
	>(
		self,
		scope: &Scope<'a, anyhow::Error>,
		base_rpc_client: Arc<BaseRpcClient>,
		genesis_hash: state_chain_runtime::Hash,
		state_chain_stream: &mut BlockStream,
	) -> Result<Self::Client>;
}

#[async_trait]
impl SignedExtrinsicClientBuilderTrait for () {
	type Client = ();

	async fn pre_compatibility<
		BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		FinalizedBlockStream: TryCachedStream<Ok = BlockInfo, Error = anyhow::Error> + Send + Unpin,
	>(
		&mut self,
		_base_rpc_client: Arc<BaseRpcClient>,
		_finalized_block_stream: &mut FinalizedBlockStream,
	) -> Result<()> {
		Ok(())
	}

	async fn build<
		'a,
		BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		BlockStream: StreamApi<FINALIZED> + Clone,
	>(
		self,
		_scope: &Scope<'a, anyhow::Error>,
		_base_rpc_client: Arc<BaseRpcClient>,
		_genesis_hash: state_chain_runtime::Hash,
		_block_stream: &mut BlockStream,
	) -> Result<Self::Client> {
		Ok(())
	}
}

struct SignedExtrinsicClientBuilder {
	nonce_and_signer:
		Option<(state_chain_runtime::Nonce, signer::PairSigner<sp_core::sr25519::Pair>)>,
	signing_key_file: std::path::PathBuf,
	required_role: AccountRole,
	wait_for_required_role: bool,
	submit_cfe_version: bool,
}
#[async_trait]
impl SignedExtrinsicClientBuilderTrait for SignedExtrinsicClientBuilder {
	type Client = extrinsic_api::signed::SignedExtrinsicClient;

	async fn pre_compatibility<
		BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		FinalizedBlockStream: TryCachedStream<Ok = BlockInfo, Error = anyhow::Error> + Send + Unpin,
	>(
		&mut self,
		base_rpc_client: Arc<BaseRpcClient>,
		finalized_block_stream: &mut FinalizedBlockStream,
	) -> Result<()> {
		// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
		// !!!!!!!!!!!!!!!! IMPORTANT: Care must be taken when changing this !!!!!!!!!!!!!!!!!!!
		// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
		// !!!!! This is because this code is run before the version compatibility checks !!!!!!
		// !!!!!!!!!!! Therefore if any storage items used here are changed between !!!!!!!!!!!!
		// !!!!!!!!!!!!!!!!!!!!!!! runtime upgrades this code will fail !!!!!!!!!!!!!!!!!!!!!!!!
		// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!

		assert!(
			self.nonce_and_signer.is_none(),
			"This function should be run exactly once successfully before build is called"
		);

		let pair = sp_core::sr25519::Pair::from_seed(&read_clean_and_decode_hex_str_file(
			&self.signing_key_file,
			"Signing Key",
			|str| {
				<[u8; 32]>::try_from(hex::decode(str)?)
					.map_err(|e| anyhow!("Failed to decode signing key: Wrong length. {e:?}"))
			},
		)?);
		let signer = signer::PairSigner::<sp_core::sr25519::Pair>::new(pair.clone());

		let account_nonce = {
			loop {
				let block_hash = finalized_block_stream.cache().hash;

				match base_rpc_client
					.storage_map_entry::<pallet_cf_account_roles::AccountRoles<state_chain_runtime::Runtime>>(
						block_hash,
						&signer.account_id,
					)
					.await?
				{
					Some(role) =>
						if self.required_role == AccountRole::Unregistered ||
							self.required_role == role
						{
							break
						} else if self.wait_for_required_role && role == AccountRole::Unregistered {
							warn!("Your Chainflip account {} does not have an assigned account role. WAITING for the account role to be set to '{:?}' at block: {block_hash}", signer.account_id, self.required_role);
						} else {
							bail!("Your Chainflip account {} has the wrong account role '{role:?}'. The '{:?}' account role is required", signer.account_id, self.required_role);
						},
					None =>
						if self.wait_for_required_role {
							warn!("Your Chainflip account {} is not funded. Note, it may take some time for your funds to be detected. WAITING for your account to be funded at block: {block_hash}", signer.account_id);
						} else {
							bail!("Your Chainflip account {} is not funded", signer.account_id);
						},
				}

				finalized_block_stream.next().unwrap_or_cancel().await?;
			}

			let block_hash = finalized_block_stream.cache().hash;

			base_rpc_client
				.storage_map_entry::<frame_system::Account<state_chain_runtime::Runtime>>(
					block_hash,
					&signer.account_id,
				)
				.await?
				.nonce
		};

		if self.submit_cfe_version {
			use crate::state_chain_observer::client::subxt_state_chain_config::StateChainConfig;
			use subxt::tx::Signer;

			let rpc_client = RpcClient::new(SubxtInterface(base_rpc_client.clone()));

			let subxt_client = subxt::client::OnlineClient::<StateChainConfig>::from_rpc_client(
				rpc_client.clone(),
			)
			.await?;
			let subxt_signer = {
				struct SubxtSignerInterface<T>(subxt::utils::AccountId32, T);
				impl subxt::tx::Signer<StateChainConfig> for SubxtSignerInterface<sp_core::sr25519::Pair> {
					fn account_id(&self) -> <StateChainConfig as subxt::Config>::AccountId {
						self.0.clone()
					}

					fn address(&self) -> <StateChainConfig as subxt::Config>::Address {
						subxt::utils::MultiAddress::Id(self.0.clone())
					}

					fn sign(&self, bytes: &[u8]) -> <StateChainConfig as subxt::Config>::Signature {
						use sp_core::Pair;
						state_chain_runtime::Signature::Sr25519(self.1.sign(bytes))
					}
				}
				SubxtSignerInterface(subxt::utils::AccountId32(*signer.account_id.as_ref()), pair)
			};

			let recorded_version = <SemVer as codec::Decode>::decode(
				&mut subxt_client
					.storage()
					.at_latest()
					.await?
					.fetch_or_default(&subxt::storage::dynamic(
						"Validator",
						"NodeCFEVersion",
						vec![subxt_signer.account_id()],
					))
					.await?
					.encoded(),
			)
			.map_err(|e| anyhow::anyhow!("Failed to decode recorded_version: {e:?}"))?;

			// Note that around CFE upgrade period, the less recent version might still be running
			// (and can even be *the* "active" instance), so it is important that it doesn't
			// downgrade the version record:
			if CFE_VERSION.is_more_recent_than(recorded_version) {
				info!(
					"Updating CFE version record from {:?} to {:?}",
					recorded_version, *CFE_VERSION
				);

				let block_hash = finalized_block_stream.cache().hash;
				let block_number = finalized_block_stream.cache().number;

				// Submitting transaction with subxt sometimes gets stuck without returning any
				// error (see https://linear.app/chainflip/issue/PRO-1064/new-cfe-version-gets-stuck-on-startup),
				// so we use a timeout to ensure we can recover:
				tokio::time::timeout(CFE_VERSION_SUBMIT_TIMEOUT, async {
					let current_nonce = rpc_client
						.request::<u32>(
							"system_accountNextIndex",
							subxt::rpc_params![&subxt_signer.account_id()],
						)
						.await?;

					subxt_client
						.tx()
						.create_signed_with_nonce(
							&subxt::dynamic::tx(
								"Validator",
								"cfe_version",
								vec![(
									"new_version",
									vec![
										("major", CFE_VERSION.major),
										("minor", CFE_VERSION.minor),
										("patch", CFE_VERSION.patch),
									],
								)],
							),
							&subxt_signer,
							current_nonce.into(),
							DefaultExtrinsicParamsBuilder::new()
								.mortal_unchecked(
									block_number.into(),
									block_hash,
									SIGNED_EXTRINSIC_LIFETIME.into(),
								)
								.build(),
						)?
						.submit_and_watch()
						.await?
						.wait_for_in_block()
						.await
				})
				.await
				.map_err(|_| anyhow::anyhow!("Timed out trying to submit CFE version"))??;
			}
		}

		self.nonce_and_signer = Some((account_nonce, signer));

		Ok(())
	}

	async fn build<
		'b,
		BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		BlockStream: StreamApi<FINALIZED> + Clone,
	>(
		self,
		scope: &Scope<'b, anyhow::Error>,
		base_rpc_client: Arc<BaseRpcClient>,
		genesis_hash: state_chain_runtime::Hash,
		state_chain_stream: &mut BlockStream,
	) -> Result<Self::Client> {
		let (nonce, signer) = self.nonce_and_signer.expect("The function pre_compatibility should be run exactly once successfully before build is called");
		Self::Client::new(scope, base_rpc_client, nonce, signer, genesis_hash, state_chain_stream)
			.await
	}
}

#[async_trait]
impl<
		BaseRpcApi: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		SignedExtrinsicClient: SignedExtrinsicApi + Send + Sync + 'static,
	> extrinsic_api::signed::SignedExtrinsicApi
	for StateChainClient<SignedExtrinsicClient, BaseRpcApi>
{
	type UntilFinalizedFuture = SignedExtrinsicClient::UntilFinalizedFuture;
	type UntilInBlockFuture = SignedExtrinsicClient::UntilInBlockFuture;

	fn account_id(&self) -> AccountId {
		self.signed_extrinsic_client.account_id()
	}

	/// Do a dry run of the extrinsic first, only submit if Ok(())
	async fn submit_signed_extrinsic_with_dry_run<Call>(
		&self,
		call: Call,
	) -> anyhow::Result<(H256, (Self::UntilInBlockFuture, Self::UntilFinalizedFuture))>
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static,
	{
		self.signed_extrinsic_client.submit_signed_extrinsic_with_dry_run(call).await
	}

	/// Submit an signed extrinsic, returning the hash of the submission
	async fn submit_signed_extrinsic<Call>(
		&self,
		call: Call,
	) -> (H256, (Self::UntilInBlockFuture, Self::UntilFinalizedFuture))
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static,
	{
		self.signed_extrinsic_client.submit_signed_extrinsic(call).await
	}

	async fn submit_signed_extrinsic_wait_for<Call>(
		&self,
		call: Call,
		wait_for: WaitFor,
	) -> Result<WaitForResult>
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static,
	{
		self.signed_extrinsic_client
			.submit_signed_extrinsic_wait_for(call, wait_for)
			.await
	}

	/// Sign, submit, and watch an extrinsic retrying if submissions fail be to finalized
	async fn finalize_signed_extrinsic<Call>(
		&self,
		call: Call,
	) -> (Self::UntilInBlockFuture, Self::UntilFinalizedFuture)
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ Clone
			+ std::fmt::Debug
			+ Send
			+ Sync
			+ 'static,
	{
		self.signed_extrinsic_client.finalize_signed_extrinsic(call).await
	}
}

#[async_trait]
impl<
		BaseRpcApi: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		SignedExtrinsicClient: Send + Sync + 'static,
	> unsigned::UnsignedExtrinsicApi for StateChainClient<SignedExtrinsicClient, BaseRpcApi>
{
	/// Submit an unsigned extrinsic.
	async fn submit_unsigned_extrinsic<Call>(
		&self,
		call: Call,
	) -> Result<H256, unsigned::ExtrinsicError>
	where
		Call: Into<state_chain_runtime::RuntimeCall>
			+ std::fmt::Debug
			+ Clone
			+ Send
			+ Sync
			+ 'static,
	{
		self.unsigned_extrinsic_client.submit_unsigned_extrinsic(call).await
	}
}

#[async_trait]
impl<
		BaseRpcApi: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		SignedExtrinsicClient: Send + Sync + 'static,
	> ChainApi for StateChainClient<SignedExtrinsicClient, BaseRpcApi>
{
	fn latest_finalized_block(&self) -> BlockInfo {
		*self.latest_finalized_block_watcher.borrow()
	}

	fn latest_unfinalized_block(&self) -> BlockInfo {
		*self.latest_unfinalized_block_watcher.borrow()
	}

	async fn finalized_block_stream(&self) -> Box<dyn StreamApi<FINALIZED>> {
		let (sender, receiver) = tokio::sync::oneshot::channel();
		self.finalized_block_stream_request_sender.send(sender).unwrap_or_cancel().await;
		receiver.unwrap_or_cancel().await
	}

	async fn unfinalized_block_stream(&self) -> Box<dyn StreamApi<UNFINALIZED>> {
		let (sender, receiver) = tokio::sync::oneshot::channel();
		self.unfinalized_block_stream_request_sender
			.send(sender)
			.unwrap_or_cancel()
			.await;
		receiver.unwrap_or_cancel().await
	}

	async fn block(&self, block_hash: state_chain_runtime::Hash) -> RpcResult<BlockInfo> {
		self.base_rpc_client.block_header(block_hash).await.map(|header| header.into())
	}
}

#[cfg(test)]
pub mod mocks {
	use crate::state_chain_observer::client::{
		extrinsic_api::{signed::SignedExtrinsicApi, unsigned::UnsignedExtrinsicApi},
		storage_api::StorageApi,
		ChainApi,
	};
	use async_trait::async_trait;
	use frame_support::storage::types::QueryKindTrait;
	use jsonrpsee::core::RpcResult;
	use mockall::mock;
	use sp_core::{storage::StorageKey, H256};
	use state_chain_runtime::AccountId;

	use super::{
		extrinsic_api::{
			self,
			signed::{WaitFor, WaitForResult},
			unsigned,
		},
		storage_api,
		stream_api::{StreamApi, FINALIZED, UNFINALIZED},
		BlockInfo,
	};

	mock! {
		pub StateChainClient {}
		#[async_trait]
		impl SignedExtrinsicApi for StateChainClient {
			type UntilFinalizedFuture = extrinsic_api::signed::MockUntilFinalized;
			type UntilInBlockFuture = extrinsic_api::signed::MockUntilInBlock;

			fn account_id(&self) -> AccountId;

			async fn submit_signed_extrinsic<Call>(&self, call: Call) -> (H256, (<Self as SignedExtrinsicApi>::UntilInBlockFuture, <Self as SignedExtrinsicApi>::UntilFinalizedFuture))
			where
				Call: Into<state_chain_runtime::RuntimeCall>
					+ Clone
					+ std::fmt::Debug
					+ Send
					+ Sync
					+ 'static;

			async fn submit_signed_extrinsic_wait_for<Call>(
				&self,
				call: Call,
				wait_for: WaitFor,
			) -> anyhow::Result<WaitForResult>
			where
				Call: Into<state_chain_runtime::RuntimeCall>
					+ Clone
					+ std::fmt::Debug
					+ Send
					+ Sync
					+ 'static;

			async fn submit_signed_extrinsic_with_dry_run<Call>(&self, call: Call) -> anyhow::Result<(H256, (<Self as SignedExtrinsicApi>::UntilInBlockFuture, <Self as SignedExtrinsicApi>::UntilFinalizedFuture))>
			where
				Call: Into<state_chain_runtime::RuntimeCall>
					+ Clone
					+ std::fmt::Debug
					+ Send
					+ Sync
					+ 'static;

			async fn finalize_signed_extrinsic<Call>(&self, call: Call) -> (<Self as SignedExtrinsicApi>::UntilInBlockFuture, <Self as SignedExtrinsicApi>::UntilFinalizedFuture)
			where
				Call: Into<state_chain_runtime::RuntimeCall>
					+ Clone
					+ std::fmt::Debug
					+ Send
					+ Sync
					+ 'static;
		}
		#[async_trait]
		impl UnsignedExtrinsicApi for StateChainClient {
			async fn submit_unsigned_extrinsic<Call>(
				&self,
				call: Call,
			) -> Result<H256, unsigned::ExtrinsicError>
			where
				Call: Into<state_chain_runtime::RuntimeCall> + Clone + std::fmt::Debug + Send + Sync + 'static;
		}
		#[async_trait]
		impl ChainApi for StateChainClient {
			fn latest_finalized_block(&self) -> BlockInfo;
			fn latest_unfinalized_block(&self) -> BlockInfo;

			async fn finalized_block_stream(&self) -> Box<dyn StreamApi<FINALIZED>>;
			async fn unfinalized_block_stream(&self) -> Box<dyn StreamApi<UNFINALIZED>>;

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
}

// Note: only to be used with finalized blocks since this asserts that they arrive strictly in order

#[cfg(test)]
mod tests {

	use std::collections::BTreeMap;

	use sp_runtime::Digest;
	use state_chain_runtime::Header;

	use super::{base_rpc_api::MockBaseRpcApi, *};

	struct TestChain {
		hashes: Vec<H256>,
		headers: BTreeMap<H256, Header>,
	}

	impl TestChain {
		fn new(total_blocks: usize) -> TestChain {
			let mut headers = Vec::<Header>::with_capacity(total_blocks);

			for index in 0..total_blocks {
				headers.push(Header {
					number: index as u32,
					parent_hash: index
						.checked_sub(1)
						.map(|parent_i| headers[parent_i].hash())
						.unwrap_or_default(),
					state_root: H256::default(),
					extrinsics_root: H256::default(),
					digest: Digest { logs: Vec::new() },
				});
			}

			let hashes = headers.iter().map(|header| header.hash()).collect();

			let headers: BTreeMap<_, _> =
				headers.into_iter().map(|header| (header.hash(), header)).collect();

			TestChain { hashes, headers }
		}
	}

	fn mock_chain_and_rpc(total_blocks: usize) -> (Arc<TestChain>, Arc<MockBaseRpcApi>) {
		let chain = Arc::new(TestChain::new(total_blocks));
		let mut rpc = MockBaseRpcApi::new();

		rpc.expect_block_hash().returning({
			let chain = chain.clone();
			move |num| Ok(chain.hashes.get(num as usize).cloned())
		});

		rpc.expect_block_header().returning({
			let chain = chain.clone();
			move |hash| Ok(chain.headers.get(&hash).expect("unknown hash").clone())
		});

		(chain, Arc::new(rpc))
	}

	// turns a (potentially) sparse block sequence into a contiguous one
	async fn inject_intervening_headers_with<const N: usize>(
		block_numbers: [u32; N],
		chain: Arc<TestChain>,
		rpc: Arc<MockBaseRpcApi>,
	) -> Vec<u32> {
		let headers =
			block_numbers.map(|block_number| &chain.headers[&chain.hashes[block_number as usize]]);

		let sparse_stream = tokio_stream::iter(
			(headers[1..]).iter().map(|header| Ok(BlockInfo::from((*header).clone()))),
		)
		.make_try_cached(BlockInfo::from(headers[0].clone()));

		let stream =
			StateChainClient::<(), MockBaseRpcApi>::inject_intervening_headers(sparse_stream, rpc)
				.await;

		let cached_header = *stream.cache();

		let headers = stream.collect::<Vec<_>>().await;

		std::iter::once(Ok(cached_header))
			.chain(headers)
			.map(|header: Result<BlockInfo, anyhow::Error>| header.unwrap().number)
			.collect::<Vec<_>>()
	}

	#[tokio::test]
	async fn test_intervening_headers() {
		let (chain, rpc) = mock_chain_and_rpc(7);

		// Should fill in the gaps:
		assert_eq!(
			&inject_intervening_headers_with([0, 1, 3, 6], chain.clone(), rpc.clone()).await,
			&[0, 1, 2, 3, 4, 5, 6]
		);

		// Already contiguous stream should be left unchanged:
		assert_eq!(
			&inject_intervening_headers_with([1, 2, 3, 4], chain.clone(), rpc.clone()).await,
			&[1, 2, 3, 4]
		);

		// Empty stream doesn't panic (Note pass a single element here, results in an empty stream
		// being passed)
		assert_eq!(&inject_intervening_headers_with([0], chain.clone(), rpc.clone()).await, &[0],);
	}
}
