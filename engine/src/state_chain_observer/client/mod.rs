pub mod base_rpc_api;
pub mod extrinsic_api;
pub mod storage_api;

#[cfg(test)]
mod tests;

use async_trait::async_trait;

use anyhow::{anyhow, Context, Result};
use cf_primitives::{AccountRole, BlockNumber};
use futures::{StreamExt, TryStreamExt};

use sp_core::{Pair, H256};
use state_chain_runtime::{AccountId, Header};
use std::{sync::Arc, time::Duration};
use tracing::info;

use utilities::{
	make_periodic_tick, read_clean_and_decode_hex_str_file,
	task_scope::{Scope, ScopedJoinHandle},
	CachedStream, MakeCachedStream,
};

use self::{
	base_rpc_api::BaseRpcClient,
	extrinsic_api::signed::{signer, SignedExtrinsicApi},
};

#[cfg(test)]
pub use self::extrinsic_api::signed::test_header;

/// For expressing an expectation regarding substrate's behaviour (Not our chain though)
const SUBSTRATE_BEHAVIOUR: &str = "Unexpected state chain node behaviour";

const SYNC_POLL_INTERVAL: Duration = Duration::from_secs(4);

#[derive(Clone)]
pub struct StreamCache {
	pub block_number: state_chain_runtime::BlockNumber,
	pub block_hash: state_chain_runtime::Hash,
}

pub trait StateChainStreamApi:
	CachedStream<Cache = StreamCache, Item = (state_chain_runtime::Hash, Header)>
	+ Send
	+ Sync
	+ Unpin
	+ 'static
{
}
impl<
		T: CachedStream<Cache = StreamCache, Item = (state_chain_runtime::Hash, Header)>
			+ Send
			+ Sync
			+ Unpin
			+ 'static,
	> StateChainStreamApi for T
{
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
	unsigned_extrinsic_client: extrinsic_api::unsigned::UnsignedExtrinsicClient,
	_block_producer: ScopedJoinHandle<()>,
	pub base_rpc_client: Arc<BaseRpcClient>,
}

impl StateChainClient<extrinsic_api::signed::SignedExtrinsicClient> {
	pub async fn connect_with_account<'a>(
		scope: &Scope<'a, anyhow::Error>,
		ws_endpoint: &str,
		signing_key_file: &std::path::Path,
		required_role: AccountRole,
		wait_for_required_role: bool,
	) -> Result<(impl StateChainStreamApi + Clone, Arc<Self>)> {
		Self::new_with_account(
			scope,
			DefaultRpcClient::connect(ws_endpoint).await?.into(),
			signing_key_file,
			required_role,
			wait_for_required_role,
		)
		.await
	}
}

impl StateChainClient<()> {
	pub async fn connect_without_account<'a>(
		scope: &Scope<'a, anyhow::Error>,
		ws_endpoint: &str,
	) -> Result<(impl StateChainStreamApi + Clone, Arc<Self>)> {
		Self::new_without_account(scope, DefaultRpcClient::connect(ws_endpoint).await?.into()).await
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
	) -> Result<(impl StateChainStreamApi + Clone, Arc<Self>)> {
		Self::new(
			scope,
			base_rpc_client,
			SignedExtrinsicClientBuilder {
				signing_key_file: signing_key_file.to_owned(), /* I have to take a clone here
				                                                * because of a compiler issue it
				                                                * seems */
				required_role,
				wait_for_required_role,
			},
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
	) -> Result<(impl StateChainStreamApi + Clone, Arc<Self>)> {
		Self::new(scope, base_rpc_client, ()).await
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
		signed_extrinsic_client_builder: SignedExtrinsicClientBuilder,
	) -> Result<(impl StateChainStreamApi + Clone, Arc<Self>)> {
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

		let (first_finalized_block_header, mut finalized_block_header_stream) =
			get_finalized_block_header_stream(base_rpc_client.clone()).await?;

		// On startup we don't care about the old blocks, so just fast forward the stream to the
		// latest block
		let (latest_block_hash, latest_block_number) = fast_forward_finalized_stream_to_latest(
			first_finalized_block_header,
			&mut finalized_block_header_stream,
			&base_rpc_client,
		)
		.await?;

		// Set up the cached state chain stream
		const BLOCK_CAPACITY: usize = 10;
		let (block_sender, block_receiver) =
			async_broadcast::broadcast::<(state_chain_runtime::Hash, Header)>(BLOCK_CAPACITY);

		let mut state_chain_stream = block_receiver.make_cached(
			StreamCache { block_hash: latest_block_hash, block_number: latest_block_number },
			|(block_hash, block_header): &(state_chain_runtime::Hash, Header)| StreamCache {
				block_hash: *block_hash,
				block_number: block_header.number,
			},
		);

		// Spawn a task that will send block headers to the cached state chain stream
		let block_producer = scope.spawn_with_handle({
			async move {
				loop {
					let block_header = finalized_block_header_stream.next().await.unwrap()?;
					if block_sender.broadcast((block_header.hash(), block_header)).await.is_err() {
						break Ok(())
					}
				}
			}
		});

		let state_chain_client = Arc::new(StateChainClient {
			genesis_hash,
			signed_extrinsic_client: signed_extrinsic_client_builder
				.build(scope, base_rpc_client.clone(), genesis_hash, &mut state_chain_stream)
				.await?,
			unsigned_extrinsic_client: extrinsic_api::unsigned::UnsignedExtrinsicClient::new(
				scope,
				base_rpc_client.clone(),
			),
			_block_producer: block_producer,
			base_rpc_client,
		});

		info!(
			"Initialised StateChainClient at block `{}`; block hash: `{:#x}`",
			state_chain_stream.cache().block_number,
			state_chain_stream.cache().block_hash
		);

		Ok((state_chain_stream, state_chain_client))
	}

	pub fn genesis_hash(&self) -> state_chain_runtime::Hash {
		self.genesis_hash
	}
}

/// Get the intervening (missing) headers from the gap between 2 blocks
async fn get_intervening_headers<BaseRpcClient: base_rpc_api::BaseRpcApi>(
	from_block_number: BlockNumber,
	to_block_number: BlockNumber,
	base_rpc_client: &Arc<BaseRpcClient>,
) -> Result<Vec<(H256, Header)>> {
	futures::stream::iter(from_block_number..to_block_number)
		.then(|block_number| async move {
			let block_hash =
				base_rpc_client.block_hash(block_number).await?.expect(SUBSTRATE_BEHAVIOUR);
			let block_header = base_rpc_client.block_header(block_hash).await?;
			assert_eq!(block_header.hash(), block_hash, "{SUBSTRATE_BEHAVIOUR}");
			assert_eq!(block_header.number, block_number, "{SUBSTRATE_BEHAVIOUR}",);
			Result::<_, anyhow::Error>::Ok((block_hash, block_header))
		})
		.try_collect()
		.await
}

/// Get an iterator of consecutive headers after the `from_header` (from_header+1..=to_header)
async fn get_consecutive_headers<BaseRpcClient: base_rpc_api::BaseRpcApi>(
	from_header: Header,
	to_header: Header,
	base_rpc_client: &Arc<BaseRpcClient>,
) -> Result<impl Iterator<Item = Result<Header, anyhow::Error>>, anyhow::Error> {
	let intervening_headers =
		get_intervening_headers(from_header.number + 1, to_header.number, base_rpc_client).await?;

	// Make sure that the intervening headers fill the gap between the headers perfectly by checking
	// parent hashes
	for (block_hash, next_block_header) in Iterator::zip(
		std::iter::once(&from_header.hash())
			.chain(intervening_headers.iter().map(|(hash, _header)| hash)),
		intervening_headers
			.iter()
			.map(|(_hash, header)| header)
			.chain(std::iter::once(&to_header)),
	) {
		assert_eq!(*block_hash, next_block_header.parent_hash);
	}

	// Return the intervening headers first and then the `to_header` to form a consecutive iterator
	Ok(intervening_headers
		.into_iter()
		.map(|(_hash, header)| header)
		.chain(std::iter::once(to_header))
		.map(Result::<_, anyhow::Error>::Ok))
}

/// Creates a stream of consecutive finalized block headers
async fn get_finalized_block_header_stream<'a, BaseRpcClient>(
	base_rpc_client: Arc<BaseRpcClient>,
) -> Result<(Header, impl futures_core::Stream<Item = Result<Header>> + 'a)>
where
	BaseRpcClient: base_rpc_api::BaseRpcApi + Send + Sync + 'a,
{
	// Sometimes more than one block is finalized at once, so this stream may have missing blocks
	// that we need to fill in.
	// https://substrate.stackexchange.com/questions/3667/api-rpc-chain-subscribefinalizedheads-missing-blocks
	// https://arxiv.org/abs/2007.01560
	let mut sparse_finalized_block_header_stream = base_rpc_client
		.subscribe_finalized_block_headers()
		.await?
		.map_err(Into::into)
		.chain(futures::stream::once(std::future::ready(Err(anyhow::anyhow!(
			"sparse_finalized_block_header_stream unexpectedly ended"
		)))));

	let mut latest_finalized_header: Header =
		sparse_finalized_block_header_stream.next().await.unwrap()?;

	let latest_finalized_header_clone = latest_finalized_header.clone();

	let finalized_block_header_stream = utilities::assert_stream_send(Box::pin(
		// Get the next finalized header and check for missing blocks
		sparse_finalized_block_header_stream
			.and_then(move |next_finalized_header| {
				assert!(
					latest_finalized_header.number < next_finalized_header.number,
					"{SUBSTRATE_BEHAVIOUR}",
				);

				// Remember the previous finalized header so we can detect a gap
				let prev_finalized_header =
					std::mem::replace(&mut latest_finalized_header, next_finalized_header.clone());

				let base_rpc_client = base_rpc_client.clone();
				async move {
					Result::<_, anyhow::Error>::Ok(futures::stream::iter(
						get_consecutive_headers(
							prev_finalized_header,
							next_finalized_header,
							&base_rpc_client,
						)
						.await?,
					))
				}
			})
			.try_flatten(),
	));

	Ok((latest_finalized_header_clone, finalized_block_header_stream))
}

/// Bring the finalized block header stream up to date with the latest finalized block
async fn fast_forward_finalized_stream_to_latest<BaseRpcClient, Stream>(
	from_block_header: Header,
	finalized_block_header_stream: &mut Stream,
	base_rpc_client: &Arc<BaseRpcClient>,
) -> Result<(H256, BlockNumber)>
where
	BaseRpcClient: base_rpc_api::BaseRpcApi,
	Stream: futures_core::Stream<Item = Result<Header>> + Unpin,
{
	let finalised_header_hash = base_rpc_client.latest_finalized_block_hash().await?;
	let finalised_header = base_rpc_client.block_header(finalised_header_hash).await?;

	Ok(if from_block_header.number < finalised_header.number {
		for block_number in from_block_header.number + 1..=finalised_header.number {
			assert_eq!(
				finalized_block_header_stream.next().await.unwrap()?.number,
				block_number,
				"{SUBSTRATE_BEHAVIOUR}"
			);
		}
		(finalised_header_hash, finalised_header.number)
	} else {
		(from_block_header.hash(), from_block_header.number)
	})
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

	fn account_id(&self) -> AccountId {
		self.signed_extrinsic_client.account_id()
	}

	/// Submit an signed extrinsic, returning the hash of the submission
	async fn submit_signed_extrinsic<Call>(&self, call: Call) -> (H256, Self::UntilFinalizedFuture)
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
	async fn finalize_signed_extrinsic<Call>(&self, call: Call) -> Self::UntilFinalizedFuture
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
	> extrinsic_api::unsigned::UnsignedExtrinsicApi
	for StateChainClient<SignedExtrinsicClient, BaseRpcApi>
{
	/// Submit an unsigned extrinsic.
	async fn submit_unsigned_extrinsic<Call>(&self, call: Call) -> H256
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

#[cfg(test)]
pub mod mocks {
	use crate::state_chain_observer::client::{
		extrinsic_api::{signed::SignedExtrinsicApi, unsigned::UnsignedExtrinsicApi},
		storage_api::StorageApi,
	};
	use async_trait::async_trait;
	use frame_support::storage::types::QueryKindTrait;
	use jsonrpsee::core::RpcResult;
	use mockall::mock;
	use sp_core::{storage::StorageKey, H256};
	use state_chain_runtime::AccountId;

	use super::{extrinsic_api, storage_api};

	mock! {
		pub StateChainClient {}
		#[async_trait]
		impl SignedExtrinsicApi for StateChainClient {
			type UntilFinalizedFuture = extrinsic_api::signed::MockUntilFinalized;

			fn account_id(&self) -> AccountId;

			async fn submit_signed_extrinsic<Call>(&self, call: Call) -> (H256, <Self as SignedExtrinsicApi>::UntilFinalizedFuture)
			where
				Call: Into<state_chain_runtime::RuntimeCall>
					+ Clone
					+ std::fmt::Debug
					+ Send
					+ Sync
					+ 'static;

			async fn finalize_signed_extrinsic<Call>(&self, call: Call) -> <Self as SignedExtrinsicApi>::UntilFinalizedFuture
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
			) -> H256
			where
				Call: Into<state_chain_runtime::RuntimeCall> + Clone + std::fmt::Debug + Send + Sync + 'static;
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

			async fn storage_map<StorageMap: storage_api::StorageMapAssociatedTypes + 'static>(
				&self,
				block_hash: state_chain_runtime::Hash,
			) -> RpcResult<Vec<(<StorageMap as storage_api::StorageMapAssociatedTypes>::Key, StorageMap::Value)>>;
		}
	}
}
