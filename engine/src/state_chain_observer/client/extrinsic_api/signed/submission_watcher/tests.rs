use cf_chains::{dot, ChainState};

use crate::{
	constants::SIGNED_EXTRINSIC_LIFETIME,
	state_chain_observer::client::base_rpc_api::MockBaseRpcApi,
};

use super::*;

const INITIAL_NONCE: state_chain_runtime::Nonce = 10;

/// If the tx fails due to a bad proof, it should fetch the runtime version and retry.
#[tokio::test]
async fn should_update_version_on_bad_proof() {
	let mut mock_rpc_api = MockBaseRpcApi::new();

	mock_rpc_api.expect_submit_extrinsic().times(1).returning(move |_| {
		Err(jsonrpsee::core::Error::Call(jsonrpsee::types::error::CallError::Custom(
			jsonrpsee::types::ErrorObject::owned(
				1010,
				"Invalid Transaction",
				Some("Transaction has a bad signature"),
			),
		)))
	});

	mock_rpc_api.expect_runtime_version().times(1).returning(move || {
		let new_runtime_version = sp_version::RuntimeVersion {
			spec_name: "test".into(),
			impl_name: "test".into(),
			authoring_version: 0,
			spec_version: 0,
			impl_version: 0,
			apis: vec![].into(),
			transaction_version: 0,
			state_version: 0,
		};
		assert_ne!(
			new_runtime_version,
			Default::default(),
			"The new runtime version must be different from the version that the watcher started with"
		);

		Ok(new_runtime_version)
	});

	// On the retry, return a success.
	mock_rpc_api
		.expect_submit_extrinsic()
		.times(1)
		.returning(move |_| Ok(H256::default()));

	let _watcher = new_watcher_and_submit_test_extrinsic(mock_rpc_api).await;
}

/// Create a new watcher and submit a dummy extrinsic.
async fn new_watcher_and_submit_test_extrinsic(
	mock_rpc_api: MockBaseRpcApi,
) -> SubmissionWatcher<MockBaseRpcApi> {
	let (mut watcher, _requests) = SubmissionWatcher::new(
		signer::PairSigner::new(sp_core::Pair::generate().0),
		INITIAL_NONCE,
		H256::default(),
		0,
		Default::default(),
		H256::default(),
		SIGNED_EXTRINSIC_LIFETIME,
		Arc::new(mock_rpc_api),
	);

	// Just some dummy call to test with
	let call =
		state_chain_runtime::RuntimeCall::Witnesser(pallet_cf_witnesser::Call::witness_at_epoch {
			call: Box::new(state_chain_runtime::RuntimeCall::PolkadotChainTracking(
				pallet_cf_chain_tracking::Call::update_chain_state {
					new_chain_state: ChainState {
						block_height: 0,
						tracked_data: dot::PolkadotTrackedData {
							median_tip: 0,
							runtime_version: Default::default(),
						},
					},
				},
			)),
			epoch_index: 0,
		});
	let mut request = Request {
		id: 0,
		next_submission_id: 0,
		pending_submissions: Default::default(),
		strictly_one_submission: false,
		resubmit_window: ..=1,
		call,
		_until_in_block_sender: Some(oneshot::channel().0),
		until_finalized_sender: oneshot::channel().0,
	};

	let _result = watcher.submit_extrinsic(&mut request).await;

	watcher
}
