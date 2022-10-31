pub mod base_rpc_api;
pub mod extrinsic_api;
mod signer;
pub mod storage_api;

use base_rpc_api::BaseRpcApi;

use anyhow::{anyhow, bail, Context, Result};
use codec::Decode;
use futures::{Stream, StreamExt, TryStreamExt};

use slog::o;
use sp_core::{storage::StorageKey, Pair, H256};
use std::sync::{atomic::AtomicU32, Arc};
use tokio::sync::RwLock;

use crate::{
	common::{read_clean_and_decode_hex_str_file, EngineTryStreamExt},
	logging::COMPONENT_KEY,
	settings,
};
use utilities::context;

pub struct StateChainClient {
	nonce: AtomicU32,
	runtime_version: RwLock<sp_version::RuntimeVersion>,
	genesis_hash: state_chain_runtime::Hash,
	signer: signer::PairSigner<sp_core::sr25519::Pair>,
	pub base_rpc_client: Arc<base_rpc_api::BaseRpcClient<jsonrpsee::ws_client::WsClient>>,
}

impl StateChainClient {
	pub fn get_genesis_hash(&self) -> state_chain_runtime::Hash {
		self.genesis_hash
	}
}

pub async fn connect_to_state_chain(
	state_chain_settings: &settings::StateChain,
	wait_for_staking: bool,
	logger: &slog::Logger,
) -> Result<(H256, impl Stream<Item = Result<state_chain_runtime::Header>>, Arc<StateChainClient>)>
{
	inner_connect_to_state_chain(state_chain_settings, wait_for_staking, logger)
		.await
		.context("Failed to connect to state chain node")
}

async fn inner_connect_to_state_chain(
	state_chain_settings: &settings::StateChain,
	wait_for_staking: bool,
	logger: &slog::Logger,
) -> Result<(H256, impl Stream<Item = Result<state_chain_runtime::Header>>, Arc<StateChainClient>)>
{
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

	let account_storage_key = StorageKey(
		frame_system::Account::<state_chain_runtime::Runtime>::hashed_key_for(&signer.account_id),
	);

	let base_rpc_client = Arc::new(base_rpc_api::BaseRpcClient::new(state_chain_settings).await?);

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
								let block_header = base_rpc_client.block_header(block_hash).await?;
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
		let finalised_header_hash = base_rpc_client.latest_finalized_block_hash().await?;
		let finalised_header = base_rpc_client.block_header(finalised_header_hash).await?;

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
		async fn get_account_nonce<StateChainRpcClient: base_rpc_api::BaseRpcApi + Send + Sync>(
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
				base_rpc_client.fetch_runtime_version(latest_block_hash).await?,
			),
			genesis_hash: base_rpc_client.block_hash(0).await?.unwrap(),
			signer: signer.clone(),
			base_rpc_client,
		}),
	))
}

#[cfg(test)]
pub mod mocks {
	use crate::state_chain_observer::client::{
		extrinsic_api::ExtrinsicApi, storage_api::SafeStorageApi,
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
					Stream<Item = anyhow::Result<state_chain_runtime::Header>> + Unpin + Send + 'static;
		}
		#[async_trait]
		impl SafeStorageApi for StateChainClient {
			async fn get_storage_item<
				Value: codec::FullCodec + 'static,
				OnEmpty: 'static,
				QueryKind: QueryKindTrait<Value, OnEmpty> + 'static,
			>(
				&self,
				storage_key: StorageKey,
				block_hash: state_chain_runtime::Hash,
			) -> RpcResult<<QueryKind as QueryKindTrait<Value, OnEmpty>>::Query>;

			async fn get_storage_value<StorageValue: StorageValueAssociatedTypes + 'static>(
				&self,
				block_hash: state_chain_runtime::Hash,
			) -> RpcResult<<StorageValue::QueryKind as QueryKindTrait<StorageValue::Value, StorageValue::OnEmpty>>::Query>;

			async fn get_storage_map_entry<StorageMap: StorageMapAssociatedTypes + 'static>(
				&self,
				block_hash: state_chain_runtime::Hash,
				key: &StorageMap::Key,
			) -> RpcResult<
				<StorageMap::QueryKind as QueryKindTrait<StorageMap::Value, StorageMap::OnEmpty>>::Query,
			>
			where
				StorageMap::Key: Sync;

			async fn get_storage_double_map_entry<StorageDoubleMap: StorageDoubleMapAssociatedTypes + 'static>(
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

			/// Gets all the storage pairs (key, value) of a StorageMap.
			/// NB: Because this is an unbounded operation, it requires the node to have
			/// the `--rpc-methods=unsafe` enabled.
			async fn get_storage_map<StorageMap: StorageMapAssociatedTypes + 'static>(
				&self,
				block_hash: state_chain_runtime::Hash,
			) -> RpcResult<Vec<(<StorageMap as StorageMapAssociatedTypes>::Key, StorageMap::Value)>>;
		}
	}
}

/*
#[cfg(test)]
pub const OUR_ACCOUNT_ID_BYTES: [u8; 32] = [0; 32];

#[cfg(test)]
pub const NOT_OUR_ACCOUNT_ID_BYTES: [u8; 32] = [1; 32];

#[cfg(test)]
impl<BaseRpcClient: StateChainRpcApi> StateChainClient<BaseRpcClient> {
	pub fn create_test_sc_client(base_rpc_client: BaseRpcClient) -> Self {
		use signer::PairSigner;

		Self {
			nonce: AtomicU32::new(0),
			our_account_id: AccountId32::new(OUR_ACCOUNT_ID_BYTES),
			base_rpc_client: base_rpc_client,
			runtime_version: RwLock::new(RuntimeVersion::default()),
			genesis_hash: Default::default(),
			signer: PairSigner::new(Pair::generate().0),
		}
	}
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
*/
