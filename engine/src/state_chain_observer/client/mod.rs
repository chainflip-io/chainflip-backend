pub mod base_rpc_api;
pub mod chain_api;
pub mod error_decoder;
pub mod extrinsic_api;
pub mod finalized_stream;
pub mod storage_api;

use async_trait::async_trait;

use anyhow::{anyhow, bail, Context, Result};
use cf_primitives::{AccountRole, SemVer};
use futures::{StreamExt, TryStreamExt};

use futures_core::Stream;
use sp_core::{Pair, H256};
use state_chain_runtime::AccountId;
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;
use tracing::info;

use utilities::{
	make_periodic_tick, read_clean_and_decode_hex_str_file, spmc,
	task_scope::{Scope, ScopedJoinHandle},
	CachedStream, MakeCachedStream,
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

#[derive(Clone)]
pub struct StreamCache {
	pub block_number: state_chain_runtime::BlockNumber,
	pub block_hash: state_chain_runtime::Hash,
}

pub trait StateChainStreamApi<const FINALIZED: bool = true>:
	CachedStream<
		Cache = StreamCache,
		Item = (state_chain_runtime::Hash, state_chain_runtime::Header),
	> + Send
	+ Sync
	+ Unpin
	+ 'static
{
}

impl<S, F> StateChainStreamApi<false> for utilities::InnerCachedStream<S, StreamCache, F>
where
	S: Stream<Item = (state_chain_runtime::Hash, state_chain_runtime::Header)>
		+ Send
		+ Sync
		+ Unpin
		+ 'static,
	F: FnMut(&S::Item) -> StreamCache + Send + Sync + Unpin + 'static,
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
	_block_producer_handles: (ScopedJoinHandle<()>, ScopedJoinHandle<()>),
	pub base_rpc_client: Arc<BaseRpcClient>,
	latest_finalized_block_hash_watcher: tokio::sync::watch::Receiver<state_chain_runtime::Hash>,
	latest_unfinalized_block_hash_watcher: tokio::sync::watch::Receiver<state_chain_runtime::Hash>,
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
				signing_key_file: signing_key_file.to_owned(), /* I have to take a clone here
				                                                * because of a compiler issue it
				                                                * seems */
				required_role,
				wait_for_required_role,
				check_unfinalized_version: required_version_and_wait.map(|(version, _)| version),
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
>(
	scope: &Scope<'_, anyhow::Error>,
	base_rpc_client: Arc<BaseRpcClient>,
	required_version_and_wait: Option<(SemVer, bool)>,
) -> Result<(watch::Receiver<H256>, impl StateChainStreamApi<false> + Clone, ScopedJoinHandle<()>)>
{
	let mut block_header_stream = {
		// https://substrate.stackexchange.com/questions/3667/api-rpc-chain-subscribefinalizedheads-missing-blocks
		// https://arxiv.org/abs/2007.01560
		let sparse_block_header_stream = base_rpc_client
			.subscribe_finalized_block_headers()
			.await?
			.map_err(Into::into)
			.chain(futures::stream::once(std::future::ready(Err(anyhow::anyhow!(
				"sparse_block_header_stream unexpectedly ended"
			)))));

		let base_rpc_client = base_rpc_client.clone();

		Box::pin(inject_intervening_headers(sparse_block_header_stream, base_rpc_client).await?)
	};

	let first_block_header = block_header_stream.next().await.unwrap()?;

	// Often `header` returns a significantly newer latest block than the stream
	// returns so we move the stream forward to this block
	let (mut latest_block_hash, latest_block_number) = {
		let header_hash = base_rpc_client.latest_finalized_block_hash().await?;
		let header = base_rpc_client.block_header(header_hash).await?;

		if first_block_header.number < header.number {
			let mut prev_stream_number = first_block_header.number;
			loop {
				let next_number = block_header_stream.next().await.unwrap()?.number;
				assert_eq!(next_number, prev_stream_number + 1, "{SUBSTRATE_BEHAVIOUR}");
				prev_stream_number = next_number;
				if next_number == header.number {
					break (header_hash, header.number)
				}
			}
		} else {
			(first_block_header.hash(), first_block_header.number)
		}
	};

	if let Some((required_version, wait_for_required_version)) = required_version_and_wait {
		if wait_for_required_version {
			let incompatible_blocks =
				futures::stream::once(futures::future::ready(Ok::<_, anyhow::Error>(
					latest_block_hash,
				)))
				.chain(futures::stream::try_unfold(
					(&mut block_header_stream, &mut latest_block_hash),
					|(block_header_stream, latest_block_hash)| async move {
						let next_header = block_header_stream.next().await.unwrap()?;
						*latest_block_hash = next_header.hash();
						Ok::<_, anyhow::Error>(Some((
							*latest_block_hash,
							(block_header_stream, latest_block_hash),
						)))
					},
				))
				.and_then(|block_hash| {
					let base_rpc_client = &base_rpc_client;
					async move {
						Ok::<_, anyhow::Error>((
							block_hash,
							base_rpc_client
								.storage_value::<pallet_cf_environment::CurrentReleaseVersion<
									state_chain_runtime::Runtime,
								>>(block_hash)
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
					latest_block_hash,
				)
				.await?;
			if !required_version.is_compatible_with(current_release_version) {
				bail!(
					"This version '{}' is incompatible with the current release '{}' at block: {}.",
					required_version,
					current_release_version,
					latest_block_hash,
				);
			}
		}
	}

	const BLOCK_CAPACITY: usize = 10;
	let (block_sender, block_receiver) =
		spmc::channel::<(state_chain_runtime::Hash, state_chain_runtime::Header)>(BLOCK_CAPACITY);

	let (latest_block_hash_sender, latest_block_hash_watcher) =
		tokio::sync::watch::channel::<state_chain_runtime::Hash>(latest_block_hash);

	Ok((
		latest_block_hash_watcher,
		block_receiver.make_cached(
			StreamCache {
				block_hash: latest_block_hash,
				block_number: latest_block_number,
			},
			|(block_hash, block_header): &(
				state_chain_runtime::Hash,
				state_chain_runtime::Header,
			)| StreamCache { block_hash: *block_hash, block_number: block_header.number },
		),

				scope.spawn_with_handle({
					let base_rpc_client = base_rpc_client.clone();
					async move {
						loop {
							let block_header =
								block_header_stream.next().await.unwrap()?;
							let block_hash = block_header.hash();
							if let Some((required_version, _)) = required_version_and_wait {
								let current_release_version = base_rpc_client.storage_value::<pallet_cf_environment::CurrentReleaseVersion<state_chain_runtime::Runtime>>(block_hash).await?;
								if !required_version.is_compatible_with(current_release_version) {
									break Err(anyhow!("This version '{}' is no longer compatible with the release version '{}' at block: {}", required_version, current_release_version, block_hash))
								}
							}

							if !block_sender.send((block_hash, block_header)).await {
								break Ok(())
							}
							if latest_block_hash_sender.send(block_hash).is_err() {
								break Ok(())
							}
						}
					}
				}),
	))
}

async fn create_unfinalized_block_subscription<
	BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
>(
	scope: &Scope<'_, anyhow::Error>,
	base_rpc_client: Arc<BaseRpcClient>,
) -> Result<(watch::Receiver<H256>, impl StateChainStreamApi<false> + Clone, ScopedJoinHandle<()>)>
{
	let mut block_header_stream = base_rpc_client
		.subscribe_unfinalized_block_headers()
		.await?
		.map_err(Into::into)
		.chain(futures::stream::once(std::future::ready(Err(anyhow::anyhow!(
			"sparse_block_header_stream unexpectedly ended"
		)))));

	let first_block_header = block_header_stream.next().await.unwrap()?;

	let latest_block_hash = first_block_header.hash();
	let latest_block_number = first_block_header.number;

	const BLOCK_CAPACITY: usize = 10;
	let (block_sender, block_receiver) =
		spmc::channel::<(state_chain_runtime::Hash, state_chain_runtime::Header)>(BLOCK_CAPACITY);

	let (latest_block_hash_sender, latest_block_hash_watcher) =
		tokio::sync::watch::channel::<state_chain_runtime::Hash>(latest_block_hash);

	Ok((
		latest_block_hash_watcher,
		block_receiver.make_cached(
			StreamCache { block_hash: latest_block_hash, block_number: latest_block_number },
			|(block_hash, block_header): &(
				state_chain_runtime::Hash,
				state_chain_runtime::Header,
			)| StreamCache { block_hash: *block_hash, block_number: block_header.number },
		),
		scope.spawn_with_handle(async move {
			loop {
				let block_header = block_header_stream.next().await.unwrap()?;
				let block_hash = block_header.hash();

				if !block_sender.send((block_hash, block_header)).await {
					break Ok(())
				}
				if latest_block_hash_sender.send(block_hash).is_err() {
					break Ok(())
				}
			}
		}),
	))
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
		signed_extrinsic_client_builder: SignedExtrinsicClientBuilder,
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
			latest_finalized_block_hash_watcher,
			mut finalized_state_chain_stream,
			finalized_block_producer_handle,
		) = {
			let (watcher, stream, handle) = create_finalized_block_subscription(
				scope,
				base_rpc_client.clone(),
				required_version_and_wait,
			)
			.await?;
			(watcher, FinalizedCachedStream::new(stream), handle)
		};

		let (
			latest_unfinalized_block_hash_watcher,
			unfinalized_state_chain_stream,
			unfinalized_block_producer_handle,
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
			base_rpc_client,
			_block_producer_handles: (
				finalized_block_producer_handle,
				unfinalized_block_producer_handle,
			),
			latest_finalized_block_hash_watcher,
			latest_unfinalized_block_hash_watcher,
		});

		info!(
			"Initialised StateChainClient at finalized block `{}`; block hash: `{:#x}`",
			finalized_state_chain_stream.cache().block_number,
			finalized_state_chain_stream.cache().block_hash
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
	signing_key_file: std::path::PathBuf,
	required_role: AccountRole,
	wait_for_required_role: bool,
	check_unfinalized_version: Option<SemVer>,
}
#[async_trait]
impl SignedExtrinsicClientBuilderTrait for SignedExtrinsicClientBuilder {
	type Client = extrinsic_api::signed::SignedExtrinsicClient;

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
		Self::Client::new(
			scope,
			base_rpc_client,
			signer::PairSigner::<sp_core::sr25519::Pair>::new(sp_core::sr25519::Pair::from_seed(
				&read_clean_and_decode_hex_str_file(
					&self.signing_key_file,
					"Signing Key",
					|str| {
						<[u8; 32]>::try_from(hex::decode(str)?).map_err(|e| {
							anyhow!("Failed to decode signing key: Wrong length. {e:?}")
						})
					},
				)?,
			)),
			self.required_role,
			self.wait_for_required_role,
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
	fn latest_finalized_hash(&self) -> state_chain_runtime::Hash {
		*self.latest_finalized_block_hash_watcher.borrow()
	}

	fn latest_unfinalized_hash(&self) -> state_chain_runtime::Hash {
		*self.latest_unfinalized_block_hash_watcher.borrow()
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
		storage_api,
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
			fn latest_finalized_hash(&self) -> state_chain_runtime::Hash;
			fn latest_unfinalized_hash(&self) -> state_chain_runtime::Hash;
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

async fn inject_intervening_headers<
	BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'static,
>(
	sparse_block_header_stream: impl Stream<Item = Result<state_chain_runtime::Header>> + Send + 'static,
	base_rpc_client: Arc<BaseRpcClient>,
) -> Result<impl Stream<Item = Result<state_chain_runtime::Header>>> {
	let mut sparse_block_header_stream = Box::pin(sparse_block_header_stream);

	let first_header: state_chain_runtime::Header =
		sparse_block_header_stream.next().await.unwrap()?;

	let mut latest_header = first_header.clone();

	let stream_rest = utilities::assert_stream_send(
		sparse_block_header_stream
			.and_then(move |next_header| {
				assert!(latest_header.number < next_header.number, "{SUBSTRATE_BEHAVIOUR}",);

				let prev_header = std::mem::replace(&mut latest_header, next_header.clone());

				let base_rpc_client = base_rpc_client.clone();
				async move {
					let base_rpc_client = &base_rpc_client;
					let intervening_headers: Vec<_> =
						futures::stream::iter(prev_header.number + 1..next_header.number)
							.then(|block_number| async move {
								let block_hash = base_rpc_client
									.block_hash(block_number)
									.await?
									.expect(SUBSTRATE_BEHAVIOUR);
								let block_header = base_rpc_client.block_header(block_hash).await?;
								assert_eq!(
									block_header.hash(),
									block_hash,
									"{SUBSTRATE_BEHAVIOUR}"
								);
								assert_eq!(
									block_header.number, block_number,
									"{SUBSTRATE_BEHAVIOUR}",
								);
								Result::<_, anyhow::Error>::Ok((block_hash, block_header))
							})
							.try_collect()
							.await?;

					for (block_hash, next_block_header) in Iterator::zip(
						std::iter::once(&prev_header.hash())
							.chain(intervening_headers.iter().map(|(hash, _header)| hash)),
						intervening_headers
							.iter()
							.map(|(_hash, header)| header)
							.chain(std::iter::once(&next_header)),
					) {
						assert_eq!(*block_hash, next_block_header.parent_hash);
					}

					Result::<_, anyhow::Error>::Ok(futures::stream::iter(
						intervening_headers
							.into_iter()
							.map(|(_hash, header)| header)
							.chain(std::iter::once(next_header))
							.map(Result::<_, anyhow::Error>::Ok),
					))
				}
			})
			.try_flatten(),
	);

	Ok(futures::stream::once(async { Ok(first_header) }).chain(stream_rest))
}

#[cfg(test)]
mod tests {

	use std::collections::BTreeMap;

	use state_chain_runtime::Header;

	use crate::state_chain_observer::test_helpers::test_header;

	use super::{base_rpc_api::MockBaseRpcApi, *};

	struct TestChain {
		hashes: Vec<H256>,
		headers: BTreeMap<H256, Header>,
	}

	impl TestChain {
		fn new(total_blocks: usize) -> TestChain {
			let mut headers = Vec::<Header>::with_capacity(total_blocks);

			for index in 0..total_blocks {
				let parent_hash = index.checked_sub(1).map(|parent_i| headers[parent_i].hash());
				let header = test_header(index as u32, parent_hash);
				headers.push(header);
			}

			let hashes = headers.iter().map(|header| header.hash()).collect();

			let headers: BTreeMap<_, _> =
				headers.into_iter().map(|header| (header.hash(), header)).collect();

			TestChain { hashes, headers }
		}
	}

	#[tokio::test]
	async fn test_intervening_headers() -> Result<()> {
		use futures::stream::StreamExt;
		let chain = Arc::new(TestChain::new(7));
		let mut rpc = MockBaseRpcApi::new();

		rpc.expect_block_hash().returning({
			let chain = chain.clone();
			move |num| Ok(chain.hashes.get(num as usize).cloned())
		});

		rpc.expect_block_header().returning({
			let chain = chain.clone();
			move |hash| Ok(chain.headers.get(&hash).expect("unknown hash").clone())
		});

		let sparse_stream = tokio_stream::iter([0, 1, 3, 6].map(|num| {
			let hash = &chain.hashes[num];
			Ok(chain.headers[hash].clone())
		}));

		let mut sparse_stream = sparse_stream.peekable();

		dbg!(std::pin::Pin::new(&mut sparse_stream).peek().await);

		let stream = inject_intervening_headers(sparse_stream, Arc::new(rpc)).await?;

		let headers = stream.collect::<Vec<_>>().await;

		assert_eq!(
			&headers.into_iter().map(|h| h.unwrap().number).collect::<Vec<_>>(),
			&[0, 1, 2, 3, 4, 5, 6]
		);

		Ok(())
	}
}
