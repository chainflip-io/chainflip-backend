use cf_chains::dot;
use jsonrpsee::types::ErrorObject;

use crate::{
	constants::SIGNED_EXTRINSIC_LIFETIME,
	state_chain_observer::client::base_rpc_api::MockBaseRpcApi,
};

use super::*;

const INITIAL_NONCE: state_chain_runtime::Index = 10;

#[tokio::test]
async fn should_increment_nonce_on_success() {
	let mut mock_rpc_api = MockBaseRpcApi::new();

	// Return a success, cause the nonce to increment
	mock_rpc_api
		.expect_submit_extrinsic()
		.times(1)
		.returning(move |_| Ok(H256::default()));

	let watcher = new_watcher_and_submit_test_extrinsic(mock_rpc_api).await;

	assert_eq!(watcher.get_anticipated_nonce(), INITIAL_NONCE + 1);
}

/// If the tx fails due to the same nonce existing in the pool already, it should increment the
/// nonce and try again.
#[tokio::test]
async fn should_increment_and_retry_if_nonce_in_pool() {
	let mut mock_rpc_api = MockBaseRpcApi::new();

	mock_rpc_api.expect_submit_extrinsic().times(1).returning(move |_| {
		Err(jsonrpsee::core::Error::Call(jsonrpsee::types::error::CallError::Custom(
			ErrorObject::from(jsonrpsee::types::error::ErrorCode::ServerError(1014)),
		)))
	});

	// On the retry, return a success.
	mock_rpc_api
		.expect_submit_extrinsic()
		.times(1)
		.returning(move |_| Ok(H256::default()));

	let watcher = new_watcher_and_submit_test_extrinsic(mock_rpc_api).await;

	// Nonce should be +2, once for the initial submission, and once for the retry
	assert_eq!(watcher.get_anticipated_nonce(), INITIAL_NONCE + 2);
}

#[tokio::test]
async fn should_increment_and_retry_if_nonce_consumed_in_prev_blocks() {
	let mut mock_rpc_api = MockBaseRpcApi::new();

	mock_rpc_api.expect_submit_extrinsic().times(1).returning(move |_| {
		Err(jsonrpsee::core::Error::Call(jsonrpsee::types::error::CallError::Custom(
			jsonrpsee::types::ErrorObject::owned(
				1010,
				"Invalid Transaction",
				Some("Transaction is outdated"),
			),
		)))
	});

	// On the retry, return a success.
	mock_rpc_api
		.expect_submit_extrinsic()
		.times(1)
		.returning(move |_| Ok(H256::default()));

	let watcher = new_watcher_and_submit_test_extrinsic(mock_rpc_api).await;

	// Nonce should be +2, once for the initial submission, and once for the retry
	assert_eq!(watcher.get_anticipated_nonce(), INITIAL_NONCE + 2);
}

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
		assert_ne!(new_runtime_version, Default::default(), 
            "The new runtime version must be different from the version that the watcher started with");

		Ok(new_runtime_version)
	});

	// On the retry, return a success.
	mock_rpc_api
		.expect_submit_extrinsic()
		.times(1)
		.returning(move |_| Ok(H256::default()));

	let watcher = new_watcher_and_submit_test_extrinsic(mock_rpc_api).await;

	// The bad proof should not have incremented the nonce, so it should only be +1 from the retry.
	assert_eq!(watcher.get_anticipated_nonce(), INITIAL_NONCE + 1);
}

/// If the tx fails due to an error that is unrelated to the nonce, it should not increment the
/// nonce and not retry.
#[tokio::test]
async fn should_not_increment_nonce_on_unrelated_failure() {
	let mut mock_rpc_api = MockBaseRpcApi::new();

	mock_rpc_api.expect_submit_extrinsic().times(1).returning(move |_| {
		Err(jsonrpsee::core::Error::Custom("some unrelated error".to_string()))
	});

	let watcher = new_watcher_and_submit_test_extrinsic(mock_rpc_api).await;

	assert_eq!(watcher.get_anticipated_nonce(), INITIAL_NONCE);
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
					state: dot::PolkadotTrackedData { block_height: 0, median_tip: 0 },
				},
			)),
			epoch_index: 0,
		});
	let mut request = Request {
		id: 0,
		pending_submissions: 0,
		allow_resubmits: false,
		lifetime: ..=1,
		call,
		result_sender: oneshot::channel().0,
	};

	assert_eq!(
		watcher.get_anticipated_nonce(),
		INITIAL_NONCE,
		"Nonce should start at INITIAL_NONCE"
	);
	let _result = watcher.submit_extrinsic(&mut request).await;

	watcher
}
