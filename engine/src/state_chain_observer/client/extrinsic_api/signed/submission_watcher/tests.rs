use frame_system::AccountInfo;
use jsonrpsee::types::ErrorObject;
use mockall::predicate::eq;
use sp_core::{
	storage::{StorageData, StorageKey},
	Encode,
};
use utilities::assert_ok;

use super::{signer::PairSigner, *};
use crate::{
	constants::SIGNED_EXTRINSIC_LIFETIME,
	state_chain_observer::client::{
		base_rpc_api::MockBaseRpcApi, extrinsic_api::signed::tests::DUMMY_CALL,
	},
};

const INITIAL_NONCE: Nonce = 10;
const INITIAL_BLOCK_NUMBER: BlockNumber = 0;

// TODO: deduplicate this with the one in the signer module
fn test_header(number: u32) -> state_chain_runtime::Header {
	state_chain_runtime::Header {
		number,
		parent_hash: H256::default(),
		state_root: H256::default(),
		extrinsics_root: H256::default(),
		digest: sp_runtime::Digest { logs: Vec::new() },
	}
}

#[tokio::test]
async fn should_increment_nonce_on_success() {
	let mut mock_rpc_api = MockBaseRpcApi::new();

	// Return a success, cause the nonce to increment
	mock_rpc_api
		.expect_submit_extrinsic()
		.once()
		.returning(move |_| Ok(Default::default()));

	let watcher = new_watcher_and_submit_test_extrinsic(mock_rpc_api).await;

	assert_eq!(watcher.anticipated_nonce, INITIAL_NONCE + 1);
}

/// If the tx fails due to the same nonce existing in the pool already, it should increment the
/// nonce and try again.
#[tokio::test]
async fn should_increment_and_retry_if_nonce_in_pool() {
	let mut mock_rpc_api = MockBaseRpcApi::new();

	mock_rpc_api.expect_submit_extrinsic().once().returning(move |_| {
		Err(jsonrpsee::core::Error::Call(jsonrpsee::types::error::CallError::Custom(
			ErrorObject::from(jsonrpsee::types::error::ErrorCode::ServerError(1014)),
		)))
	});

	// On the retry, return a success.
	mock_rpc_api
		.expect_submit_extrinsic()
		.once()
		.returning(move |_| Ok(Default::default()));

	let watcher = new_watcher_and_submit_test_extrinsic(mock_rpc_api).await;

	// Nonce should be +2, once for the initial submission, and once for the retry
	assert_eq!(watcher.anticipated_nonce, INITIAL_NONCE + 2);
}

#[tokio::test]
async fn should_increment_and_retry_if_nonce_consumed_in_prev_blocks() {
	let mut mock_rpc_api = MockBaseRpcApi::new();

	mock_rpc_api.expect_submit_extrinsic().once().returning(move |_| {
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
		.once()
		.returning(move |_| Ok(Default::default()));

	let watcher = new_watcher_and_submit_test_extrinsic(mock_rpc_api).await;

	// Nonce should be +2, once for the initial submission, and once for the retry
	assert_eq!(watcher.anticipated_nonce, INITIAL_NONCE + 2);
}

/// If the tx fails due to a bad proof, it should fetch the runtime version and retry.
#[tokio::test]
async fn should_update_version_on_bad_proof() {
	let mut mock_rpc_api = MockBaseRpcApi::new();

	mock_rpc_api.expect_submit_extrinsic().once().returning(move |_| {
		Err(jsonrpsee::core::Error::Call(jsonrpsee::types::error::CallError::Custom(
			jsonrpsee::types::ErrorObject::owned(
				1010,
				"Invalid Transaction",
				Some("Transaction has a bad signature"),
			),
		)))
	});

	mock_rpc_api.expect_runtime_version().once().returning(move || {
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
		.once()
		.returning(move |_| Ok(Default::default()));

	let watcher = new_watcher_and_submit_test_extrinsic(mock_rpc_api).await;

	// The bad proof should not have incremented the nonce, so it should only be +1 from the retry.
	assert_eq!(watcher.anticipated_nonce, INITIAL_NONCE + 1);
}

/// If the tx fails due to an error that is unrelated to the nonce, it should not increment the
/// nonce and not retry.
#[tokio::test]
async fn should_not_increment_nonce_on_unrelated_failure() {
	let mut mock_rpc_api = MockBaseRpcApi::new();

	mock_rpc_api.expect_submit_extrinsic().once().returning(move |_| {
		Err(jsonrpsee::core::Error::Custom("some unrelated error".to_string()))
	});

	let watcher = new_watcher_and_submit_test_extrinsic(mock_rpc_api).await;

	assert_eq!(watcher.anticipated_nonce, INITIAL_NONCE);
}

/// Create a new watcher and submit a dummy extrinsic.
async fn new_watcher_and_submit_test_extrinsic(
	mock_rpc_api: MockBaseRpcApi,
) -> SubmissionWatcher<MockBaseRpcApi> {
	let (mut watcher, _requests) = new_watcher_with_defaults(mock_rpc_api);

	let mut request = Request {
		id: 0,
		pending_submissions: 0,
		allow_resubmits: false,
		lifetime: ..=1,
		call: DUMMY_CALL.clone(),
		result_sender: oneshot::channel().0,
	};

	assert_eq!(watcher.anticipated_nonce, INITIAL_NONCE, "Nonce should start at INITIAL_NONCE");
	let _result = watcher.submit_extrinsic(&mut request).await;

	watcher
}

/// Create a new submission watcher with INITIAL_NONCE, INITIAL_BLOCK_NUMBER and default values.
fn new_watcher_with_defaults(
	mock_rpc_api: MockBaseRpcApi,
) -> (SubmissionWatcher<MockBaseRpcApi>, BTreeMap<RequestID, Request>) {
	SubmissionWatcher::new(
		signer::PairSigner::new(sp_core::Pair::generate().0),
		INITIAL_NONCE,
		Default::default(),
		INITIAL_BLOCK_NUMBER,
		Default::default(),
		Default::default(),
		SIGNED_EXTRINSIC_LIFETIME,
		Arc::new(mock_rpc_api),
	)
}

#[tokio::test]
async fn test_submit_extrinsic_at_nonce() {
	const NONCE: Nonce = INITIAL_NONCE + 10;
	const REQUEST_ID: RequestID = 1;

	let mut mock_rpc_api = MockBaseRpcApi::new();

	mock_rpc_api
		.expect_submit_extrinsic()
		.once()
		.returning(move |_| Ok(Default::default()));

	let (mut watcher, _requests) = new_watcher_with_defaults(mock_rpc_api);

	let mut request = Request {
		id: REQUEST_ID,
		pending_submissions: 0,
		allow_resubmits: false,
		lifetime: ..=1,
		call: DUMMY_CALL.clone(),
		result_sender: oneshot::channel().0,
	};

	// Sanity check that the watcher starts with no submissions
	assert!(watcher.submissions_by_nonce.is_empty());

	let _result = watcher.submit_extrinsic_at_nonce(&mut request, NONCE).await;

	// The new submission should have been added to the watcher and the request should have 1
	// pending submission
	assert_eq!(
		watcher
			.submissions_by_nonce
			.get(&NONCE)
			.unwrap()
			.iter()
			.next()
			.unwrap()
			.request_id,
		REQUEST_ID
	);
	assert_eq!(request.pending_submissions, 1);
}

#[tokio::test]
async fn test_cleanup_expired_requests() {
	let mock_rpc_api = MockBaseRpcApi::new();

	let (mut watcher, mut requests) = new_watcher_with_defaults(mock_rpc_api);

	const TEST_BLOCK_NUMBER: BlockNumber = 10;

	const REQUEST_ID_ZERO_SUBMISSIONS: RequestID = 1;
	requests.insert(
		REQUEST_ID_ZERO_SUBMISSIONS,
		Request {
			id: REQUEST_ID_ZERO_SUBMISSIONS,
			// No submissions left
			pending_submissions: 0,
			allow_resubmits: false,
			lifetime: ..=TEST_BLOCK_NUMBER + 1,
			call: DUMMY_CALL.clone(),
			result_sender: oneshot::channel().0,
		},
	);

	const REQUEST_ID_LIFETIME_OVER: RequestID = 2;
	requests.insert(
		REQUEST_ID_LIFETIME_OVER,
		Request {
			id: REQUEST_ID_LIFETIME_OVER,
			pending_submissions: 0,
			allow_resubmits: false,
			// Lifetime is over this block
			lifetime: ..=TEST_BLOCK_NUMBER,
			call: DUMMY_CALL.clone(),
			result_sender: oneshot::channel().0,
		},
	);

	const REQUEST_ID_NOT_EXPIRED: RequestID = 3;
	requests.insert(
		REQUEST_ID_NOT_EXPIRED,
		Request {
			id: REQUEST_ID_NOT_EXPIRED,
			// Has one submission left
			pending_submissions: 1,
			allow_resubmits: false,
			lifetime: ..=TEST_BLOCK_NUMBER + 1,
			call: DUMMY_CALL.clone(),
			result_sender: oneshot::channel().0,
		},
	);

	assert_eq!(requests.len(), 3);

	watcher.cleanup_requests(TEST_BLOCK_NUMBER, &mut requests).await.unwrap();

	// These 2 request should have been removed
	assert!(!requests.contains_key(&REQUEST_ID_ZERO_SUBMISSIONS));
	assert!(!requests.contains_key(&REQUEST_ID_LIFETIME_OVER));

	// This request should still be there without a change to the pending submissions
	assert_eq!(requests.get(&REQUEST_ID_NOT_EXPIRED).unwrap().pending_submissions, 1);
}

#[tokio::test]
async fn test_resubmit_expired_requests() {
	let mut mock_rpc_api = MockBaseRpcApi::new();

	// The cleanup should trigger a re-submission
	mock_rpc_api
		.expect_submit_extrinsic()
		.once()
		.returning(move |_| Ok(Default::default()));

	let (mut watcher, mut requests) = new_watcher_with_defaults(mock_rpc_api);

	const TEST_BLOCK_NUMBER: BlockNumber = 10;

	const REQUEST_ID_ALLOW_RESUBMIT: RequestID = 4;
	requests.insert(
		REQUEST_ID_ALLOW_RESUBMIT,
		Request {
			id: REQUEST_ID_ALLOW_RESUBMIT,
			// Has 0 submissions left, but allows resubmits
			pending_submissions: 0,
			allow_resubmits: true,
			lifetime: ..=TEST_BLOCK_NUMBER + 1,
			call: DUMMY_CALL.clone(),
			result_sender: oneshot::channel().0,
		},
	);

	watcher.cleanup_requests(TEST_BLOCK_NUMBER, &mut requests).await.unwrap();

	// The request should still be there, but now with 1 pending submission
	assert_eq!(requests.get(&REQUEST_ID_ALLOW_RESUBMIT).unwrap().pending_submissions, 1);
}

#[test]
fn test_cleanup_expired_submissions() {
	const TEST_BLOCK_NUMBER: BlockNumber = INITIAL_BLOCK_NUMBER + 1;
	const REQUEST_ID: RequestID = 5;
	const NONCE: Nonce = INITIAL_NONCE + 1;

	let mock_rpc_api = MockBaseRpcApi::new();

	let submissions = vec![
		Submission {
			// Not Expired
			lifetime: ..TEST_BLOCK_NUMBER + 2,
			tx_hash: Default::default(),
			request_id: REQUEST_ID,
		},
		Submission {
			// Expired
			lifetime: ..TEST_BLOCK_NUMBER + 1,
			tx_hash: Default::default(),
			request_id: REQUEST_ID,
		},
	];

	let mut requests: BTreeMap<RequestID, Request> = BTreeMap::new();
	requests.insert(
		REQUEST_ID,
		Request {
			id: REQUEST_ID,
			// Has 2 pending submissions
			pending_submissions: 2,
			allow_resubmits: false,
			lifetime: ..=TEST_BLOCK_NUMBER + 1,
			call: DUMMY_CALL.clone(),
			result_sender: oneshot::channel().0,
		},
	);

	// Create a submission watcher with `submissions_by_nonce` already filled
	let mut watcher = SubmissionWatcher {
		submissions_by_nonce: BTreeMap::from_iter(vec![(NONCE, submissions)].into_iter()),
		anticipated_nonce: NONCE + 1,
		signer: signer::PairSigner::new(sp_core::Pair::generate().0),
		finalized_nonce: INITIAL_NONCE,
		finalized_block_hash: Default::default(),
		finalized_block_number: INITIAL_BLOCK_NUMBER,
		runtime_version: Default::default(),
		genesis_hash: Default::default(),
		extrinsic_lifetime: SIGNED_EXTRINSIC_LIFETIME,
		base_rpc_client: Arc::new(mock_rpc_api),
	};

	// Sanity check that the number of submissions match up
	assert_eq!(
		requests.get(&REQUEST_ID).unwrap().pending_submissions,
		watcher.submissions_by_nonce.iter().next().unwrap().1.len()
	);

	watcher.cleanup_submissions(TEST_BLOCK_NUMBER, &mut requests);

	// The expired submission should have been removed, leaving the request with 1 pending
	assert_eq!(requests.get(&REQUEST_ID).unwrap().pending_submissions, 1);
	assert_eq!(watcher.submissions_by_nonce.get(&NONCE).unwrap().len(), 1);
}

#[tokio::test]
async fn test_update_finalized_data() {
	const TEST_BLOCK_NUMBER: BlockNumber = INITIAL_BLOCK_NUMBER + 1;
	const NEW_NONCE: Nonce = INITIAL_NONCE + 1;

	let mut mock_rpc_api = MockBaseRpcApi::new();
	let signer = PairSigner::new(sp_core::Pair::generate().0);
	let new_hash = H256::random();

	// The watcher should request the block info for an up-to-date block number
	mock_rpc_api.expect_block().once().return_once(|_| {
		Ok(Some(state_chain_runtime::SignedBlock {
			block: state_chain_runtime::Block {
				header: test_header(TEST_BLOCK_NUMBER),
				extrinsics: vec![],
			},
			justifications: None,
		}))
	});

	// The watcher will request the events, not relevant for this test
	mock_rpc_api.expect_storage().once().return_once(move |_, _| Ok(None));

	// The watcher should request account info for an up-to-date nonce.
	let account_info = AccountInfo {
		nonce: NEW_NONCE,
		consumers: 0,
		providers: 0,
		sufficients: 0,
		data: vec![()],
	};
	mock_rpc_api
		.expect_storage()
		.with(
			eq(new_hash),
			eq(StorageKey(frame_system::Account::<state_chain_runtime::Runtime>::hashed_key_for(
				&signer.account_id,
			))),
		)
		.once()
		.return_once(move |_, _| Ok(Some(StorageData(account_info.encode()))));

	let (mut watcher, mut requests) = SubmissionWatcher::new(
		signer,
		INITIAL_NONCE,
		Default::default(),
		INITIAL_BLOCK_NUMBER,
		Default::default(),
		Default::default(),
		SIGNED_EXTRINSIC_LIFETIME,
		Arc::new(mock_rpc_api),
	);

	// Sanity check that the values are set correctly before the update
	assert_eq!(watcher.finalized_nonce, INITIAL_NONCE);
	assert_eq!(watcher.anticipated_nonce, INITIAL_NONCE);
	assert_eq!(watcher.finalized_block_hash, Default::default());
	assert_eq!(watcher.finalized_block_number, INITIAL_BLOCK_NUMBER);

	watcher.on_block_finalized(&mut requests, new_hash).await.unwrap();

	// All values should have been updated
	assert_eq!(watcher.finalized_nonce, NEW_NONCE);
	assert_eq!(watcher.anticipated_nonce, NEW_NONCE);
	assert_eq!(watcher.finalized_block_hash, new_hash);
	assert_eq!(watcher.finalized_block_number, TEST_BLOCK_NUMBER);
}

#[tokio::test]
async fn should_error_if_account_nonce_falls_back() {
	let mut mock_rpc_api = MockBaseRpcApi::new();
	let signer = PairSigner::new(sp_core::Pair::generate().0);
	let new_hash = H256::random();

	// The watcher should request the block info for an up-to-date block number
	mock_rpc_api.expect_block().once().return_once(|_| {
		Ok(Some(state_chain_runtime::SignedBlock {
			block: state_chain_runtime::Block {
				header: test_header(INITIAL_BLOCK_NUMBER + 1),
				extrinsics: vec![],
			},
			justifications: None,
		}))
	});

	// The watcher will request the events, not relevant for this test
	mock_rpc_api.expect_storage().once().return_once(move |_, _| Ok(None));

	// Return a nonce that is below the watchers finalized nonce (INITIAL_NONCE)
	let account_info = AccountInfo {
		nonce: INITIAL_NONCE - 1,
		consumers: 0,
		providers: 0,
		sufficients: 0,
		data: vec![()],
	};
	mock_rpc_api
		.expect_storage()
		.with(
			eq(new_hash),
			eq(StorageKey(frame_system::Account::<state_chain_runtime::Runtime>::hashed_key_for(
				&signer.account_id,
			))),
		)
		.once()
		.return_once(move |_, _| Ok(Some(StorageData(account_info.encode()))));

	let (mut watcher, mut requests) = SubmissionWatcher::new(
		signer,
		INITIAL_NONCE,
		Default::default(),
		INITIAL_BLOCK_NUMBER,
		Default::default(),
		Default::default(),
		SIGNED_EXTRINSIC_LIFETIME,
		Arc::new(mock_rpc_api),
	);

	assert_eq!(watcher.finalized_nonce, INITIAL_NONCE);

	// The function should return an error
	watcher.on_block_finalized(&mut requests, new_hash).await.unwrap_err();
}

#[test]
fn test_find_submission_and_process() {
	const TEST_BLOCK_NUMBER: BlockNumber = INITIAL_BLOCK_NUMBER + 1;
	const REQUEST_ID: RequestID = 5;
	const NONCE: Nonce = INITIAL_NONCE + 1;

	let mock_rpc_api = MockBaseRpcApi::new();
	let signer = signer::PairSigner::<sp_core::sr25519::Pair>::new(sp_core::Pair::generate().0);

	// This is the extrinsic that it will be looking for
	let extrinsic = signer
		.new_signed_extrinsic(
			DUMMY_CALL.clone(),
			&Default::default(),
			Default::default(),
			Default::default(),
			INITIAL_BLOCK_NUMBER,
			SIGNED_EXTRINSIC_LIFETIME,
			NONCE,
		)
		.0;

	let tx_hash =
		<state_chain_runtime::Runtime as frame_system::Config>::Hashing::hash_of(&extrinsic);

	// The submission has a tx_hash that matches the extrinsic
	let submissions =
		vec![Submission { lifetime: ..TEST_BLOCK_NUMBER + 2, tx_hash, request_id: REQUEST_ID }];

	let (result_sender, mut result_receiver) = oneshot::channel();

	let mut requests: BTreeMap<RequestID, Request> = BTreeMap::new();
	requests.insert(
		REQUEST_ID,
		Request {
			id: REQUEST_ID,
			pending_submissions: submissions.len(),
			allow_resubmits: false,
			lifetime: ..=TEST_BLOCK_NUMBER + 1,
			call: DUMMY_CALL.clone(),
			result_sender,
		},
	);

	let header = test_header(TEST_BLOCK_NUMBER);
	let block =
		state_chain_runtime::Block { header: header.clone(), extrinsics: vec![extrinsic.clone()] };

	let events =
		vec![state_chain_runtime::RuntimeEvent::System(frame_system::Event::ExtrinsicSuccess {
			dispatch_info: Default::default(),
		})];

	// Create a submission watcher with `submissions_by_nonce` already filled
	let mut watcher = SubmissionWatcher {
		submissions_by_nonce: BTreeMap::from_iter(vec![(NONCE, submissions)].into_iter()),
		anticipated_nonce: NONCE + 1,
		signer,
		finalized_nonce: INITIAL_NONCE,
		finalized_block_hash: Default::default(),
		finalized_block_number: INITIAL_BLOCK_NUMBER,
		runtime_version: Default::default(),
		genesis_hash: Default::default(),
		extrinsic_lifetime: SIGNED_EXTRINSIC_LIFETIME,
		base_rpc_client: Arc::new(mock_rpc_api),
	};

	watcher.find_submission_and_process(&extrinsic, events.iter().cloned(), &mut requests, &block);

	// The request and submission should have completed and we should receive the events
	assert!(requests.is_empty());
	assert!(watcher.submissions_by_nonce.is_empty());
	assert_eq!(
		result_receiver.try_recv().unwrap().unwrap(),
		(tx_hash, events, header, DispatchInfo::default())
	);
}

#[tokio::test]
/// Test the `Submit` strategy in the `new_request` function.
async fn test_submit_request() {
	const REQUEST_ID: RequestID = 3;

	let mut mock_rpc_api = MockBaseRpcApi::new();

	mock_rpc_api
		.expect_submit_extrinsic()
		.once()
		.returning(move |_| Ok(Default::default()));

	let (mut watcher, mut requests) = new_watcher_with_defaults(mock_rpc_api);

	// Fill the requests with some junk data to make sure the new request is appended correctly
	(0..REQUEST_ID).for_each(|id| {
		requests.insert(
			id,
			Request {
				id,
				pending_submissions: 0,
				allow_resubmits: false,
				lifetime: ..=1,
				call: DUMMY_CALL.clone(),
				result_sender: oneshot::channel().0,
			},
		);
	});

	let (result_sender, mut result_receiver) = oneshot::channel();

	assert_ok!(
		watcher
			.new_request(
				&mut requests,
				DUMMY_CALL.clone(),
				oneshot::channel().0,
				RequestStrategy::Submit(result_sender)
			)
			.await
	);

	// Check the details of the new request
	let request = requests.get(&REQUEST_ID).unwrap();
	assert_eq!(request.pending_submissions, 1);
	assert!(!request.allow_resubmits);
	assert_eq!(request.lifetime, ..=(INITIAL_BLOCK_NUMBER + 1 + REQUEST_LIFETIME));

	// Should receive the hash of the submitted extrinsic
	assert_eq!(result_receiver.try_recv().unwrap(), Default::default());
}

#[tokio::test]
async fn should_fix_gap_in_submission_nonces() {
	let mut mock_rpc_api = MockBaseRpcApi::new();
	let signer = PairSigner::new(sp_core::Pair::generate().0);

	// For this test we are pretending that the finalized nonce is 10, but there is a pretend
	// existing submission at 11, so the gap must be filled.
	const FINALIZED_NONCE: state_chain_runtime::Index = 10;
	const EXISTING_SUBMISSION_NONCE: state_chain_runtime::Index = FINALIZED_NONCE + 1;

	// It will submit an extrinsic at the finalized nonce. We return a success, this is where the
	// fault is detected
	mock_rpc_api.expect_submit_extrinsic().once().returning(move |extrinsic| {
		let (_, _, (.., frame_system::CheckNonce(nonce), _, _)) = extrinsic.signature.unwrap();
		assert_eq!(nonce, FINALIZED_NONCE);
		Ok(Default::default())
	});

	// It will try and submit another extrinsic at the finalized nonce + 1. This will return an
	// error because a submitted extrinsic with the same nonce already exists. The gap was filled
	// and this is the confirmation.
	mock_rpc_api.expect_submit_extrinsic().once().returning(move |extrinsic| {
		let (_, _, (.., frame_system::CheckNonce(nonce), _, _)) = extrinsic.signature.unwrap();
		assert_eq!(nonce, EXISTING_SUBMISSION_NONCE);
		Err(jsonrpsee::core::Error::Call(jsonrpsee::types::error::CallError::Custom(
			ErrorObject::from(jsonrpsee::types::error::ErrorCode::ServerError(1014)),
		)))
	});

	let (mut watcher, mut requests) = SubmissionWatcher::new(
		signer,
		FINALIZED_NONCE,
		Default::default(),
		0,
		Default::default(),
		Default::default(),
		SIGNED_EXTRINSIC_LIFETIME,
		Arc::new(mock_rpc_api),
	);
	assert_eq!(watcher.finalized_nonce, FINALIZED_NONCE);

	// setting the anticipated_nonce forward to simulate a real world scenario
	watcher.anticipated_nonce = EXISTING_SUBMISSION_NONCE + 1;

	// We need 2 requests that allow resubmits to be able to fix the nonce gap of 1 in a single run
	requests.insert(
		1,
		Request {
			id: 1,
			pending_submissions: 1,
			allow_resubmits: true,
			lifetime: ..=1,
			call: DUMMY_CALL.clone(),
			result_sender: oneshot::channel().0,
		},
	);
	requests.insert(
		2,
		Request {
			id: 2,
			pending_submissions: 0,
			allow_resubmits: true,
			lifetime: ..=1,
			call: DUMMY_CALL.clone(),
			result_sender: oneshot::channel().0,
		},
	);

	watcher.detect_nonce_fault_and_try_fix(&mut requests).await.unwrap();

	// The next nonce that will be used should be EXISTING_SUBMISSION_NONCE + 1 because
	// EXISTING_SUBMISSION_NONCE is already taken
	assert_eq!(watcher.anticipated_nonce, EXISTING_SUBMISSION_NONCE + 1);
}
