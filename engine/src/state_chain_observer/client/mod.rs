pub mod base_rpc_api;
pub mod extrinsic_api;
mod signer;
pub mod storage_api;

use base_rpc_api::BaseRpcApi;

use anyhow::{anyhow, bail, Context, Result};
use cf_primitives::AccountRole;
use futures::{Stream, StreamExt, TryFutureExt, TryStreamExt};

use slog::o;
use sp_core::{Pair, H256};
use std::sync::{atomic::AtomicU32, Arc};
use tokio::sync::RwLock;

use crate::{
	common::{read_clean_and_decode_hex_str_file, EngineTryStreamExt},
	logging::COMPONENT_KEY,
	settings,
	state_chain_observer::client::storage_api::StorageApi,
	task_scope::{Scope, ScopedJoinHandle},
};

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

fn assert_stream_send<'u, R>(
	strm: impl 'u + Send + Stream<Item = R>,
) -> impl 'u + Send + Stream<Item = R> {
	strm
}

impl StateChainClient {
	pub async fn new<'a>(
		scope: &Scope<'a, anyhow::Error>,
		state_chain_settings: &settings::StateChain,
		required_role: AccountRole,
		wait_for_required_role: bool,
		logger: &slog::Logger,
	) -> Result<(H256, impl Stream<Item = state_chain_runtime::Header>, Arc<StateChainClient>)> {
		Self::inner_new(scope, state_chain_settings, required_role, wait_for_required_role, logger)
			.await
			.context("Failed to initialize StateChainClient")
	}

	async fn inner_new<'a>(
		scope: &Scope<'a, anyhow::Error>,
		state_chain_settings: &settings::StateChain,
		required_role: AccountRole,
		wait_for_required_role: bool,
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
				assert_stream_send(Box::pin(
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
				)),
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
			loop {
				match base_rpc_client
					.storage_map_entry::<pallet_cf_account_roles::AccountRoles<state_chain_runtime::Runtime>>(
						latest_block_hash,
						&signer.account_id,
					)
					.await?
				{
					Some(role) =>
						if required_role == AccountRole::None || required_role == role {
							break
						} else if wait_for_required_role && role == AccountRole::None {
							slog::warn!(logger, "Your Chainflip account {} does not have an assigned account role. WAITING for the account role to be set to '{:?}' at block: {}", signer.account_id, required_role, latest_block_number);
						} else {
							bail!("Your Chainflip account {} has the wrong account role '{:?}'. The '{:?}' account role is required", signer.account_id, role, required_role);
						},
					None =>
						if wait_for_required_role {
							slog::warn!(logger, "Your Chainflip account {} is not staked. Note, if you have already staked, it may take some time for your stake to be detected. WAITING for your account to be staked at block: {}", signer.account_id, latest_block_number);
						} else {
							bail!("Your Chainflip account {} is not staked", signer.account_id);
						},
				}

				let block_header = finalized_block_header_stream.next().await.unwrap()?;
				latest_block_hash = block_header.hash();
				latest_block_number += 1;
				assert_eq!(latest_block_number, block_header.number);
			}

			(
				latest_block_hash,
				latest_block_number,
				base_rpc_client
					.storage_map_entry::<frame_system::Account<state_chain_runtime::Runtime>>(
						latest_block_hash,
						&signer.account_id,
					)
					.await?
					.nonce,
			)
		};

		const BLOCK_CAPACITY: usize = 10;

		let (block_sender, block_receiver) = async_broadcast::broadcast(BLOCK_CAPACITY);
		let task_handle = scope.spawn_with_handle(async move {
			finalized_block_header_stream
				.try_for_each(|block_header| {
					block_sender.broadcast(block_header).map_err(anyhow::Error::new).map_ok(|_| ())
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
				Call: Into<state_chain_runtime::RuntimeCall> + Clone + std::fmt::Debug + Send + Sync + 'static;

			async fn submit_unsigned_extrinsic<Call>(
				&self,
				call: Call,
				logger: &slog::Logger,
			) -> Result<H256>
			where
				Call: Into<state_chain_runtime::RuntimeCall> + Clone + std::fmt::Debug + Send + Sync + 'static;

			async fn watch_submitted_extrinsic<BlockStream>(
				&self,
				extrinsic_hash: state_chain_runtime::Hash,
				block_stream: &mut BlockStream,
			) -> Result<Vec<state_chain_runtime::RuntimeEvent>>
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
