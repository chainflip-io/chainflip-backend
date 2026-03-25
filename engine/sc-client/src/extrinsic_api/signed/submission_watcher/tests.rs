// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use base_rpc_api::WatchExtrinsicStream;
use cf_chains::{dot, ChainState};
use cf_utilities::task_scope::task_scope;
use futures::stream;
use futures_util::FutureExt;
use jsonrpsee::types::ErrorObject;
use sc_transaction_pool_api::TransactionStatus;
use std::sync::{
	atomic::{AtomicU8, Ordering},
	Arc,
};

use crate::{base_rpc_api::MockBaseRpcApi, SIGNED_EXTRINSIC_LIFETIME};

use super::*;

const INITIAL_NONCE: state_chain_runtime::Nonce = 10;

/// If the tx fails due to a bad proof, it should fetch the runtime version and retry.
#[tokio::test]
async fn should_update_version_on_bad_proof() {
	task_scope(|scope| {
		async {
			let mut mock_rpc_api = MockBaseRpcApi::new();

			mock_rpc_api.expect_next_account_nonce().return_once(move |_| Ok(1));
			mock_rpc_api.expect_submit_and_watch_extrinsic().times(1).returning(move |_| {
				Err(ErrorObject::owned(
					1010,
					"Invalid Transaction",
					Some("Transaction has a bad signature"),
				)
				.into())
			});

			mock_rpc_api.expect_runtime_version().times(1).returning(move |_| {
				let new_runtime_version = sp_version::RuntimeVersion {
					spec_name: "test".into(),
					impl_name: "test".into(),
					authoring_version: 0,
					spec_version: 0,
					impl_version: 0,
					apis: vec![].into(),
					transaction_version: 0,
					system_version: 0,
				};
				assert_ne!(
				new_runtime_version,
				Default::default(),
				"The new runtime version must be different from the version that the watcher started with"
			);

				Ok(new_runtime_version)
			});

			// On the retry, return a success.
			mock_rpc_api.expect_next_account_nonce().return_once(move |_| Ok(1));

			mock_rpc_api
				.expect_submit_and_watch_extrinsic()
				.return_once(move |_| Ok(Box::pin(stream::empty()) as WatchExtrinsicStream));

			let _watcher = new_watcher_and_submit_test_extrinsic(scope, mock_rpc_api).await;

			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap();
}

#[tokio::test]
async fn should_remove_terminated_submission_from_tracking() {
	task_scope(|scope| {
		async {
			let mut mock_rpc_api = MockBaseRpcApi::new();

			mock_rpc_api.expect_next_account_nonce().return_once(move |_| Ok(1));
			mock_rpc_api.expect_submit_and_watch_extrinsic().return_once(move |_| {
				Ok(Box::pin(stream::iter(vec![Ok(TransactionStatus::Dropped)]))
					as WatchExtrinsicStream)
			});

			let (mut watcher, mut requests) = SubmissionWatcher::new(
				scope,
				signer::PairSigner::new(sp_core::Pair::generate().0),
				INITIAL_NONCE,
				H256::default(),
				0,
				Default::default(),
				H256::default(),
				SIGNED_EXTRINSIC_LIFETIME,
				Arc::new(mock_rpc_api),
			);

			watcher
				.new_request(
					&mut requests,
					test_call(),
					oneshot::channel().0,
					oneshot::channel().0,
					RequestStrategy::AllowMultipleSubmissions,
				)
				.await?;

			assert_eq!(requests.get(&0).unwrap().pending_submissions.len(), 1);
			assert_eq!(watcher.submissions_by_nonce.len(), 1);

			let submission_details = watcher.watch_for_submission_in_block().await;
			watcher.on_submission_in_block(&mut requests, submission_details).await?;

			assert!(requests.get(&0).unwrap().pending_submissions.is_empty());
			assert!(watcher.submissions_by_nonce.is_empty());

			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap();
}

#[tokio::test]
async fn should_retry_after_dropped_on_next_finalized_block() {
	task_scope(|scope| {
		async {
			let mut mock_rpc_api = MockBaseRpcApi::new();
			let finalized_block_hash = H256::from_low_u64_be(42);

			// Initial submission + retry submission.
			mock_rpc_api.expect_next_account_nonce().times(2).returning(move |_| Ok(1));
			let submit_calls = Arc::new(AtomicU8::new(0));
			mock_rpc_api.expect_submit_and_watch_extrinsic().times(2).returning(move |_| {
				match submit_calls.fetch_add(1, Ordering::Relaxed) {
					0 => Ok(Box::pin(stream::iter(vec![Ok(TransactionStatus::Dropped)]))
						as WatchExtrinsicStream),
					_ => Ok(Box::pin(stream::empty()) as WatchExtrinsicStream),
				}
			});

			// Finalized block processing.
			mock_rpc_api
				.expect_block()
				.withf(move |hash| *hash == finalized_block_hash)
				.return_once(move |_| {
					Ok(Some(state_chain_runtime::SignedBlock {
						block: state_chain_runtime::Block {
							header: state_chain_runtime::Header {
								parent_hash: H256::default(),
								number: 1,
								state_root: H256::default(),
								extrinsics_root: H256::default(),
								digest: Default::default(),
							},
							extrinsics: vec![],
						},
						justifications: None,
					}))
				});
			// `on_block_finalized` fetches `System::Events` and `System::Account` (nonce).
			mock_rpc_api.expect_storage().times(2).returning(move |_, _| Ok(None));

			let (mut watcher, mut requests) = SubmissionWatcher::new(
				scope,
				signer::PairSigner::new(sp_core::Pair::generate().0),
				0,
				H256::default(),
				0,
				Default::default(),
				H256::default(),
				SIGNED_EXTRINSIC_LIFETIME,
				Arc::new(mock_rpc_api),
			);

			watcher
				.new_request(
					&mut requests,
					test_call(),
					oneshot::channel().0,
					oneshot::channel().0,
					RequestStrategy::AllowMultipleSubmissions,
				)
				.await?;

			let submission_details = watcher.watch_for_submission_in_block().await;
			watcher.on_submission_in_block(&mut requests, submission_details).await?;
			assert!(requests.get(&0).unwrap().pending_submissions.is_empty());

			watcher.on_block_finalized(&mut requests, finalized_block_hash).await?;

			let request = requests.get(&0).unwrap();
			assert_eq!(request.pending_submissions.len(), 1);
			assert_eq!(request.next_submission_id, 2);

			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap();
}

fn test_call() -> state_chain_runtime::RuntimeCall {
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
	})
}

/// Create a new watcher and submit a dummy extrinsic.
async fn new_watcher_and_submit_test_extrinsic<'a, 'env>(
	scope: &'a Scope<'env, anyhow::Error>,
	mock_rpc_api: MockBaseRpcApi,
) -> SubmissionWatcher<'a, 'env, MockBaseRpcApi> {
	let (mut watcher, _requests) = SubmissionWatcher::new(
		scope,
		signer::PairSigner::new(sp_core::Pair::generate().0),
		INITIAL_NONCE,
		H256::default(),
		0,
		Default::default(),
		H256::default(),
		SIGNED_EXTRINSIC_LIFETIME,
		Arc::new(mock_rpc_api),
	);

	let mut request = Request {
		id: 0,
		next_submission_id: 0,
		pending_submissions: Default::default(),
		strictly_one_submission: false,
		resubmit_window: ..=1,
		call: test_call(),
		until_in_block_sender: Some(oneshot::channel().0),
		until_finalized_sender: oneshot::channel().0,
	};

	let _result = watcher.submit_extrinsic(&mut request).await;

	watcher
}
