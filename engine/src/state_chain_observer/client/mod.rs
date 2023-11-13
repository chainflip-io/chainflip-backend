pub mod base_rpc_api;
pub mod chain_api;
pub mod error_decoder;
pub mod extrinsic_api;
pub mod finalized_stream;
pub mod storage_api;

use async_trait::async_trait;

use anyhow::{anyhow, bail, Context, Result};
use cf_primitives::{AccountRole, NodeCFEVersions, SemVer};
use futures::{StreamExt, TryStreamExt};

use futures_core::Stream;
use futures_util::FutureExt;
use sp_core::{Pair, H256};
use state_chain_runtime::AccountId;
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;
use tracing::{info, warn};

use utilities::{
	loop_select, make_periodic_tick, read_clean_and_decode_hex_str_file, spmc,
	task_scope::{Scope, OR_CANCEL},
	CachedStream, MakeCachedStream, MakeTryCachedStream, TryCachedStream,
};

use self::{
	base_rpc_api::BaseRpcClient,
	chain_api::ChainApi,
	extrinsic_api::{
		signed::{signer, SignedExtrinsicApi},
		unsigned,
	},
	finalized_stream::FinalizedCachedStream,
	storage_api::StorageApi,
};

/// For expressing an expectation regarding substrate's behaviour (Not our chain though)
const SUBSTRATE_BEHAVIOUR: &str = "Unexpected state chain node behaviour";

const SYNC_POLL_INTERVAL: Duration = Duration::from_secs(4);

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

pub trait StateChainStreamApi<const FINALIZED: bool = true>:
	CachedStream<Cache = BlockInfo, Item = BlockInfo> + Send + Sync + Unpin + 'static
{
}

impl<S, F> StateChainStreamApi<false> for utilities::InnerCachedStream<S, BlockInfo, F>
where
	S: Stream<Item = BlockInfo> + Send + Sync + Unpin + 'static,
	F: FnMut(&S::Item) -> BlockInfo + Send + Sync + Unpin + 'static,
{
}

impl<S: StateChainStreamApi<false>> StateChainStreamApi for FinalizedCachedStream<S> {}

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
		tokio::sync::mpsc::Sender<tokio::sync::oneshot::Sender<Box<dyn StateChainStreamApi>>>,
	unfinalized_block_stream_request_sender: tokio::sync::mpsc::Sender<
		tokio::sync::oneshot::Sender<Box<dyn StateChainStreamApi<false>>>,
	>,
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
		required_version_and_wait: Option<(SemVer, bool)>,
	) -> Result<(
		impl StateChainStreamApi + Clone,
		impl StateChainStreamApi<false> + Clone,
		Arc<Self>,
	)> {
		Self::new_with_account(
			scope,
			DefaultRpcClient::connect(ws_endpoint).await?.into(),
			signing_key_file,
			required_role,
			wait_for_required_role,
			required_version_and_wait,
		)
		.await
	}
}

impl StateChainClient<()> {
	pub async fn connect_without_account<'a>(
		scope: &Scope<'a, anyhow::Error>,
		ws_endpoint: &str,
		required_version_and_wait: Option<(SemVer, bool)>,
	) -> Result<(
		impl StateChainStreamApi + Clone,
		impl StateChainStreamApi<false> + Clone,
		Arc<Self>,
	)> {
		Self::new_without_account(
			scope,
			DefaultRpcClient::connect(ws_endpoint).await?.into(),
			required_version_and_wait,
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
		required_version_and_wait: Option<(SemVer, bool)>,
	) -> Result<(
		impl StateChainStreamApi + Clone,
		impl StateChainStreamApi<false> + Clone,
		Arc<Self>,
	)> {
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
				check_unfinalized_version: required_version_and_wait.map(|(version, _)| version),
				update_cfe_version: required_version_and_wait.map(|(version, _)| version),
			},
			required_version_and_wait,
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
		required_version_and_wait: Option<(SemVer, bool)>,
	) -> Result<(
		impl StateChainStreamApi + Clone,
		impl StateChainStreamApi<false> + Clone,
		Arc<Self>,
	)> {
		Self::new(scope, base_rpc_client, (), required_version_and_wait).await
	}
}

async fn create_finalized_block_subscription<
	BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
	SignedExtrinsicClientBuilder: SignedExtrinsicClientBuilderTrait,
>(
	scope: &Scope<'_, anyhow::Error>,
	base_rpc_client: Arc<BaseRpcClient>,
	signed_extrinsic_client_builder: &mut SignedExtrinsicClientBuilder,
	required_version_and_wait: Option<(SemVer, bool)>,
) -> Result<(
	watch::Receiver<BlockInfo>,
	impl StateChainStreamApi + Clone,
	tokio::sync::mpsc::Sender<tokio::sync::oneshot::Sender<Box<dyn StateChainStreamApi>>>,
)> {
	let mut finalized_block_stream = {
		// https://substrate.stackexchange.com/questions/3667/api-rpc-chain-subscribefinalizedheads-missing-blocks
		// https://arxiv.org/abs/2007.01560
		let sparse_finalized_block_stream = base_rpc_client
			.subscribe_finalized_block_headers()
			.await?
			.map_err(Into::into)
			.map_ok(|header| -> BlockInfo { header.into() })
			.chain(futures::stream::once(std::future::ready(Err(anyhow::anyhow!(
				"sparse_finalized_block_header_stream unexpectedly ended"
			)))));

		let mut finalized_block_stream = Box::pin(
			inject_intervening_headers(sparse_finalized_block_stream, base_rpc_client.clone())
				.await?,
		);

		let latest_finalized_block: BlockInfo = finalized_block_stream.next().await.unwrap()?;

		finalized_block_stream.make_try_cached(latest_finalized_block, |header| *header)
	};

	// Often `finalized_header` returns a significantly newer latest block than the stream
	// returns so we move the stream forward to this block
	{
		let finalised_header_hash = base_rpc_client.latest_finalized_block_hash().await?;
		let finalised_header = base_rpc_client.block_header(finalised_header_hash).await?;
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

	if let Some((required_version, wait_for_required_version)) = required_version_and_wait {
		let latest_block = *finalized_block_stream.cache();
		if wait_for_required_version {
			let incompatible_blocks =
				futures::stream::once(futures::future::ready(Ok::<_, anyhow::Error>(latest_block)))
					.chain(finalized_block_stream.by_ref())
					.and_then(|block| {
						let base_rpc_client = &base_rpc_client;
						async move {
							Ok::<_, anyhow::Error>((
								block.hash,
								base_rpc_client
									.storage_value::<pallet_cf_environment::CurrentReleaseVersion<
										state_chain_runtime::Runtime,
									>>(block.hash)
									.await?,
							))
						}
					})
					.try_take_while(|(_block_hash, current_release_version)| {
						futures::future::ready({
							Ok::<_, anyhow::Error>(
								!required_version.is_compatible_with(*current_release_version),
							)
						})
					})
					.boxed();

			incompatible_blocks.try_for_each(move |(block_hash, current_release_version)| futures::future::ready({
			info!("This version '{}' is incompatible with the current release '{}' at block: {}. WAITING for a compatible release version.", required_version, current_release_version, block_hash);
			Ok::<_, anyhow::Error>(())
		})).await?;
		} else {
			let current_release_version = base_rpc_client
				.storage_value::<pallet_cf_environment::CurrentReleaseVersion<state_chain_runtime::Runtime>>(
					latest_block.hash,
				)
				.await?;
			if !required_version.is_compatible_with(current_release_version) {
				bail!(
					"This version '{}' is incompatible with the current release '{}' at block: {}.",
					required_version,
					current_release_version,
					latest_block.hash,
				);
			}
		}
	}

	const BLOCK_CAPACITY: usize = 10;
	let (mut block_sender, block_receiver) = spmc::channel::<BlockInfo>(BLOCK_CAPACITY);

	let latest_block = *finalized_block_stream.cache();
	let (latest_block_sender, latest_block_watcher) =
		tokio::sync::watch::channel::<BlockInfo>(latest_block);

	let (block_stream_request_sender, block_stream_request_receiver) =
		tokio::sync::mpsc::channel::<tokio::sync::oneshot::Sender<Box<dyn StateChainStreamApi>>>(1);

	scope.spawn({
		let base_rpc_client = base_rpc_client.clone();
		let mut finalized_block_stream = finalized_block_stream.into_inner();
		let mut block_stream_request_receiver: tokio_stream::wrappers::ReceiverStream<_> =
			tokio_stream::wrappers::ReceiverStream::new(block_stream_request_receiver);
		let mut latest_block = latest_block;
		async move {
			loop_select!(
				let result_block = finalized_block_stream.next().map(|option| option.unwrap()) => {
					let block = result_block?;
					latest_block = block;
					if let Some((required_version, _)) = required_version_and_wait {
						let current_release_version = base_rpc_client.storage_value::<pallet_cf_environment::CurrentReleaseVersion<state_chain_runtime::Runtime>>(block.hash).await?;
						if !required_version.is_compatible_with(current_release_version) {
							break Err(anyhow!("This version '{}' is no longer compatible with the release version '{}' at block: {}", required_version, current_release_version, block.hash))
						}
					}

					block_sender.send(block).await;
					let _result = latest_block_sender.send(block);
				},
				if let Some(block_stream_request) = block_stream_request_receiver.next() => {
					let _result = block_stream_request.send(Box::new(FinalizedCachedStream::new(block_sender.receiver().make_cached(latest_block, |block: &BlockInfo| *block))));
				} else break Ok(()),
			)
		}
	});

	Ok((
		latest_block_watcher,
		FinalizedCachedStream::new(
			block_receiver.make_cached(latest_block, |block: &BlockInfo| *block),
		),
		block_stream_request_sender,
	))
}

async fn create_unfinalized_block_subscription<
	BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
>(
	scope: &Scope<'_, anyhow::Error>,
	base_rpc_client: Arc<BaseRpcClient>,
) -> Result<(
	watch::Receiver<BlockInfo>,
	impl StateChainStreamApi<false> + Clone,
	tokio::sync::mpsc::Sender<tokio::sync::oneshot::Sender<Box<dyn StateChainStreamApi<false>>>>,
)> {
	let mut sparse_block_stream = base_rpc_client
		.subscribe_unfinalized_block_headers()
		.await?
		.map_err(Into::into)
		.map_ok(|header| -> BlockInfo { header.into() })
		.chain(futures::stream::once(std::future::ready(Err(anyhow::anyhow!(
			"sparse_block_stream unexpectedly ended"
		)))));

	let first_block = sparse_block_stream.next().await.unwrap()?;

	const BLOCK_CAPACITY: usize = 10;
	let (mut block_sender, block_receiver) = spmc::channel::<BlockInfo>(BLOCK_CAPACITY);

	let (latest_block_sender, latest_block_watcher) =
		tokio::sync::watch::channel::<BlockInfo>(first_block);

	let (block_stream_request_sender, block_stream_request_receiver) = tokio::sync::mpsc::channel::<
		tokio::sync::oneshot::Sender<Box<dyn StateChainStreamApi<false>>>,
	>(1);

	scope.spawn({
		let mut block_stream_request_receiver: tokio_stream::wrappers::ReceiverStream<_> =
			tokio_stream::wrappers::ReceiverStream::new(block_stream_request_receiver);
		let mut latest_block = first_block;
		async move {
			loop_select!(
				let result_block = sparse_block_stream.next().map(|option| option.unwrap()) => {
					let block = result_block?;
					latest_block = block;

					block_sender.send(block).await;
					let _result = latest_block_sender.send(block);
				},
				if let Some(block_stream_request) = block_stream_request_receiver.next() => {
					let _result = block_stream_request.send(Box::new(block_sender.receiver().make_cached(latest_block, |block: &BlockInfo| *block)));
				} else break Ok(()),
			)
		}
	});

	Ok((
		latest_block_watcher,
		block_receiver.make_cached(first_block, |block: &BlockInfo| *block),
		block_stream_request_sender,
	))
}

async fn inject_intervening_headers<
	BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
>(
	sparse_block_stream: impl Stream<Item = Result<BlockInfo>> + Send + 'static,
	base_rpc_client: Arc<BaseRpcClient>,
) -> Result<impl Stream<Item = Result<BlockInfo>>> {
	let mut sparse_block_stream = Box::pin(sparse_block_stream);

	let latest_finalized_block: BlockInfo =
		sparse_block_stream.next().await.ok_or(anyhow!("initial header missing"))??;

	let stream_rest = utilities::assert_stream_send(
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
			.try_flatten(),
	);

	Ok(futures::stream::once(async move { Ok(latest_finalized_block) }).chain(stream_rest))
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
		required_version_and_wait: Option<(SemVer, bool)>,
	) -> Result<(
		impl StateChainStreamApi + Clone,
		impl StateChainStreamApi<false> + Clone,
		Arc<Self>,
	)> {
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
		) = create_finalized_block_subscription(
			scope,
			base_rpc_client.clone(),
			&mut signed_extrinsic_client_builder,
			required_version_and_wait,
		)
		.await?;

		let (
			latest_unfinalized_block_watcher,
			unfinalized_state_chain_stream,
			unfinalized_block_stream_request_sender,
		) = create_unfinalized_block_subscription(scope, base_rpc_client.clone()).await?;

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
		FinalizedBlockStream: TryCachedStream<Cache = BlockInfo, Item = Result<BlockInfo, anyhow::Error>> + Send + Unpin,
	>(
		&mut self,
		base_rpc_client: Arc<BaseRpcClient>,
		finalized_block_stream: &mut FinalizedBlockStream,
	) -> Result<()>;

	async fn build<
		'a,
		BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		BlockStream: StateChainStreamApi + Clone,
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
		FinalizedBlockStream: TryCachedStream<Cache = BlockInfo, Item = Result<BlockInfo, anyhow::Error>> + Send + Unpin,
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
		BlockStream: StateChainStreamApi + Clone,
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
	check_unfinalized_version: Option<SemVer>,
	update_cfe_version: Option<SemVer>,
}
#[async_trait]
impl SignedExtrinsicClientBuilderTrait for SignedExtrinsicClientBuilder {
	type Client = extrinsic_api::signed::SignedExtrinsicClient;

	async fn pre_compatibility<
		BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		FinalizedBlockStream: TryCachedStream<Cache = BlockInfo, Item = Result<BlockInfo, anyhow::Error>> + Send + Unpin,
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

				finalized_block_stream.next().await.expect(OR_CANCEL)?;
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

		if let Some(this_cfe_version) = self.update_cfe_version {
			use crate::state_chain_observer::client::base_rpc_api::SubxtInterface;
			use subxt::{tx::Signer, PolkadotConfig};

			let subxt_client = subxt::client::OnlineClient::<PolkadotConfig>::from_rpc_client(
				Arc::new(SubxtInterface(base_rpc_client.clone())),
			)
			.await?;
			let subxt_signer = {
				struct SubxtSignerInterface<T>(subxt::utils::AccountId32, T);
				impl subxt::tx::Signer<PolkadotConfig> for SubxtSignerInterface<sp_core::sr25519::Pair> {
					fn account_id(&self) -> <subxt::PolkadotConfig as subxt::Config>::AccountId {
						self.0.clone()
					}

					fn address(&self) -> <subxt::PolkadotConfig as subxt::Config>::Address {
						subxt::utils::MultiAddress::Id(self.0.clone())
					}

					fn sign(
						&self,
						bytes: &[u8],
					) -> <subxt::PolkadotConfig as subxt::Config>::Signature {
						use sp_core::Pair;
						subxt::utils::MultiSignature::Sr25519(self.1.sign(bytes).0)
					}
				}
				SubxtSignerInterface(subxt::utils::AccountId32(*signer.account_id.as_ref()), pair)
			};

			let recorded_versions = <NodeCFEVersions as codec::Decode>::decode(
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
			.map_err(|e| anyhow::anyhow!("Failed to decode recorded_versions: {e:?}"))?;

			// Note that around CFE upgrade period, the less recent version might still be running
			// (and can even be *the* "active" instance), so it is important that it doesn't
			// downgrade the version record:
			if this_cfe_version.is_more_recent_than(recorded_versions.cfe) {
				// if we need to update CFE version we also update the node version
				let running_node_version = subxt_client.rpc().system_version().await?;
				let new_node_version: SemVer = SemVer::parse(running_node_version.as_str())?;

				info!(
					"Updating CFE version record from {:?} to {:?}",
					recorded_versions.cfe, this_cfe_version
				);
				info!(
					"Updating Node version record from {:?} to {:?}",
					recorded_versions.node, new_node_version
				);

				subxt_client
					.tx()
					.sign_and_submit_then_watch_default(
						&subxt::dynamic::tx(
							"Validator",
							"set_node_cfe_version",
							vec![(
								"new_version",
								vec![
									vec![
										("major", new_node_version.major),
										("minor", new_node_version.minor),
										("patch", new_node_version.patch),
									],
									vec![
										("major", this_cfe_version.major),
										("minor", this_cfe_version.minor),
										("patch", this_cfe_version.patch),
									],
								],
							)],
						),
						&subxt_signer,
					)
					.await?
					.wait_for_in_block()
					.await?;
			}
		}

		self.nonce_and_signer = Some((account_nonce, signer));

		Ok(())
	}

	async fn build<
		'b,
		BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
		BlockStream: StateChainStreamApi + Clone,
	>(
		self,
		scope: &Scope<'b, anyhow::Error>,
		base_rpc_client: Arc<BaseRpcClient>,
		genesis_hash: state_chain_runtime::Hash,
		state_chain_stream: &mut BlockStream,
	) -> Result<Self::Client> {
		let (nonce, signer) = self.nonce_and_signer.expect("The function pre_compatibility should be run exactly once successfully before build is called");
		Self::Client::new(
			scope,
			base_rpc_client,
			nonce,
			signer,
			self.check_unfinalized_version,
			genesis_hash,
			state_chain_stream,
		)
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

	async fn finalized_block_stream(&self) -> Box<dyn StateChainStreamApi> {
		let (sender, receiver) = tokio::sync::oneshot::channel();
		self.finalized_block_stream_request_sender.send(sender).await.expect(OR_CANCEL);
		receiver.await.expect(OR_CANCEL)
	}

	async fn unfinalized_block_stream(&self) -> Box<dyn StateChainStreamApi<false>> {
		let (sender, receiver) = tokio::sync::oneshot::channel();
		self.unfinalized_block_stream_request_sender
			.send(sender)
			.await
			.expect(OR_CANCEL);
		receiver.await.expect(OR_CANCEL)
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
		extrinsic_api::{self, unsigned},
		storage_api, BlockInfo, StateChainStreamApi,
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

			async fn finalized_block_stream(&self) -> Box<dyn StateChainStreamApi>;
			async fn unfinalized_block_stream(&self) -> Box<dyn StateChainStreamApi<false>>;
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
	) -> Result<Vec<u32>> {
		let sparse_stream = tokio_stream::iter(block_numbers).map(move |num| {
			let hash = &chain.hashes[num as usize];
			Ok(chain.headers[hash].clone().into())
		});

		let stream = inject_intervening_headers(sparse_stream, rpc).await?;

		let headers = stream.collect::<Vec<_>>().await;

		Ok(headers.into_iter().map(|h| h.unwrap().number).collect::<Vec<_>>())
	}

	#[tokio::test]
	async fn test_intervening_headers() -> Result<()> {
		let (chain, rpc) = mock_chain_and_rpc(7);

		// Should fill in the gaps:
		assert_eq!(
			&inject_intervening_headers_with([0, 1, 3, 6], chain.clone(), rpc.clone()).await?,
			&[0, 1, 2, 3, 4, 5, 6]
		);

		// Already contiguous stream should be left unchanged:
		assert_eq!(
			&inject_intervening_headers_with([1, 2, 3, 4], chain.clone(), rpc.clone()).await?,
			&[1, 2, 3, 4]
		);

		// Empty stream results in an error (rather than panicking):
		assert!(&inject_intervening_headers_with([], chain.clone(), rpc.clone()).await.is_err());

		Ok(())
	}
}
