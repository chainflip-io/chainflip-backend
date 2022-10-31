mod signer;
pub mod storage_traits;

use anyhow::{anyhow, bail, Context, Result};
use codec::{Decode, Encode, FullCodec};
use custom_rpc::CustomApiClient;
use frame_support::{pallet_prelude::InvalidTransaction, storage::types::QueryKindTrait};
use frame_system::Phase;
use futures::{Stream, StreamExt, TryStreamExt};
use jsonrpsee::{
	core::{
		client::{ClientT, SubscriptionClientT},
		Error as RpcError, RpcResult,
	},
	types::{error::CallError, ErrorObject, ErrorObjectOwned},
	ws_client::WsClientBuilder,
};
use slog::o;
use sp_core::{
	storage::{StorageData, StorageKey},
	Bytes, Pair, H256,
};
use sp_runtime::{
	generic::Era,
	traits::{BlakeTwo256, Hash},
	AccountId32, MultiAddress,
};
use sp_version::RuntimeVersion;
use state_chain_runtime::SignedBlock;
use std::sync::{
	atomic::{AtomicU32, Ordering},
	Arc,
};
use tokio::sync::RwLock;

use crate::{
	common::{read_clean_and_decode_hex_str_file, EngineTryStreamExt},
	constants::MAX_EXTRINSIC_RETRY_ATTEMPTS,
	logging::COMPONENT_KEY,
	settings,
};
use utilities::context;

#[cfg(test)]
use mockall::automock;

use async_trait::async_trait;

use sc_rpc_api::{
	author::AuthorApiClient, chain::ChainApiClient, state::StateApiClient, system::SystemApiClient,
};

pub trait ChainflipClient:
	CustomApiClient
	+ SystemApiClient<state_chain_runtime::Hash, state_chain_runtime::BlockNumber>
	+ StateApiClient<state_chain_runtime::Hash>
	+ AuthorApiClient<
		state_chain_runtime::Hash,
		<state_chain_runtime::Block as sp_runtime::traits::Block>::Hash,
	> + ChainApiClient<
		state_chain_runtime::BlockNumber,
		state_chain_runtime::Hash,
		state_chain_runtime::Header,
		state_chain_runtime::SignedBlock,
	>
{
}

impl<
		T: SubscriptionClientT
			+ ClientT
			+ CustomApiClient
			+ SystemApiClient<state_chain_runtime::Hash, state_chain_runtime::BlockNumber>
			+ StateApiClient<state_chain_runtime::Hash>
			+ AuthorApiClient<
				state_chain_runtime::Hash,
				<state_chain_runtime::Block as sp_runtime::traits::Block>::Hash,
			> + ChainApiClient<
				state_chain_runtime::BlockNumber,
				state_chain_runtime::Hash,
				state_chain_runtime::Header,
				state_chain_runtime::SignedBlock,
			>,
	> ChainflipClient for T
{
}

pub struct StateChainRpcClient<C: ChainflipClient> {
	rpc_client: Arc<C>,
}

/// Wraps the substrate client library methods
#[cfg_attr(test, automock)]
#[async_trait]
pub trait StateChainRpcApi {
	async fn submit_extrinsic(
		&self,
		extrinsic: state_chain_runtime::UncheckedExtrinsic,
	) -> RpcResult<sp_core::H256>;

	async fn storage(
		&self,
		block_hash: state_chain_runtime::Hash,
		storage_key: StorageKey,
	) -> RpcResult<Option<StorageData>>;

	async fn storage_pairs(
		&self,
		block_hash: state_chain_runtime::Hash,
		storage_key: StorageKey,
	) -> RpcResult<Vec<(StorageKey, StorageData)>>;

	async fn get_block(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<Option<SignedBlock>>;

	async fn latest_block_hash(&self) -> RpcResult<H256>;

	async fn rotate_keys(&self) -> RpcResult<Bytes>;

	async fn fetch_runtime_version(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<RuntimeVersion>;

	async fn is_auction_phase(&self) -> RpcResult<bool>;
}

#[async_trait]
impl<C> StateChainRpcApi for StateChainRpcClient<C>
where
	C: ChainflipClient + Send + Sync,
{
	async fn submit_extrinsic(
		&self,
		extrinsic: state_chain_runtime::UncheckedExtrinsic,
	) -> RpcResult<sp_core::H256> {
		self.rpc_client.submit_extrinsic(Bytes::from(extrinsic.encode())).await
	}

	async fn get_block(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<Option<SignedBlock>> {
		self.rpc_client.block(Some(block_hash)).await
	}

	async fn latest_block_hash(&self) -> RpcResult<H256> {
		Ok(self
			.rpc_client
			.header(None)
			.await?
			.expect("Latest block hash could not be fetched")
			.hash())
	}

	async fn storage(
		&self,
		block_hash: state_chain_runtime::Hash,
		storage_key: StorageKey,
	) -> RpcResult<Option<StorageData>> {
		self.rpc_client.storage(storage_key, Some(block_hash)).await
	}

	async fn rotate_keys(&self) -> RpcResult<Bytes> {
		self.rpc_client.rotate_keys().await
	}

	async fn storage_pairs(
		&self,
		block_hash: state_chain_runtime::Hash,
		storage_key: StorageKey,
	) -> RpcResult<Vec<(StorageKey, StorageData)>> {
		self.rpc_client.storage_pairs(storage_key, Some(block_hash)).await
	}

	async fn fetch_runtime_version(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> RpcResult<RuntimeVersion> {
		self.rpc_client.runtime_version(Some(block_hash)).await
	}

	async fn is_auction_phase(&self) -> RpcResult<bool> {
		self.rpc_client.cf_is_auction_phase(None).await
	}
}

pub struct StateChainClient<RpcClient: StateChainRpcApi> {
	nonce: AtomicU32,
	/// Our Node's AccountId
	pub our_account_id: AccountId32,

	runtime_version: RwLock<sp_version::RuntimeVersion>,
	genesis_hash: state_chain_runtime::Hash,
	pub signer: signer::PairSigner<sp_core::sr25519::Pair>,

	pub state_chain_rpc_client: RpcClient,
}

impl<RpcClient: StateChainRpcApi> StateChainClient<RpcClient> {
	pub fn get_genesis_hash(&self) -> state_chain_runtime::Hash {
		self.genesis_hash
	}
}

#[cfg(test)]
pub const OUR_ACCOUNT_ID_BYTES: [u8; 32] = [0; 32];

#[cfg(test)]
pub const NOT_OUR_ACCOUNT_ID_BYTES: [u8; 32] = [1; 32];

#[cfg(test)]
impl<RpcClient: StateChainRpcApi> StateChainClient<RpcClient> {
	pub fn create_test_sc_client(rpc_client: RpcClient) -> Self {
		use signer::PairSigner;

		Self {
			nonce: AtomicU32::new(0),
			our_account_id: AccountId32::new(OUR_ACCOUNT_ID_BYTES),
			state_chain_rpc_client: rpc_client,
			runtime_version: RwLock::new(RuntimeVersion::default()),
			genesis_hash: Default::default(),
			signer: PairSigner::new(Pair::generate().0),
		}
	}
}

use crate::state_chain_observer::client::storage_traits::StorageMapAssociatedTypes;

fn invalid_err_obj(invalid_reason: InvalidTransaction) -> ErrorObjectOwned {
	ErrorObject::owned(1010, "Invalid Transaction", Some(<&'static str>::from(invalid_reason)))
}

impl<RpcClient: StateChainRpcApi> StateChainClient<RpcClient> {
	fn create_and_sign_extrinsic(
		&self,
		call: state_chain_runtime::Call,
		runtime_version: &RuntimeVersion,
		genesis_hash: state_chain_runtime::Hash,
		nonce: state_chain_runtime::Index,
	) -> state_chain_runtime::UncheckedExtrinsic {
		let extra: state_chain_runtime::SignedExtra = (
			frame_system::CheckNonZeroSender::new(),
			frame_system::CheckSpecVersion::new(),
			frame_system::CheckTxVersion::new(),
			frame_system::CheckGenesis::new(),
			frame_system::CheckEra::from(Era::Immortal),
			frame_system::CheckNonce::from(nonce),
			frame_system::CheckWeight::new(),
			// This is the tx fee tip. Normally this determines transaction priority. We currently
			// ignore this in the runtime but it needs to be set to some default value.
			state_chain_runtime::ChargeTransactionPayment::from(0),
		);
		let additional_signed = (
			(),
			runtime_version.spec_version,
			runtime_version.transaction_version,
			genesis_hash,
			genesis_hash,
			(),
			(),
			(),
		);

		let signed_payload = state_chain_runtime::SignedPayload::from_raw(
			call.clone(),
			extra.clone(),
			additional_signed,
		);
		let signature = signed_payload.using_encoded(|bytes| self.signer.sign(bytes));

		state_chain_runtime::UncheckedExtrinsic::new_signed(
			call,
			MultiAddress::Id(self.signer.account_id().clone()),
			signature,
			extra,
		)
	}

	/// Sign and submit an extrinsic, retrying up to [MAX_EXTRINSIC_RETRY_ATTEMPTS] times if it
	/// fails on an invalid nonce.
	pub async fn submit_signed_extrinsic<Call>(
		&self,
		call: Call,
		logger: &slog::Logger,
	) -> Result<H256>
	where
		Call: Into<state_chain_runtime::Call> + Clone + std::fmt::Debug,
	{
		for _ in 0..MAX_EXTRINSIC_RETRY_ATTEMPTS {
			// use the previous value but increment it for the next thread that loads/fetches it
			let nonce = self.nonce.fetch_add(1, Ordering::Relaxed);
			let runtime_version = { self.runtime_version.read().await.clone() };
			match self
				.state_chain_rpc_client
				.submit_extrinsic(self.create_and_sign_extrinsic(
					call.clone().into(),
					&runtime_version,
					self.genesis_hash,
					nonce,
				))
				.await
			{
				Ok(tx_hash) => {
					slog::info!(
						logger,
						"{:?} submitted successfully with tx_hash: {:#x}",
						&call,
						tx_hash
					);
					return Ok(tx_hash)
				},
				Err(rpc_err) => match rpc_err {
					// This occurs when a transaction with the same nonce is in the transaction pool
					// (and the priority is <= priority of that existing tx)
					RpcError::Call(CallError::Custom(ref obj)) if obj.code() == 1014 => {
						slog::warn!(
							logger,
							"Extrinsic submission failed with nonce: {}. Error: {:?}. Transaction with same nonce found in transaction pool.",
							nonce,
							rpc_err
						);
					},
					// This occurs when the nonce has already been *consumed* i.e a transaction with
					// that nonce is in a block
					RpcError::Call(CallError::Custom(ref obj))
						if obj == &invalid_err_obj(InvalidTransaction::Stale) =>
					{
						// Since we can submit, crash (lose in-memory nonce state), restart => fetch
						// nonce from finalised. If the tx we submitted is not yet finalised, we
						// will fetch a nonce that will be too low. Which would cause this warning
						// on startup at submission of first (possibly couple) of extrinsics.
						slog::warn!(
							logger,
							"Extrinsic submission failed with nonce: {}. Error: {:?}. Transaction stale.",
							nonce,
							rpc_err
						);
					},
					RpcError::Call(CallError::Custom(ref obj))
						if obj == &invalid_err_obj(InvalidTransaction::BadProof) =>
					{
						slog::warn!(
                            logger,
                            "Extrinsic submission failed with nonce: {}. Error: {:?}. Refetching the runtime version.",
                            nonce,
                            rpc_err
                        );

						// we want to reset the nonce, either for the next extrinsic, or for when
						// we retry this one, with the updated runtime_version
						self.nonce.fetch_sub(1, Ordering::Relaxed);

						let latest_block_hash =
							self.state_chain_rpc_client.latest_block_hash().await?;

						let runtime_version = self
							.state_chain_rpc_client
							.fetch_runtime_version(latest_block_hash)
							.await?;

						{
							let runtime_version_locked =
								{ self.runtime_version.read().await.clone() };

							if runtime_version_locked == runtime_version {
								slog::warn!(logger, "Fetched RuntimeVersion of {:?} is the same as the previous RuntimeVersion. This is not expected.", &runtime_version);
								// break, as the error is now very unlikely to be solved by fetching
								// again
								break
							}

							*(self.runtime_version.write().await) = runtime_version;
						}
						// don't `return`, therefore go back to the top of the loop and retry
						// sending the transaction
					},
					err => {
						slog::error!(
							logger,
							"Extrinsic failed with error: {}. Extrinsic: {:?}",
							err,
							&call,
						);
						self.nonce.fetch_sub(1, Ordering::Relaxed);
						return Err(err.into())
					},
				},
			}
		}
		slog::error!(logger, "Exceeded maximum number of retry attempts");
		Err(anyhow!("Exceeded maximum number of retry attempts",))
	}

	/// Submit an unsigned extrinsic.
	pub async fn submit_unsigned_extrinsic<Call>(
		&self,
		call: Call,
		logger: &slog::Logger,
	) -> Result<H256>
	where
		Call: Into<state_chain_runtime::Call> + 'static + std::fmt::Debug + Clone + Send,
	{
		let extrinsic = state_chain_runtime::UncheckedExtrinsic::new_unsigned(call.clone().into());
		let expected_hash = BlakeTwo256::hash_of(&extrinsic);
		match self.state_chain_rpc_client.submit_extrinsic(extrinsic).await {
			Ok(tx_hash) => {
				slog::info!(
					logger,
					"Unsigned extrinsic {:?} submitted successfully with tx_hash: {:#x}",
					&call,
					tx_hash
				);
				assert_eq!(
					tx_hash, expected_hash,
					"tx_hash returned from RPC does not match expected hash"
				);
				Ok(tx_hash)
			},
			Err(rpc_err) => {
				match rpc_err {
					// POOL_ALREADY_IMPORTED error occurs when the transaction is already in the
					// pool More than one node can submit the same unsigned extrinsic. E.g. in the
					// case of a threshold signature success. Thus, if we get a "Transaction already
					// in pool" "error" we know that this particular extrinsic has already been
					// submitted. And so we can ignore the error and return the transaction hash
					RpcError::Call(CallError::Custom(ref obj)) if obj.code() == 1013 => {
						slog::trace!(
							logger,
							"Unsigned extrinsic {:?} with tx_hash {:#x} already in pool.",
							&call,
							expected_hash
						);
						Ok(expected_hash)
					},
					_ => {
						slog::error!(
							logger,
							"Unsigned extrinsic failed with error: {}. Extrinsic: {:?}",
							rpc_err,
							&call
						);
						Err(rpc_err.into())
					},
				}
			},
		}
	}

	/// Watches *only* submitted extrinsics. I.e. Cannot watch for chain called extrinsics.
	pub async fn watch_submitted_extrinsic<BlockStream>(
		&self,
		extrinsic_hash: state_chain_runtime::Hash,
		block_stream: &mut BlockStream,
	) -> Result<Vec<state_chain_runtime::Event>>
	where
		BlockStream:
			Stream<Item = anyhow::Result<state_chain_runtime::Header>> + Unpin + Send + 'static,
	{
		while let Some(result_header) = block_stream.next().await {
			let header = result_header?;
			let block_hash = header.hash();
			if let Some(signed_block) = self.state_chain_rpc_client.get_block(block_hash).await? {
				match signed_block.block.extrinsics.iter().position(|ext| {
					let hash = BlakeTwo256::hash_of(ext);
					hash == extrinsic_hash
				}) {
					Some(extrinsic_index_found) => {
						let events_for_block = self
							.get_storage_value::<frame_system::Events<state_chain_runtime::Runtime>>(
								block_hash,
							)
							.await?;
						return Ok(events_for_block
							.into_iter()
							.filter_map(|event_record| {
								if let Phase::ApplyExtrinsic(i) = event_record.phase {
									if i as usize != extrinsic_index_found {
										None
									} else {
										Some(event_record.event)
									}
								} else {
									None
								}
							})
							.collect::<Vec<_>>())
					},
					None => continue,
				}
			};
		}
		Err(anyhow!("Block stream loop exited, no event found",))
	}

	async fn get_storage_item<
		Value: FullCodec,
		OnEmpty,
		QueryKind: QueryKindTrait<Value, OnEmpty>,
	>(
		&self,
		storage_key: StorageKey,
		block_hash: H256,
		log_str: &str,
	) -> Result<<QueryKind as QueryKindTrait<Value, OnEmpty>>::Query> {
		Ok(QueryKind::from_optional_value_to_query(
			self.state_chain_rpc_client
				.storage(block_hash, storage_key.clone())
				.await
				.context(format!(
					"Failed to get storage {} with key: {:?} at block hash {:#x}",
					log_str, storage_key, block_hash
				))?
				.map(|data| context!(Value::decode(&mut &data.0[..])).unwrap()),
		))
	}

    pub async fn get_storage_value<StorageValue: storage_traits::StorageValueAssociatedTypes>(
        &self,
        block_hash: state_chain_runtime::Hash,
	) -> Result<<StorageValue::QueryKind as QueryKindTrait<StorageValue::Value, StorageValue::OnEmpty>>::Query>{
		self.get_storage_item::<StorageValue::Value, StorageValue::OnEmpty, StorageValue::QueryKind>(StorageValue::_hashed_key(), block_hash, "value").await
	}

	pub async fn get_storage_map<StorageMap: storage_traits::StorageMapAssociatedTypes>(
		&self,
		block_hash: state_chain_runtime::Hash,
		key: &StorageMap::Key,
	) -> Result<
		<StorageMap::QueryKind as QueryKindTrait<StorageMap::Value, StorageMap::OnEmpty>>::Query,
	> {
		self.get_storage_item::<StorageMap::Value, StorageMap::OnEmpty, StorageMap::QueryKind>(
			StorageMap::_hashed_key_for(key),
			block_hash,
			"map",
		)
		.await
	}

	pub async fn get_storage_double_map<
		StorageDoubleMap: storage_traits::StorageDoubleMapAssociatedTypes,
	>(
		&self,
		block_hash: state_chain_runtime::Hash,
		key1: &StorageDoubleMap::Key1,
		key2: &StorageDoubleMap::Key2,
	) -> Result<
		<StorageDoubleMap::QueryKind as QueryKindTrait<
			StorageDoubleMap::Value,
			StorageDoubleMap::OnEmpty,
		>>::Query,
	> {
		self.get_storage_item::<StorageDoubleMap::Value, StorageDoubleMap::OnEmpty, StorageDoubleMap::QueryKind>(StorageDoubleMap::_hashed_key_for(key1, key2), block_hash, "double map").await
	}

	/// Gets all the storage pairs (key, value) of a StorageMap.
	/// NB: Because this is an unbounded operation, it requires the node to have
	/// the `--rpc-methods=unsafe` enabled.
	pub async fn get_all_storage_pairs<StorageMap: storage_traits::StorageMapAssociatedTypes>(
		&self,
		block_hash: state_chain_runtime::Hash,
	) -> Result<Vec<(<StorageMap as StorageMapAssociatedTypes>::Key, StorageMap::Value)>> {
		Ok(self
			.state_chain_rpc_client
			.storage_pairs(block_hash, StorageMap::_prefix_hash())
			.await?
			.into_iter()
			.map(|(storage_key, storage_data)| {
				(
					StorageMap::key_from_storage_key(&storage_key),
					context!(StorageMap::Value::decode(&mut &storage_data.0[..])).unwrap(),
				)
			})
			.collect())
	}

	pub async fn rotate_session_keys(&self) -> Result<Bytes> {
		let session_key_bytes: Bytes = self.state_chain_rpc_client.rotate_keys().await?;
		Ok(session_key_bytes)
	}

	pub async fn is_auction_phase(&self) -> Result<bool> {
		self.state_chain_rpc_client.is_auction_phase().await.map_err(Into::into)
	}
}

fn try_unwrap_value<T, E>(lorv: sp_rpc::list::ListOrValue<Option<T>>, error: E) -> Result<T, E> {
	match lorv {
		sp_rpc::list::ListOrValue::Value(Some(value)) => Ok(value),
		_ => Err(error),
	}
}

pub async fn connect_to_state_chain(
	state_chain_settings: &settings::StateChain,
	wait_for_staking: bool,
	logger: &slog::Logger,
) -> Result<(
	H256,
	impl Stream<Item = Result<state_chain_runtime::Header>>,
	Arc<StateChainClient<StateChainRpcClient<impl ChainflipClient>>>,
)> {
	inner_connect_to_state_chain(state_chain_settings, wait_for_staking, logger)
		.await
		.context("Failed to connect to state chain node")
}

async fn inner_connect_to_state_chain(
	state_chain_settings: &settings::StateChain,
	wait_for_staking: bool,
	logger: &slog::Logger,
) -> Result<(
	H256,
	impl Stream<Item = Result<state_chain_runtime::Header>>,
	Arc<StateChainClient<StateChainRpcClient<impl ChainflipClient>>>,
)> {
	let logger = logger.new(o!(COMPONENT_KEY => "StateChainConnector"));
	let signer = signer::PairSigner::<sp_core::sr25519::Pair>::new(
		sp_core::sr25519::Pair::from_seed(&read_clean_and_decode_hex_str_file(
			&state_chain_settings.signing_key_file,
			"State Chain Signing Key",
			|str| {
				<[u8; 32]>::try_from(hex::decode(str).map_err(anyhow::Error::new)?)
					.map_err(|_err| anyhow!("Wrong length"))
			},
		)?),
	);

	let our_account_id = signer.account_id().to_owned();

	let account_storage_key = StorageKey(
		frame_system::Account::<state_chain_runtime::Runtime>::hashed_key_for(&our_account_id),
	);

	let state_chain_rpc_client =
		connect_to_state_chain_without_signer(state_chain_settings).await?;

	let (first_finalized_block_header, mut finalized_block_header_stream) = {
		// https://substrate.stackexchange.com/questions/3667/api-rpc-chain-subscribefinalizedheads-missing-blocks
		// https://arxiv.org/abs/2007.01560
		let mut sparse_finalized_block_header_stream = state_chain_rpc_client
			.rpc_client
			.subscribe_finalized_heads()
			.await?
			.map_err(Into::into)
			.chain(futures::stream::once(std::future::ready(Err(anyhow::anyhow!(
				"sparse_finalized_block_header_stream unexpectedly ended"
			)))));

		let mut latest_finalized_header: state_chain_runtime::Header =
			sparse_finalized_block_header_stream.next().await.unwrap()?;
		let rpc_client = state_chain_rpc_client.rpc_client.clone();

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

						let rpc_client = rpc_client.clone();
						async move {
							let rpc_client = &rpc_client;
							let intervening_headers: Vec<_> = futures::stream::iter(
								prev_finalized_header.number + 1..next_finalized_header.number,
							)
							.then(|block_number| async move {
								let block_hash = try_unwrap_value(
									rpc_client
										.block_hash(Some(sp_rpc::list::ListOrValue::Value(
											block_number.into(),
										)))
										.await?,
									anyhow!("Finalized block missing hash"),
								)
								.unwrap();
								let block_header =
									rpc_client.header(Some(block_hash)).await?.unwrap();
								assert_eq!(block_header.hash(), block_hash);
								assert_eq!(block_header.number, block_number);
								Result::<_, anyhow::Error>::Ok((block_hash, block_header))
							})
							.try_collect()
							.await?;

							for (block_hash, next_block_header) in Iterator::zip(
								std::iter::once(&prev_finalized_header.hash())
									.chain(intervening_headers.iter().map(|(hash, _header)| hash)),
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

	// Often `finalized_header` returns a significantly newer latest block than the stream returns
	// so we move the stream forward to this block
	let (mut latest_block_hash, mut latest_block_number) = {
		let finalised_header_hash = state_chain_rpc_client.rpc_client.finalized_head().await?;
		let finalised_header = state_chain_rpc_client.rpc_client
            .header(Some(finalised_header_hash))
            .await?
            .expect("We have the hash from the chain, so there should definitely be a header for this block");

		if first_finalized_block_header.number < finalised_header.number {
			for block_number in first_finalized_block_header.number + 1..=finalised_header.number {
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
		async fn get_account_nonce<C: ChainflipClient + Send + Sync>(
			state_rpc_client: &C,
			account_storage_key: &StorageKey,
			block_hash: state_chain_runtime::Hash,
		) -> Result<Option<u32>> {
			Ok(
				if let Some(encoded_account_info) =
					state_rpc_client.storage(account_storage_key.clone(), Some(block_hash)).await?
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

		let rpc_client = state_chain_rpc_client.rpc_client.as_ref();

		let account_nonce = match get_account_nonce(
			rpc_client,
			&account_storage_key,
			latest_block_hash,
		)
		.await?
		{
			Some(nonce) => nonce,
			None =>
				if wait_for_staking {
					loop {
						if let Some(nonce) =
							get_account_nonce(rpc_client, &account_storage_key, latest_block_hash)
								.await?
						{
							break nonce
						} else {
							slog::warn!(logger, "Your Chainflip account {} is not staked. WAITING for account to be staked at block: {}", our_account_id, latest_block_number);
							let block_header =
								finalized_block_header_stream.next().await.unwrap()?;
							latest_block_hash = block_header.hash();
							latest_block_number += 1;
							assert_eq!(latest_block_number, block_header.number);
						}
					}
				} else {
					bail!("Your Chainflip account {} is not staked", our_account_id);
				},
		};

		(latest_block_hash, latest_block_number, account_nonce)
	};

	slog::info!(
		logger,
		"Initalising State Chain state at block `{}`; block hash: `{:#x}`",
		latest_block_number,
		latest_block_hash
	);

	Ok((
		latest_block_hash,
		finalized_block_header_stream,
		Arc::new(StateChainClient {
			nonce: AtomicU32::new(account_nonce),
			runtime_version: RwLock::new(
				state_chain_rpc_client.fetch_runtime_version(latest_block_hash).await?,
			),
			genesis_hash: try_unwrap_value(
				state_chain_rpc_client
					.rpc_client
					.block_hash(Some(sp_rpc::number::NumberOrHex::from(0u64).into()))
					.await?,
				anyhow!("Genesis block doesn't exist?"),
			)?,
			signer: signer.clone(),
			state_chain_rpc_client,
			our_account_id,
		}),
	))
}

pub async fn connect_to_state_chain_without_signer(
	state_chain_settings: &settings::StateChain,
) -> Result<StateChainRpcClient<impl ChainflipClient>> {
	let ws_endpoint = state_chain_settings.ws_endpoint.as_str();
	let rpc_client = Arc::new(
		WsClientBuilder::default()
			.build(&url::Url::parse(ws_endpoint)?)
			.await
			.with_context(|| {
				format!("Failed to establish rpc connection to substrate node '{}'", ws_endpoint)
			})?,
	);

	Ok(StateChainRpcClient { rpc_client })
}

#[cfg(test)]
mod tests {

	use sp_runtime::create_runtime_str;
	use sp_version::RuntimeVersion;

	use crate::{
		logging::{self, test_utils::new_test_logger},
		settings::{CfSettings, CommandLineOptions, Settings},
	};

	use utilities::assert_ok;

	use super::*;

	#[ignore = "depends on running state chain, and a configured Local.toml file"]
	#[tokio::main]
	#[test]
	async fn test_finalised_storage_subs() {
		let settings = <Settings as CfSettings>::load_settings_from_all_sources(
			"config/Local.toml",
			None,
			CommandLineOptions::default(),
		)
		.unwrap();
		let logger = logging::test_utils::new_test_logger();
		let (_, mut block_stream, state_chain_client) =
			connect_to_state_chain(&settings.state_chain, false, &logger)
				.await
				.expect("Could not connect");

		println!("My account id is: {}", state_chain_client.our_account_id);

		while let Some(block) = block_stream.next().await {
			let block_header = block.unwrap();
			let block_hash = block_header.hash();
			let block_number = block_header.number;
			println!(
				"Getting events from block {} with block_hash: {:?}",
				block_number, block_hash
			);
			let my_state_for_this_block = state_chain_client
				.get_all_storage_pairs::<pallet_cf_validator::AccountPeerMapping<state_chain_runtime::Runtime>>(
					block_hash,
				)
				.await
				.unwrap();

			println!("Returning AccountPeerMapping for this block: {:?}", my_state_for_this_block);
		}
	}

	#[tokio::test]
	async fn nonce_increments_on_success() {
		let logger = new_test_logger();
		let bytes: [u8; 32] =
			hex::decode("276dabe5c09f607729280c91c3de2dc588cd0e6ccba24db90cae050d650b3fc3")
				.unwrap()
				.try_into()
				.unwrap();
		let tx_hash = H256::from(bytes);

		let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
		mock_state_chain_rpc_client
			.expect_submit_extrinsic()
			.times(1)
			.returning(move |_| Ok(tx_hash));

		let state_chain_client =
			StateChainClient::create_test_sc_client(mock_state_chain_rpc_client);

		let force_rotation_call: state_chain_runtime::Call =
			pallet_cf_governance::Call::propose_governance_extrinsic {
				call: Box::new(pallet_cf_validator::Call::force_rotation {}.into()),
			}
			.into();

		assert_ok!(state_chain_client.submit_signed_extrinsic(force_rotation_call, &logger).await);

		assert_eq!(state_chain_client.nonce.load(Ordering::Relaxed), 1);
	}

	#[tokio::test]
	async fn tx_retried_and_nonce_incremented_on_fail_due_to_nonce_in_tx_pool_each_time() {
		let logger = new_test_logger();

		let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
		mock_state_chain_rpc_client
			.expect_submit_extrinsic()
			.times(MAX_EXTRINSIC_RETRY_ATTEMPTS)
			.returning(move |_| {
				Err(CallError::Custom(ErrorObject::owned::<()>(1014, "Priority too low", None))
					.into())
			});

		let state_chain_client =
			StateChainClient::create_test_sc_client(mock_state_chain_rpc_client);

		let force_rotation_call: state_chain_runtime::Call =
			pallet_cf_governance::Call::propose_governance_extrinsic {
				call: Box::new(pallet_cf_validator::Call::force_rotation {}.into()),
			}
			.into();

		state_chain_client
			.submit_signed_extrinsic(force_rotation_call, &logger)
			.await
			.unwrap_err();

		assert_eq!(state_chain_client.nonce.load(Ordering::Relaxed), 10);
	}

	#[tokio::test]
	async fn tx_retried_and_nonce_incremented_on_fail_due_to_nonce_consumed_in_prev_blocks_each_time(
	) {
		let logger = new_test_logger();

		let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
		mock_state_chain_rpc_client
			.expect_submit_extrinsic()
			.times(MAX_EXTRINSIC_RETRY_ATTEMPTS)
			.returning(move |_| {
				Err(CallError::Custom(ErrorObject::owned(
					1010,
					"Invalid Transaction",
					Some(<&'static str>::from(InvalidTransaction::Stale)),
				))
				.into())
			});

		let state_chain_client =
			StateChainClient::create_test_sc_client(mock_state_chain_rpc_client);

		let force_rotation_call: state_chain_runtime::Call =
			pallet_cf_governance::Call::propose_governance_extrinsic {
				call: Box::new(pallet_cf_validator::Call::force_rotation {}.into()),
			}
			.into();

		state_chain_client
			.submit_signed_extrinsic(force_rotation_call, &logger)
			.await
			.unwrap_err();

		assert_eq!(state_chain_client.nonce.load(Ordering::Relaxed), 10);
	}

	#[tokio::test]
	async fn tx_retried_and_nonce_not_incremented_but_version_updated_when_invalid_tx_bad_proof() {
		let logger = new_test_logger();

		let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
		mock_state_chain_rpc_client.expect_submit_extrinsic().times(1).returning(
			move |_ext: state_chain_runtime::UncheckedExtrinsic| {
				Err(CallError::Custom(ErrorObject::owned(
					1010,
					"Invalid Transaction",
					Some(<&'static str>::from(InvalidTransaction::BadProof)),
				))
				.into())
			},
		);

		// Second time called, should succeed
		mock_state_chain_rpc_client
			.expect_submit_extrinsic()
			.times(1)
			.returning(move |_| Ok(H256::default()));

		mock_state_chain_rpc_client
			.expect_latest_block_hash()
			.times(1)
			.returning(|| Ok(H256::default()));

		mock_state_chain_rpc_client
			.expect_fetch_runtime_version()
			.times(1)
			.returning(move |_| {
				Ok(RuntimeVersion {
					spec_name: create_runtime_str!("fake-chainflip-node"),
					impl_name: create_runtime_str!("fake-chainflip-node"),
					authoring_version: 1,
					spec_version: 104,
					impl_version: 1,
					apis: Default::default(),
					transaction_version: 1,
					state_version: 1,
				})
			});

		let state_chain_client =
			StateChainClient::create_test_sc_client(mock_state_chain_rpc_client);

		let force_rotation_call: state_chain_runtime::Call =
			pallet_cf_governance::Call::propose_governance_extrinsic {
				call: Box::new(pallet_cf_validator::Call::force_rotation {}.into()),
			}
			.into();

		assert_ok!(state_chain_client.submit_signed_extrinsic(force_rotation_call, &logger).await);

		// we should only have incremented the nonce once, on the success
		assert_eq!(state_chain_client.nonce.load(Ordering::Relaxed), 1);

		// we should have updated the runtime version
		assert_eq!(state_chain_client.runtime_version.read().await.spec_version, 104);
	}

	#[tokio::test]
	async fn tx_fails_for_reason_unrelated_to_nonce_does_not_retry_does_not_increment_nonce() {
		let logger = new_test_logger();

		// Return a non-nonce related error, we submit two extrinsics that fail in the same way
		let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
		mock_state_chain_rpc_client
			.expect_submit_extrinsic()
			.times(1)
			.returning(move |_| Err(RpcError::RequestTimeout));

		let state_chain_client =
			StateChainClient::create_test_sc_client(mock_state_chain_rpc_client);

		let force_rotation_call: state_chain_runtime::Call =
			pallet_cf_governance::Call::propose_governance_extrinsic {
				call: Box::new(pallet_cf_validator::Call::force_rotation {}.into()),
			}
			.into();

		state_chain_client
			.submit_signed_extrinsic(force_rotation_call.clone(), &logger)
			.await
			.unwrap_err();

		assert_eq!(state_chain_client.nonce.load(Ordering::Relaxed), 0);
	}

	// 1. We submit a tx
	// 2. Tx fails with a nonce error, so we leave the nonce incremented
	// 3. We call again (incrementing the nonce for next time) nonce for this call is 1.
	// 4. We succeed, therefore the nonce for the next call is 2.
	#[tokio::test]
	async fn tx_fails_due_to_nonce_increments_nonce_then_exits_when_successful() {
		let logger = new_test_logger();

		let bytes: [u8; 32] =
			hex::decode("276dabe5c09f607729280c91c3de2dc588cd0e6ccba24db90cae050d650b3fc3")
				.unwrap()
				.try_into()
				.unwrap();
		let tx_hash = H256::from(bytes);

		// Return a non-nonce related error, we submit two extrinsics that fail in the same way
		let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
		mock_state_chain_rpc_client
			.expect_submit_extrinsic()
			.times(1)
			.returning(move |_| {
				Err(CallError::Custom(ErrorObject::owned::<()>(1014, "Priority too low", None))
					.into())
			});

		mock_state_chain_rpc_client
			.expect_submit_extrinsic()
			.times(1)
			.returning(move |_| Ok(tx_hash));

		let state_chain_client =
			StateChainClient::create_test_sc_client(mock_state_chain_rpc_client);

		let force_rotation_call: state_chain_runtime::Call =
			pallet_cf_governance::Call::propose_governance_extrinsic {
				call: Box::new(pallet_cf_validator::Call::force_rotation {}.into()),
			}
			.into();

		assert_ok!(
			state_chain_client
				.submit_signed_extrinsic(force_rotation_call.clone(), &logger)
				.await
		);

		assert_eq!(state_chain_client.nonce.load(Ordering::Relaxed), 2);
	}
}
