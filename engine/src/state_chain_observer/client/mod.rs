pub mod base_rpc_api;
pub mod extrinsic_api;
mod signer;
pub mod storage_api;

use base_rpc_api::BaseRpcApi;

use anyhow::{anyhow, bail, Context, Result};
use codec::Decode;
use futures::{Stream, StreamExt, TryFutureExt, TryStreamExt};

use slog::o;
use sp_core::{storage::StorageKey, Pair, H256};
use std::sync::{atomic::AtomicU32, Arc};
use tokio::sync::RwLock;

use crate::{
	common::{read_clean_and_decode_hex_str_file, EngineTryStreamExt},
	logging::COMPONENT_KEY,
	settings,
	task_scope::{Scope, ScopedJoinHandle},
};
use utilities::context;

pub struct StateChainClient<
	BaseRpcClient = base_rpc_api::BaseRpcClient<jsonrpsee::ws_client::WsClient>,
> {
	nonce: AtomicU32,
	runtime_version: RwLock<sp_version::RuntimeVersion>,
	genesis_hash: state_chain_runtime::Hash,
	signer: signer::PairSigner<sp_core::sr25519::Pair>,
	_task_handle: ScopedJoinHandle<()>,
	pub base_rpc_client: Arc<BaseRpcClient>,
}

impl<BaseRpcClient> StateChainClient<BaseRpcClient> {
	pub fn get_genesis_hash(&self) -> state_chain_runtime::Hash {
		self.genesis_hash
	}
}

impl StateChainClient {
	pub async fn new<'a>(
		scope: &Scope<'a, anyhow::Error>,
		state_chain_settings: &settings::StateChain,
		wait_for_staking: bool,
		logger: &slog::Logger,
	) -> Result<(H256, impl Stream<Item = state_chain_runtime::Header>, Arc<StateChainClient>)> {
		Self::inner_new(scope, state_chain_settings, wait_for_staking, logger)
			.await
			.context("Failed to initialize StateChainClient")
	}

	async fn inner_new<'a>(
		scope: &Scope<'a, anyhow::Error>,
		state_chain_settings: &settings::StateChain,
		wait_for_staking: bool,
		logger: &slog::Logger,
	) -> Result<(H256, impl Stream<Item = state_chain_runtime::Header>, Arc<StateChainClient>)> {
		let logger = logger.new(o!(COMPONENT_KEY => "StateChainClient"));
		let signer = signer::PairSigner::<sp_core::sr25519::Pair>::new(
			sp_core::sr25519::Pair::from_seed(&read_clean_and_decode_hex_str_file(
				&state_chain_settings.signing_key_file,
				"Signing Key",
				|str| {
					<[u8; 32]>::try_from(hex::decode(str).map_err(anyhow::Error::new)?)
						.map_err(|_err| anyhow!("Wrong length"))
				},
			)?),
		);

		let account_storage_key =
			StorageKey(frame_system::Account::<state_chain_runtime::Runtime>::hashed_key_for(
				&signer.account_id,
			));

		let base_rpc_client =
			Arc::new(base_rpc_api::BaseRpcClient::new(state_chain_settings).await?);

		let (first_finalized_block_header, mut finalized_block_header_stream) = {
			// https://substrate.stackexchange.com/questions/3667/api-rpc-chain-subscribefinalizedheads-missing-blocks
			// https://arxiv.org/abs/2007.01560
			let mut sparse_finalized_block_header_stream = base_rpc_client
				.subscribe_finalized_block_headers()
				.await?
				.map_err(Into::into)
				.chain(futures::stream::once(std::future::ready(Err(anyhow::anyhow!(
					"sparse_finalized_block_header_stream unexpectedly ended"
				)))));

			let mut latest_finalized_header: state_chain_runtime::Header =
				sparse_finalized_block_header_stream.next().await.unwrap()?;
			let base_rpc_client = base_rpc_client.clone();

			(
				latest_finalized_header.clone(),
				Box::pin(
					sparse_finalized_block_header_stream
						.and_then(move |next_finalized_header| {
							assert!(latest_finalized_header.number < next_finalized_header.number);

							let prev_finalized_header = std::mem::replace(
								&mut latest_finalized_header,
								next_finalized_header.clone(),
							);

							let base_rpc_client = base_rpc_client.clone();
							async move {
								let base_rpc_client = &base_rpc_client;
								let intervening_headers: Vec<_> = futures::stream::iter(
									prev_finalized_header.number + 1..next_finalized_header.number,
								)
								.then(|block_number| async move {
									let block_hash =
										base_rpc_client.block_hash(block_number).await?.unwrap();
									let block_header =
										base_rpc_client.block_header(block_hash).await?;
									assert_eq!(block_header.hash(), block_hash);
									assert_eq!(block_header.number, block_number);
									Result::<_, anyhow::Error>::Ok((block_hash, block_header))
								})
								.try_collect()
								.await?;

								for (block_hash, next_block_header) in Iterator::zip(
									std::iter::once(&prev_finalized_header.hash()).chain(
										intervening_headers.iter().map(|(hash, _header)| hash),
									),
									intervening_headers
										.iter()
										.map(|(_hash, header)| header)
										.chain(std::iter::once(&next_finalized_header)),
								) {
									assert_eq!(*block_hash, next_block_header.parent_hash);
								}

								Result::<_, anyhow::Error>::Ok(futures::stream::iter(
									intervening_headers
										.into_iter()
										.map(|(_hash, header)| header)
										.chain(std::iter::once(next_finalized_header))
										.map(Result::<_, anyhow::Error>::Ok),
								))
							}
						})
						.end_after_error()
						.try_flatten(),
				),
			)
		};

		// Often `finalized_header` returns a significantly newer latest block than the stream
		// returns so we move the stream forward to this block
		let (mut latest_block_hash, mut latest_block_number) = {
			let finalised_header_hash = base_rpc_client.latest_finalized_block_hash().await?;
			let finalised_header = base_rpc_client.block_header(finalised_header_hash).await?;

			if first_finalized_block_header.number < finalised_header.number {
				for block_number in
					first_finalized_block_header.number + 1..=finalised_header.number
				{
					assert_eq!(
						finalized_block_header_stream.next().await.unwrap()?.number,
						block_number
					);
				}
				(finalised_header_hash, finalised_header.number)
			} else {
				(first_finalized_block_header.hash(), first_finalized_block_header.number)
			}
		};

		let (latest_block_hash, latest_block_number, account_nonce) = {
			async fn get_account_nonce<
				StateChainRpcClient: base_rpc_api::BaseRpcApi + Send + Sync,
			>(
				state_rpc_client: &StateChainRpcClient,
				account_storage_key: &StorageKey,
				block_hash: state_chain_runtime::Hash,
			) -> Result<Option<u32>> {
				Ok(
					if let Some(encoded_account_info) =
						state_rpc_client.storage(block_hash, account_storage_key.clone()).await?
					{
						let account_info: frame_system::AccountInfo<
							state_chain_runtime::Index,
							<state_chain_runtime::Runtime as frame_system::Config>::AccountData,
						> = context!(Decode::decode(&mut &encoded_account_info.0[..])).unwrap();
						Some(account_info.nonce)
					} else {
						None
					},
				)
			}

			let base_rpc_client = base_rpc_client.as_ref();

			let account_nonce = match get_account_nonce(
				base_rpc_client,
				&account_storage_key,
				latest_block_hash,
			)
			.await?
			{
				Some(nonce) => nonce,
				None =>
					if wait_for_staking {
						loop {
							if let Some(nonce) = get_account_nonce(
								base_rpc_client,
								&account_storage_key,
								latest_block_hash,
							)
							.await?
							{
								break nonce
							} else {
								slog::warn!(logger, "Your Chainflip account {} is not staked. WAITING for account to be staked at block: {}", signer.account_id, latest_block_number);
								let block_header =
									finalized_block_header_stream.next().await.unwrap()?;
								latest_block_hash = block_header.hash();
								latest_block_number += 1;
								assert_eq!(latest_block_number, block_header.number);
							}
						}
					} else {
						bail!("Your Chainflip account {} is not staked", signer.account_id);
					},
			};

			(latest_block_hash, latest_block_number, account_nonce)
		};

		const BLOCK_CAPACITY: usize = 10;

		let (block_sender, block_receiver) = async_channel::bounded(BLOCK_CAPACITY);
		let task_handle = scope.spawn_with_handle(async move {
			finalized_block_header_stream
				.try_for_each(|block_header| {
					block_sender.send(block_header).map_err(anyhow::Error::new)
				})
				.await
		});

		let state_chain_client = Arc::new(StateChainClient {
			nonce: AtomicU32::new(account_nonce),
			runtime_version: RwLock::new(base_rpc_client.runtime_version().await?),
			genesis_hash: base_rpc_client.block_hash(0).await?.unwrap(),
			signer: signer.clone(),
			_task_handle: task_handle,
			base_rpc_client,
		});

		slog::info!(
			logger,
			"Initialised StateChainClient at block `{}`; block hash: `{:#x}`",
			latest_block_number,
			latest_block_hash
		);

		Ok((latest_block_hash, block_receiver, state_chain_client))
	}
}

#[cfg(test)]
pub mod mocks {
	use crate::state_chain_observer::client::{
		extrinsic_api::ExtrinsicApi, storage_api::StorageApi,
	};
	use anyhow::Result;
	use async_trait::async_trait;
	use frame_support::storage::types::QueryKindTrait;
	use futures::Stream;
	use jsonrpsee::core::RpcResult;
	use mockall::mock;
	use sp_core::{storage::StorageKey, H256};
	use state_chain_runtime::AccountId;

	use super::storage_api::{
		StorageDoubleMapAssociatedTypes, StorageMapAssociatedTypes, StorageValueAssociatedTypes,
	};

	mock! {
		pub StateChainClient {}
		#[async_trait]
		impl ExtrinsicApi for StateChainClient {
			fn account_id(&self) -> AccountId;

			async fn submit_signed_extrinsic<Call>(
				&self,
				call: Call,
				logger: &slog::Logger,
			) -> Result<H256>
			where
				Call: Into<state_chain_runtime::Call> + Clone + std::fmt::Debug + Send + Sync + 'static;

			async fn submit_unsigned_extrinsic<Call>(
				&self,
				call: Call,
				logger: &slog::Logger,
			) -> Result<H256>
			where
				Call: Into<state_chain_runtime::Call> + Clone + std::fmt::Debug + Send + Sync + 'static;

			async fn watch_submitted_extrinsic<BlockStream>(
				&self,
				extrinsic_hash: state_chain_runtime::Hash,
				block_stream: &mut BlockStream,
			) -> Result<Vec<state_chain_runtime::Event>>
			where
				BlockStream:
					Stream<Item = state_chain_runtime::Header> + Unpin + Send + 'static;
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

			async fn storage_value<StorageValue: StorageValueAssociatedTypes + 'static>(
				&self,
				block_hash: state_chain_runtime::Hash,
			) -> RpcResult<<StorageValue::QueryKind as QueryKindTrait<StorageValue::Value, StorageValue::OnEmpty>>::Query>;

			async fn storage_map_entry<StorageMap: StorageMapAssociatedTypes + 'static>(
				&self,
				block_hash: state_chain_runtime::Hash,
				key: &StorageMap::Key,
			) -> RpcResult<
				<StorageMap::QueryKind as QueryKindTrait<StorageMap::Value, StorageMap::OnEmpty>>::Query,
			>
			where
				StorageMap::Key: Sync;

			async fn storage_double_map_entry<StorageDoubleMap: StorageDoubleMapAssociatedTypes + 'static>(
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

			async fn storage_map<StorageMap: StorageMapAssociatedTypes + 'static>(
				&self,
				block_hash: state_chain_runtime::Hash,
			) -> RpcResult<Vec<(<StorageMap as StorageMapAssociatedTypes>::Key, StorageMap::Value)>>;
		}
	}
}
