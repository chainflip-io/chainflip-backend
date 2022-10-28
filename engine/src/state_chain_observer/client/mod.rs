pub mod base_rpc_api;
mod signer;
pub mod storage_api;

use base_rpc_api::BaseRpcApi;

use anyhow::{anyhow, bail, Context, Result};
use codec::{Decode, Encode};
use frame_support::pallet_prelude::InvalidTransaction;
use frame_system::Phase;
use futures::{Stream, StreamExt, TryStreamExt};
use jsonrpsee::{
	core::Error as RpcError,
	types::{error::CallError, ErrorObject, ErrorObjectOwned},
};

use slog::o;
use sp_core::{storage::StorageKey, Pair, H256};
use sp_runtime::{
	generic::Era,
	traits::{BlakeTwo256, Hash},
	MultiAddress,
};
use sp_version::RuntimeVersion;
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

use self::storage_api::SafeStorageApi;

pub struct StateChainClient {
	nonce: AtomicU32,
	runtime_version: RwLock<sp_version::RuntimeVersion>,
	genesis_hash: state_chain_runtime::Hash,
	pub signer: signer::PairSigner<sp_core::sr25519::Pair>,
	pub base_rpc_client: Arc<base_rpc_api::BaseRpcClient<jsonrpsee::ws_client::WsClient>>,
}

impl StateChainClient {
	pub fn get_genesis_hash(&self) -> state_chain_runtime::Hash {
		self.genesis_hash
	}
}

fn invalid_err_obj(invalid_reason: InvalidTransaction) -> ErrorObjectOwned {
	ErrorObject::owned(1010, "Invalid Transaction", Some(<&'static str>::from(invalid_reason)))
}

impl StateChainClient {
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
			MultiAddress::Id(self.signer.account_id.clone()),
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
				.base_rpc_client
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
							self.base_rpc_client.latest_finalized_block_hash().await?;

						let runtime_version =
							self.base_rpc_client.fetch_runtime_version(latest_block_hash).await?;

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
		match self.base_rpc_client.submit_extrinsic(extrinsic).await {
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
			if let Some(signed_block) = self.base_rpc_client.block(block_hash).await? {
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
