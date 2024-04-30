use std::{collections::BTreeSet, sync::Arc};

use crate::{
	btc::retry_rpc::mocks::MockBtcRetryRpcClient,
	dot::retry_rpc::mocks::MockDotHttpRpcClient,
	evm::retry_rpc::mocks::MockEvmRetryRpcClient,
	state_chain_observer::{
		client::{
			extrinsic_api,
			stream_api::{StateChainStream, FINALIZED},
		},
		test_helpers::test_header,
	},
};
use cf_chains::{evm::Transaction, ChainCrypto};
use cf_primitives::{AccountRole, CeremonyId, GENESIS_EPOCH};
use futures::FutureExt;
use mockall::predicate::eq;
use multisig::{eth::EvmCryptoScheme, ChainSigning, SignatureToThresholdSignature};
use pallet_cf_cfe_interface::{
	CfeEvent, KeyHandoverRequest, KeygenRequest, ThresholdSignatureRequest, TxBroadcastRequest,
};
use sp_runtime::AccountId32;

use sp_core::H256;
use state_chain_runtime::{
	AccountId, BitcoinInstance, EvmInstance, PolkadotInstance, Runtime, RuntimeCall,
};
use utilities::cached_stream::MakeCachedStream;

use crate::{
	settings::Settings,
	state_chain_observer::{client::mocks::MockStateChainClient, sc_observer},
};
use multisig::{
	client::{KeygenFailureReason, MockMultisigClientApi, SigningFailureReason},
	eth::EthSigning,
	CryptoScheme, KeyId,
};
use utilities::task_scope::task_scope;

use super::{crypto_compat::CryptoCompat, get_ceremony_id_counters_before_block};

async fn start_sc_observer<
	BlockStream: crate::state_chain_observer::client::stream_api::StreamApi<FINALIZED>,
>(
	state_chain_client: MockStateChainClient,
	sc_block_stream: BlockStream,
	eth_rpc: MockEvmRetryRpcClient,
) {
	sc_observer::start(
		Arc::new(state_chain_client),
		sc_block_stream,
		eth_rpc,
		MockEvmRetryRpcClient::new(),
		MockDotHttpRpcClient::new(),
		MockBtcRetryRpcClient::new(),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
	)
	.await
	.unwrap_err();
}

// TODO: We should test that this works for historical epochs too. We should be able to sign for
// historical epochs we were a part of
#[tokio::test]
async fn only_encodes_and_signs_when_specified() {
	let account_id = AccountId::new([0; 32]);

	let mut state_chain_client = MockStateChainClient::new();

	state_chain_client.expect_account_id().once().return_once({
		let account_id = account_id.clone();
		|| account_id
	});

	let block = test_header(20, None);
	let sc_block_stream = tokio_stream::iter([]).make_cached(block);

	use state_chain_runtime::Runtime;

	state_chain_client
		.expect_storage_value::<pallet_cf_cfe_interface::CfeEvents<Runtime>>()
		.with(eq(block.hash))
		.once()
		.return_once(move |_| {
			Ok(vec![
				CfeEvent::<Runtime>::EthTxBroadcastRequest(TxBroadcastRequest::<Runtime, _> {
					broadcast_id: Default::default(),
					nominee: account_id,
					payload: Transaction::default(),
				}),
				CfeEvent::<Runtime>::EthTxBroadcastRequest(TxBroadcastRequest::<Runtime, _> {
					broadcast_id: Default::default(),
					nominee: AccountId32::new([1; 32]), // NOT OUR ACCOUNT ID
					payload: Transaction::default(),
				}),
			])
		});

	let mut eth_rpc_mock_broadcast = MockEvmRetryRpcClient::new();

	// This doesn't always get called since the test can finish without the scope that spawns the
	// broadcast task finishing.
	eth_rpc_mock_broadcast.expect_broadcast_transaction().return_once(|_| {
		// Return some hash
		Ok(H256::from([1; 32]))
	});

	let mut eth_mock_clone = MockEvmRetryRpcClient::new();
	eth_mock_clone.expect_clone().return_once(|| eth_rpc_mock_broadcast);

	start_sc_observer(state_chain_client, StateChainStream::new(sc_block_stream), eth_mock_clone)
		.await;
}

// TODO: Test that when we return None for polkadot vault
// witnessing isn't started for dot, but is started for ETH

/// Test all 3 cases of handling a signing request: not participating, failure, and success.
async fn should_handle_signing_request<C, I>()
where
	C: CryptoScheme + Send + Sync,
	I: 'static + Send + Sync,

	Runtime: pallet_cf_threshold_signature::Config<I>,
	RuntimeCall:
		std::convert::From<pallet_cf_threshold_signature::Call<Runtime, I>>,
	<<Runtime as pallet_cf_threshold_signature::Config<I>>::TargetChainCrypto as
ChainCrypto>::ThresholdSignature: std::convert::From<<C as CryptoScheme>::Signature>,
	Vec<C::Signature>: SignatureToThresholdSignature<
		<Runtime as pallet_cf_threshold_signature::Config<I>>::TargetChainCrypto

	>,
{
	let key_id = KeyId::new(1, [0u8; 32]);
	let payload = C::signing_payload_for_test();
	let our_account_id = AccountId32::new([0; 32]);
	let not_our_account_id = AccountId32::new([1u8; 32]);
	assert_ne!(our_account_id, not_our_account_id);

	let mut state_chain_client = MockStateChainClient::new();
	let mut multisig_client = MockMultisigClientApi::<C>::new();

	// All 3 signing requests will ask for the account id
	state_chain_client
		.expect_account_id()
		.times(3)
		.return_const(our_account_id.clone());

	// ceremony_id_1 is a non-participating ceremony and should update the latest ceremony id
	let ceremony_id_1 = 1;
	multisig_client
		.expect_update_latest_ceremony_id()
		.with(eq(ceremony_id_1))
		.once()
		.returning(|_| ());

	// ceremony_id_2 is a failure and should submit a signed extrinsic
	let ceremony_id_2 = ceremony_id_1 + 1;
	multisig_client
		.expect_initiate_signing()
		.with(
			eq(ceremony_id_2),
			eq(BTreeSet::from_iter([our_account_id.clone()])),
			eq(vec![(key_id.clone(), payload.clone())]),
		)
		.once()
		.return_once(|_, _, _| {
			futures::future::ready(Err((
				BTreeSet::new(),
				SigningFailureReason::InvalidParticipants,
			)))
			.boxed()
		});
	state_chain_client
		.expect_finalize_signed_extrinsic::<pallet_cf_threshold_signature::Call<Runtime, I>>()
		.with(eq(pallet_cf_threshold_signature::Call::<Runtime, I>::report_signature_failed {
			ceremony_id: ceremony_id_2,
			offenders: BTreeSet::default(),
		}))
		.once()
		.return_once(|_| {
			(
				extrinsic_api::signed::MockUntilInBlock::new(),
				extrinsic_api::signed::MockUntilFinalized::new(),
			)
		});

	// ceremony_id_3 is a success and should submit an unsigned extrinsic
	let ceremony_id_3 = ceremony_id_2 + 1;
	let signatures = vec![C::signature_for_test()];
	let signatures_clone = signatures.clone();
	multisig_client
		.expect_initiate_signing()
		.with(
			eq(ceremony_id_3),
			eq(BTreeSet::from_iter([our_account_id.clone()])),
			eq(vec![(key_id.clone(), payload.clone())]),
		)
		.once()
		.return_once(move |_, _, _| futures::future::ready(Ok(signatures_clone)).boxed());
	state_chain_client
		.expect_submit_unsigned_extrinsic()
		.with(eq(pallet_cf_threshold_signature::Call::<Runtime, I>::signature_success {
			ceremony_id: ceremony_id_3,
			signature: signatures.to_threshold_signature(),
		}))
		.once()
		.return_once(|_: pallet_cf_threshold_signature::Call<Runtime, I>| Ok(H256::default()));

	let state_chain_client = Arc::new(state_chain_client);
	task_scope(|scope| {
		async {
			// Handle a signing request that we are not participating in
			sc_observer::handle_signing_request::<_, _, C, I>(
				scope,
				&multisig_client,
				state_chain_client.clone(),
				ceremony_id_1,
				BTreeSet::from_iter([not_our_account_id.clone()]),
				vec![(key_id.clone(), payload.clone())],
			)
			.await;

			// Handle a signing request that we are participating in.
			// This one will return an error.
			sc_observer::handle_signing_request::<_, _, C, I>(
				scope,
				&multisig_client,
				state_chain_client.clone(),
				ceremony_id_2,
				BTreeSet::from_iter([our_account_id.clone()]),
				vec![(key_id.clone(), payload.clone())],
			)
			.await;

			// Handle another signing request that we are participating in.
			// This one will return success.
			sc_observer::handle_signing_request::<_, _, C, I>(
				scope,
				&multisig_client,
				state_chain_client.clone(),
				ceremony_id_3,
				BTreeSet::from_iter([our_account_id]),
				vec![(key_id, payload)],
			)
			.await;

			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap();
}

// Test that the ceremony requests are calling the correct MultisigClientApi functions
// depending on whether we are participating in the ceremony or not.
#[tokio::test]
async fn should_handle_signing_request_eth() {
	should_handle_signing_request::<EvmCryptoScheme, EvmInstance>().await;
}

mod dot_signing {

	use multisig::polkadot::PolkadotCryptoScheme;

	use super::*;
	use PolkadotInstance;

	#[tokio::test]
	async fn should_handle_signing_request_dot() {
		should_handle_signing_request::<PolkadotCryptoScheme, PolkadotInstance>().await;
	}
}

async fn should_handle_keygen_request<C, I>()
where
	C: ChainSigning<
			ChainCrypto = <Runtime as pallet_cf_threshold_signature::Config<I>>::TargetChainCrypto,
		> + Send
		+ Sync,
	I: CryptoCompat<C, C::ChainCrypto> + 'static + Send + Sync,
	Runtime: pallet_cf_threshold_signature::Config<I>,
	RuntimeCall: std::convert::From<pallet_cf_threshold_signature::Call<Runtime, I>>,
{
	let first_ceremony_id = 1;
	let our_account_id = AccountId32::new([0; 32]);
	let not_our_account_id = AccountId32::new([1u8; 32]);
	assert_ne!(our_account_id, not_our_account_id);

	let mut state_chain_client = MockStateChainClient::new();
	state_chain_client
		.expect_account_id()
		.times(2)
		.return_const(our_account_id.clone());
	state_chain_client
		.expect_finalize_signed_extrinsic::<pallet_cf_threshold_signature::Call<Runtime, I>>()
		.once()
		.return_once(|_| {
			(
				extrinsic_api::signed::MockUntilInBlock::new(),
				extrinsic_api::signed::MockUntilFinalized::new(),
			)
		});
	let state_chain_client = Arc::new(state_chain_client);

	let mut multisig_client = MockMultisigClientApi::<C::CryptoScheme>::new();
	multisig_client
		.expect_update_latest_ceremony_id()
		.with(eq(first_ceremony_id))
		.once()
		.return_once(|_| ());

	let next_ceremony_id = first_ceremony_id + 1;
	// Set up the mock api to expect the keygen and sign calls for the ceremonies we are
	// participating in. It doesn't matter what failure reasons they return.
	multisig_client
		.expect_initiate_keygen()
		.with(
			eq(next_ceremony_id),
			eq(GENESIS_EPOCH),
			eq(BTreeSet::from_iter([our_account_id.clone()])),
		)
		.once()
		.return_once(|_, _, _| {
			futures::future::ready(Err((BTreeSet::new(), KeygenFailureReason::InvalidParticipants)))
				.boxed()
		});

	task_scope(|scope| {
		async {
			// Handle a keygen request that we are not participating in
			sc_observer::handle_keygen_request::<_, _, _, I>(
				scope,
				&multisig_client,
				state_chain_client.clone(),
				first_ceremony_id,
				GENESIS_EPOCH,
				BTreeSet::from_iter([not_our_account_id.clone()]),
			)
			.await;

			// Handle a keygen request that we are participating in
			sc_observer::handle_keygen_request::<_, _, _, I>(
				scope,
				&multisig_client,
				state_chain_client.clone(),
				next_ceremony_id,
				GENESIS_EPOCH,
				BTreeSet::from_iter([our_account_id]),
			)
			.await;
			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap();
}

#[tokio::test]
async fn should_handle_keygen_request_eth() {
	should_handle_keygen_request::<EthSigning, EvmInstance>().await;
}

mod dot_keygen {
	use multisig::polkadot::PolkadotSigning;

	use super::*;
	use PolkadotInstance;
	#[tokio::test]
	async fn should_handle_keygen_request_dot() {
		should_handle_keygen_request::<PolkadotSigning, PolkadotInstance>().await;
	}
}

#[tokio::test]
async fn should_handle_key_handover_request()
where
	Runtime: pallet_cf_threshold_signature::Config<BitcoinInstance>,
	RuntimeCall: std::convert::From<pallet_cf_threshold_signature::Call<Runtime, BitcoinInstance>>,
{
	use multisig::bitcoin::BtcCryptoScheme;

	let first_ceremony_id = 1;
	let our_account_id = AccountId32::new([0; 32]);
	let not_our_account_id = AccountId32::new([1u8; 32]);
	assert_ne!(our_account_id, not_our_account_id);

	let mut state_chain_client = MockStateChainClient::new();
	let mut multisig_client = MockMultisigClientApi::<BtcCryptoScheme>::new();

	// Both requests will ask for the account id
	state_chain_client
		.expect_account_id()
		.times(2)
		.return_const(our_account_id.clone());

	// The first ceremony is a non-participating ceremony so it should update the latest ceremony id
	multisig_client
		.expect_update_latest_ceremony_id()
		.with(eq(first_ceremony_id))
		.once()
		.return_once(|_| ());

	// The second ceremony is a failure and should submit a signed extrinsic
	let next_ceremony_id = first_ceremony_id + 1;
	let key_to_share = cf_chains::btc::AggKey::default();
	multisig_client
		.expect_initiate_key_handover()
		.with(
			eq(next_ceremony_id),
			eq(KeyId::new(GENESIS_EPOCH, key_to_share.current)),
			eq(GENESIS_EPOCH + 1),
			eq(BTreeSet::from_iter([our_account_id.clone()])),
			eq(BTreeSet::from_iter([our_account_id.clone()])),
		)
		.once()
		.return_once(|_, _, _, _, _| {
			futures::future::ready(Err((BTreeSet::new(), KeygenFailureReason::InvalidParticipants)))
				.boxed()
		});
	state_chain_client
		.expect_finalize_signed_extrinsic::<pallet_cf_threshold_signature::Call<Runtime, BitcoinInstance>>(
		)
		.once()
		.return_once(|_| {
			(
				extrinsic_api::signed::MockUntilInBlock::new(),
				extrinsic_api::signed::MockUntilFinalized::new(),
			)
		});

	let state_chain_client = Arc::new(state_chain_client);
	task_scope(|scope| {
		async {
			// Handle the key handover request that we are not participating in
			sc_observer::handle_key_handover_request::<_, _>(
				scope,
				&multisig_client,
				state_chain_client.clone(),
				first_ceremony_id,
				GENESIS_EPOCH,
				GENESIS_EPOCH + 1,
				BTreeSet::from_iter([not_our_account_id.clone()]),
				BTreeSet::from_iter([not_our_account_id.clone()]),
				key_to_share,
				Default::default(),
			)
			.await;

			// Handle the key handover request that we are participating in
			sc_observer::handle_key_handover_request::<_, _>(
				scope,
				&multisig_client,
				state_chain_client.clone(),
				next_ceremony_id,
				GENESIS_EPOCH,
				GENESIS_EPOCH + 1,
				BTreeSet::from_iter([our_account_id.clone()]),
				BTreeSet::from_iter([our_account_id.clone()]),
				key_to_share,
				Default::default(),
			)
			.await;
			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap();
}

#[tokio::test]
async fn should_process_initial_block_first() {
	let mut state_chain_client = MockStateChainClient::new();

	state_chain_client.expect_account_id().once().return_once({
		let account_id = AccountId::new([0; 32]);
		|| account_id
	});

	let initial_block = test_header(20, None);
	let next_block = test_header(21, Some(initial_block.hash));

	// Create a stream with only `next_block` in it. The initial block is cached.
	let sc_block_stream = tokio_stream::iter([next_block]).make_cached(initial_block);

	let mut seq = mockall::Sequence::new();

	// We expect the SCO to process the initial block first, even though it was not in the stream.
	state_chain_client
		.expect_storage_value::<pallet_cf_cfe_interface::CfeEvents<Runtime>>()
		.with(eq(initial_block.hash))
		.once()
		.in_sequence(&mut seq)
		.return_once(move |_| Ok(vec![]));

	// Then it should process the block that was in the stream
	state_chain_client
		.expect_storage_value::<pallet_cf_cfe_interface::CfeEvents<Runtime>>()
		.with(eq(next_block.hash))
		.once()
		.in_sequence(&mut seq)
		.return_once(move |_| Ok(vec![]));

	let mut eth_rpc_mock_broadcast = MockEvmRetryRpcClient::new();

	// This doesn't always get called since the test can finish without the scope that spawns the
	// broadcast task finishing.
	eth_rpc_mock_broadcast.expect_broadcast_transaction().return_once(|_| {
		// Return some hash
		Ok(H256::from([1; 32]))
	});

	let mut eth_mock_clone = MockEvmRetryRpcClient::new();
	eth_mock_clone.expect_clone().return_once(|| eth_rpc_mock_broadcast);

	start_sc_observer(
		state_chain_client,
		StateChainStream::<true, _>::new(sc_block_stream),
		eth_mock_clone,
	)
	.await;
}

#[tokio::test]
async fn test_get_ceremony_id_counters_with_events() {
	const ETH_CEREMONY_ID_COUNTER_BEFORE_INITIAL_BLOCK: CeremonyId = 10;
	const DOT_CEREMONY_ID_COUNTER_BEFORE_INITIAL_BLOCK: CeremonyId = 20;
	const BTC_CEREMONY_ID_COUNTER_BEFORE_INITIAL_BLOCK: CeremonyId = 30;
	let block_hash = H256::default();

	let test_block_streams = vec![
		// Test 1: 1 signing request for each chain and another event that should not effect the id
		// counters
		vec![
			CfeEvent::<Runtime>::EthTxBroadcastRequest(TxBroadcastRequest::<Runtime, _> {
				broadcast_id: Default::default(),
				nominee: AccountId32::new([1; 32]),
				payload: Default::default(),
			}),
			CfeEvent::<Runtime>::EvmThresholdSignatureRequest(ThresholdSignatureRequest::<
				Runtime,
				_,
			> {
				ceremony_id: ETH_CEREMONY_ID_COUNTER_BEFORE_INITIAL_BLOCK + 1,
				epoch_index: 1,
				key: Default::default(),
				signatories: Default::default(),
				payload: Default::default(),
			}),
			CfeEvent::<Runtime>::DotThresholdSignatureRequest(ThresholdSignatureRequest::<
				Runtime,
				_,
			> {
				ceremony_id: DOT_CEREMONY_ID_COUNTER_BEFORE_INITIAL_BLOCK + 1,
				epoch_index: 1,
				key: Default::default(),
				signatories: Default::default(),
				payload: cf_chains::dot::EncodedPolkadotPayload(vec![]),
			}),
			CfeEvent::<Runtime>::BtcThresholdSignatureRequest(ThresholdSignatureRequest::<
				Runtime,
				_,
			> {
				ceremony_id: BTC_CEREMONY_ID_COUNTER_BEFORE_INITIAL_BLOCK + 1,
				epoch_index: 1,
				key: Default::default(),
				signatories: Default::default(),
				payload: Default::default(),
			}),
		],
		// Test 2: 1 keygen request for each chain
		vec![
			CfeEvent::<Runtime>::EvmKeygenRequest(KeygenRequest::<Runtime> {
				ceremony_id: ETH_CEREMONY_ID_COUNTER_BEFORE_INITIAL_BLOCK + 1,
				epoch_index: 1,
				participants: Default::default(),
			}),
			CfeEvent::<Runtime>::DotKeygenRequest(KeygenRequest::<Runtime> {
				ceremony_id: DOT_CEREMONY_ID_COUNTER_BEFORE_INITIAL_BLOCK + 1,
				epoch_index: 1,
				participants: Default::default(),
			}),
			CfeEvent::<Runtime>::BtcKeygenRequest(KeygenRequest::<Runtime> {
				ceremony_id: BTC_CEREMONY_ID_COUNTER_BEFORE_INITIAL_BLOCK + 1,
				epoch_index: 1,
				participants: Default::default(),
			}),
		],
		// Test 3: 1 key handover request for BTC (and keygen requests for the other chains to
		// avoid test complexity)
		vec![
			CfeEvent::<Runtime>::EvmKeygenRequest(KeygenRequest::<Runtime> {
				ceremony_id: ETH_CEREMONY_ID_COUNTER_BEFORE_INITIAL_BLOCK + 1,
				epoch_index: 1,
				participants: Default::default(),
			}),
			CfeEvent::<Runtime>::DotKeygenRequest(KeygenRequest::<Runtime> {
				ceremony_id: DOT_CEREMONY_ID_COUNTER_BEFORE_INITIAL_BLOCK + 1,
				epoch_index: 1,
				participants: Default::default(),
			}),
			CfeEvent::<Runtime>::BtcKeyHandoverRequest(KeyHandoverRequest::<Runtime, _> {
				ceremony_id: BTC_CEREMONY_ID_COUNTER_BEFORE_INITIAL_BLOCK + 1,
				from_epoch: 1,
				to_epoch: 2,
				key_to_share: cf_chains::btc::AggKey::default(),
				sharing_participants: Default::default(),
				receiving_participants: Default::default(),
				new_key: cf_chains::btc::AggKey::default(),
			}),
		],
	];

	// Run the function on all 3 test streams and check the ceremony id counters are correct
	for test_block_stream in test_block_streams {
		let mut state_chain_client = MockStateChainClient::new();

		state_chain_client
			.expect_storage_value::<pallet_cf_cfe_interface::CfeEvents<Runtime>>()
			.with(eq(block_hash))
			.once()
			.return_once(|_| Ok(test_block_stream));

		let ceremony_id_counters =
			get_ceremony_id_counters_before_block(block_hash, Arc::new(state_chain_client))
				.await
				.unwrap();

		assert_eq!(ceremony_id_counters.ethereum, ETH_CEREMONY_ID_COUNTER_BEFORE_INITIAL_BLOCK);
		assert_eq!(ceremony_id_counters.polkadot, DOT_CEREMONY_ID_COUNTER_BEFORE_INITIAL_BLOCK);
		assert_eq!(ceremony_id_counters.bitcoin, BTC_CEREMONY_ID_COUNTER_BEFORE_INITIAL_BLOCK);
	}
}

#[tokio::test]
async fn test_get_ceremony_id_counters_without_events() {
	const ETH_CEREMONY_ID_COUNTER: CeremonyId = 10;
	const DOT_CEREMONY_ID_COUNTER: CeremonyId = 20;
	const BTC_CEREMONY_ID_COUNTER: CeremonyId = 30;
	let block_hash = H256::default();
	let mut state_chain_client = MockStateChainClient::new();

	// Because the block has no events, we expect it to grab the ceremony id counter for each chain
	// from storage.
	state_chain_client
		.expect_storage_value::<pallet_cf_threshold_signature::CeremonyIdCounter<
			state_chain_runtime::Runtime,
			state_chain_runtime::EvmInstance,
		>>()
		.with(eq(block_hash))
		.once()
		.return_once(|_| Ok(ETH_CEREMONY_ID_COUNTER));
	state_chain_client
		.expect_storage_value::<pallet_cf_threshold_signature::CeremonyIdCounter<
			state_chain_runtime::Runtime,
			state_chain_runtime::PolkadotInstance,
		>>()
		.with(eq(block_hash))
		.once()
		.return_once(|_| Ok(DOT_CEREMONY_ID_COUNTER));
	state_chain_client
		.expect_storage_value::<pallet_cf_threshold_signature::CeremonyIdCounter<
			state_chain_runtime::Runtime,
			state_chain_runtime::BitcoinInstance,
		>>()
		.with(eq(block_hash))
		.once()
		.return_once(|_| Ok(BTC_CEREMONY_ID_COUNTER));

	// No events in the stream that would change the ceremony id counters
	state_chain_client
		.expect_storage_value::<pallet_cf_cfe_interface::CfeEvents<Runtime>>()
		.with(eq(block_hash))
		.once()
		.return_once(|_| {
			Ok(vec![
				// EthTxBroadcastRequest should not effect the ceremony id counter
				CfeEvent::<Runtime>::EthTxBroadcastRequest(TxBroadcastRequest::<Runtime, _> {
					broadcast_id: Default::default(),
					nominee: AccountId32::new([1; 32]),
					payload: Default::default(),
				}),
			])
		});

	let ceremony_id_counters =
		get_ceremony_id_counters_before_block(block_hash, Arc::new(state_chain_client))
			.await
			.unwrap();

	// Check the each ceremony id counter is the same as the one in storage
	assert_eq!(ceremony_id_counters.ethereum, ETH_CEREMONY_ID_COUNTER);
	assert_eq!(ceremony_id_counters.polkadot, DOT_CEREMONY_ID_COUNTER);
	assert_eq!(ceremony_id_counters.bitcoin, BTC_CEREMONY_ID_COUNTER);
}

#[tokio::test]
#[ignore = "runs forever, useful for testing without having to start the whole CFE"]
async fn run_the_sc_observer() {
	task_scope(|scope| {
		async {
			let settings = Settings::new_test().unwrap();

			let (sc_block_stream, _, state_chain_client) =
				crate::state_chain_observer::client::StateChainClient::connect_with_account(
					scope,
					&settings.state_chain.ws_endpoint,
					&settings.state_chain.signing_key_file,
					AccountRole::Unregistered,
					false,
					false,
					None,
				)
				.await
				.unwrap();

			sc_observer::start(
				state_chain_client,
				sc_block_stream,
				MockEvmRetryRpcClient::new(),
				MockEvmRetryRpcClient::new(),
				MockDotHttpRpcClient::new(),
				MockBtcRetryRpcClient::new(),
				MockMultisigClientApi::new(),
				MockMultisigClientApi::new(),
				MockMultisigClientApi::new(),
			)
			.await
			.unwrap_err();

			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap();
}
