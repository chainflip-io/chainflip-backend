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
use codec::Encode;
use futures::stream;
use futures_util::FutureExt;
use jsonrpsee::types::ErrorObject;
use sc_transaction_pool_api::TransactionStatus;
use sp_core::storage::StorageData;
use sp_runtime::traits::Header as _;
use std::{
	sync::{
		atomic::{AtomicU8, Ordering},
		Arc,
	},
	time::Duration,
};

use crate::{base_rpc_api::MockBaseRpcApi, SIGNED_EXTRINSIC_LIFETIME};

use super::*;

const INITIAL_NONCE: state_chain_runtime::Nonce = 10;

/// If the tx fails due to a bad proof, it should fetch the runtime version and retry.
#[tokio::test]
async fn should_update_version_on_bad_proof() {
	tokio::time::timeout(
		Duration::from_secs(5),
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
		}),
	)
	.await
	.expect("runtime version refresh path should not hang")
	.unwrap();
}

/// A burst of BadProofs within one finalized block must refetch the runtime version at most once
/// (it only changes on a runtime upgrade), not once per failure.
#[tokio::test]
async fn bad_proof_burst_refetches_version_once_per_block() {
	task_scope(|scope| {
		async {
			let mut mock_rpc_api = MockBaseRpcApi::new();

			mock_rpc_api.expect_next_account_nonce().return_once(move |_| Ok(1));

			// Two BadProofs then success, all within the same (initial) finalized block.
			let submit_calls = Arc::new(AtomicU8::new(0));
			mock_rpc_api.expect_submit_and_watch_extrinsic().times(3).returning(move |_| {
				match submit_calls.fetch_add(1, Ordering::Relaxed) {
					0 | 1 => Err(ErrorObject::owned(
						1010,
						"Invalid Transaction",
						Some("Transaction has a bad signature"),
					)
					.into()),
					_ => Ok(Box::pin(stream::empty()) as WatchExtrinsicStream),
				}
			});

			// Rate-limited: refetched once despite two BadProofs in the same block.
			mock_rpc_api
				.expect_runtime_version()
				.times(1)
				.returning(move |_| Ok(Default::default()));

			let mut watcher =
				new_watcher_with_mock_rpc_api(scope, mock_rpc_api, INITIAL_NONCE, 0).await;
			let mut request = new_test_request(0, false);
			watcher.submit_extrinsic(&mut request).await.unwrap();

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
				0,
				finalized_watch(0),
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
async fn should_track_submission_when_already_in_pool() {
	const NONCE: state_chain_runtime::Nonce = 1;

	task_scope(|scope| {
		async {
			let mut mock_rpc_api = MockBaseRpcApi::new();

			mock_rpc_api.expect_submit_and_watch_extrinsic().times(1).return_once(move |_| {
				Err(ErrorObject::owned(
					POOL_ALREADY_IMPORTED,
					"Already Imported",
					Option::<()>::None,
				)
				.into())
			});

			let mut watcher =
				new_watcher_with_mock_rpc_api(scope, mock_rpc_api, INITIAL_NONCE, 0).await;
			let mut request = new_test_request(7, false);

			let tx_hash = watcher
				.submit_extrinsic_at_nonce(&mut request, NONCE)
				.await?
				.map_err(|_| anyhow::anyhow!("Unexpected submission logic error"))?;

			assert_eq!(request.next_submission_id, 1);
			assert_eq!(request.pending_submissions.get(&0), Some(&NONCE));

			let submission = watcher
				.submissions_by_nonce
				.get(&NONCE)
				.and_then(|submissions| submissions.first())
				.expect("submission must be tracked");

			assert_eq!(submission.tx_hash, tx_hash);
			assert_eq!(submission.request_id, request.id);
			assert!(!watcher.submission_status_futures.contains_key(&(request.id, submission.id)));

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
				0,
				finalized_watch(0),
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
	.expect("runtime version refresh path should not hang");
}

/// AncientBirthBlock should be recoverable:
///  - the retry is signed with the era anchor from the latest-finalized watch (the fresher
///    REFRESHED block), not the stale scan position,
///  - the watcher's tracked finalized block (`self.finalized_block_*`) is NOT mutated — that cursor
///    must only advance through `on_block_finalized`, otherwise its strictly-increasing invariant
///    breaks.
#[tokio::test]
async fn should_recover_from_ancient_birth_block() {
	use sp_runtime::generic::Era;

	const INITIAL_FINALIZED_BLOCK_NUMBER: state_chain_runtime::BlockNumber = 5;
	const REFRESHED_FINALIZED_BLOCK_NUMBER: state_chain_runtime::BlockNumber = 42;
	const NONCE: state_chain_runtime::Nonce = 1;

	tokio::time::timeout(
		Duration::from_secs(5),
		task_scope(|scope| {
			async {
				let mut mock_rpc_api = MockBaseRpcApi::new();

				mock_rpc_api.expect_next_account_nonce().return_once(move |_| Ok(NONCE));
				mock_rpc_api.expect_submit_and_watch_extrinsic().times(1).returning(move |_| {
					Err(ErrorObject::owned(
						1010,
						"Invalid Transaction",
						Some(<&'static str>::from(InvalidTransaction::AncientBirthBlock)),
					)
					.into())
				});

				// On the retry, return a success.
				mock_rpc_api
					.expect_submit_and_watch_extrinsic()
					.return_once(move |_| Ok(Box::pin(stream::empty()) as WatchExtrinsicStream));

				// Scan position stays at the (stale) INITIAL block; the era anchor is sourced from
				// the latest-finalized watch, seeded at the fresher REFRESHED block.
				let (mut watcher, _requests) = SubmissionWatcher::new(
					scope,
					signer::PairSigner::new(sp_core::Pair::generate().0),
					INITIAL_NONCE,
					INITIAL_FINALIZED_BLOCK_NUMBER,
					finalized_watch(REFRESHED_FINALIZED_BLOCK_NUMBER),
					Default::default(),
					H256::default(),
					SIGNED_EXTRINSIC_LIFETIME,
					Arc::new(mock_rpc_api),
				);
				let mut request = new_test_request(0, false);
				watcher.submit_extrinsic(&mut request).await.unwrap();

				// The watcher's tracked finalized block (scan cursor) must NOT have changed — only
				// `on_block_finalized` is allowed to advance it.
				assert_eq!(watcher.finalized_block_number, INITIAL_FINALIZED_BLOCK_NUMBER);

				// The successful retry must have been signed with the era anchor from the
				// watch (REFRESHED block), not the stale scan position. The stored submission
				// lifetime is `..era.death(anchor)`, so we compare against what Era::mortal
				// would produce for the watch's anchor.
				let tracked_submission = watcher
					.submissions_by_nonce
					.get(&NONCE)
					.and_then(|v| v.first())
					.expect("submission should be tracked on successful retry");
				let expected_death = Era::mortal(
					SIGNED_EXTRINSIC_LIFETIME as u64,
					REFRESHED_FINALIZED_BLOCK_NUMBER as u64,
				)
				.death(REFRESHED_FINALIZED_BLOCK_NUMBER as u64)
					as state_chain_runtime::BlockNumber;
				assert_eq!(
					tracked_submission.lifetime,
					..expected_death,
					"retry must be signed with refreshed era anchor"
				);

				// Sanity: the stale-era lifetime would have been a *different* value;
				// otherwise the assertion above is vacuous.
				let stale_death = Era::mortal(
					SIGNED_EXTRINSIC_LIFETIME as u64,
					INITIAL_FINALIZED_BLOCK_NUMBER as u64,
				)
				.death(INITIAL_FINALIZED_BLOCK_NUMBER as u64)
					as state_chain_runtime::BlockNumber;
				assert_ne!(expected_death, stale_death);

				Ok(())
			}
			.boxed()
		}),
	)
	.await
	.expect("ancient birth block recovery path should not hang")
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
#[tokio::test]
async fn should_cleanup_duplicate_submissions_for_same_extrinsic_and_nonce_after_finalization() {
	const NONCE: state_chain_runtime::Nonce = 10;
	const FINALIZED_BLOCK_HASH: H256 = H256([3; 32]);

	task_scope(|scope| {
		async {
			let mut mock_rpc_api = MockBaseRpcApi::new();
			let first_submitted_extrinsic =
				Arc::new(std::sync::Mutex::new(None::<state_chain_runtime::UncheckedExtrinsic>));
			let first_submitted_extrinsic_for_mock = first_submitted_extrinsic.clone();

			mock_rpc_api.expect_submit_and_watch_extrinsic().times(1).return_once(
				move |submitted_extrinsic| {
					*first_submitted_extrinsic_for_mock.lock().unwrap() =
						Some(submitted_extrinsic.clone());
					Ok(Box::pin(stream::pending()) as WatchExtrinsicStream)
				},
			);
			mock_rpc_api.expect_submit_and_watch_extrinsic().times(1).return_once(move |_| {
				Err(ErrorObject::owned(
					POOL_ALREADY_IMPORTED,
					"Already Imported",
					Option::<()>::None,
				)
				.into())
			});

			let mut watcher = new_watcher_with_mock_rpc_api(scope, mock_rpc_api, NONCE, 0).await;
			let mut request = new_test_request(42, false);

			let first_hash = watcher
				.submit_extrinsic_at_nonce(&mut request, NONCE)
				.await?
				.map_err(|_| anyhow::anyhow!("Unexpected submission logic error"))?;
			let second_hash = watcher
				.submit_extrinsic_at_nonce(&mut request, NONCE)
				.await?
				.map_err(|_| anyhow::anyhow!("Unexpected submission logic error"))?;

			assert_ne!(first_hash, second_hash);
			assert_eq!(request.pending_submissions.len(), 2);
			assert_eq!(watcher.submissions_by_nonce.get(&NONCE).map(Vec::len), Some(2));
			assert_eq!(watcher.submission_status_futures.len(), 1);

			let matching_extrinsic = first_submitted_extrinsic
				.lock()
				.unwrap()
				.clone()
				.expect("first submitted extrinsic should be captured");

			let block = state_chain_runtime::SignedBlock {
				block: state_chain_runtime::Block {
					header: state_chain_runtime::Header::new(
						1,
						Default::default(),
						Default::default(),
						Default::default(),
						Default::default(),
					),
					extrinsics: vec![matching_extrinsic],
				},
				justifications: None,
			};
			let events = vec![Box::new(frame_system::EventRecord::<
				state_chain_runtime::RuntimeEvent,
				H256,
			> {
				phase: frame_system::Phase::ApplyExtrinsic(0),
				event: state_chain_runtime::RuntimeEvent::System(
					frame_system::Event::ExtrinsicSuccess {
						dispatch_info: frame_system::DispatchEventInfo {
							weight: Default::default(),
							class: frame_support::dispatch::DispatchClass::Normal,
							pays_fee: frame_support::dispatch::Pays::Yes,
						},
					},
				),
				topics: vec![],
			})];

			let account_info = frame_system::AccountInfo::<
				state_chain_runtime::Nonce,
				<state_chain_runtime::Runtime as frame_system::Config>::AccountData,
			> {
				nonce: NONCE + 1,
				..Default::default()
			};

			let base_rpc_client = Arc::get_mut(&mut watcher.base_rpc_client)
				.expect("watcher should have unique ownership of mock rpc");
			base_rpc_client.expect_block().times(1).return_once(move |hash| {
				assert_eq!(hash, FINALIZED_BLOCK_HASH);
				Ok(Some(block))
			});
			base_rpc_client
				.expect_storage()
				.times(1)
				.return_once(move |_, _| Ok(Some(StorageData(events.encode()))));
			base_rpc_client
				.expect_storage()
				.times(1)
				.return_once(move |_, _| Ok(Some(StorageData(account_info.encode()))));

			let mut requests = BTreeMap::new();
			requests.insert(request.id, request);

			watcher.on_block_finalized(&mut requests, FINALIZED_BLOCK_HASH).await?;

			assert!(requests.is_empty());
			assert!(!watcher.submissions_by_nonce.contains_key(&NONCE));
			assert!(watcher.submission_status_futures.is_empty());

			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap();
}

/// Regression test for the nonce-gap bug: when a `StrictlyOneSubmission`
/// request (the `submit_signed_extrinsic` path, e.g. election votes) is dropped from
/// the pool it is abandoned rather than resubmitted. If the optimistic `best_nonce`2
/// cache were left untouched, the *next* submission would take `best_nonce + 1` and
/// skip the now-vacant nonce, leaving a gap that strands every later submission behind
/// it in the future queue. Dropping a submission must invalidate `best_nonce` so the
/// next submission refreshes from the pool and refills the hole.
#[tokio::test]
async fn dropped_submission_invalidates_nonce_so_next_submission_refills_gap() {
	const GAP_NONCE: state_chain_runtime::Nonce = 10;

	task_scope(|scope| {
		async {
			let mut mock_rpc_api = MockBaseRpcApi::new();

			// The pool reports `GAP_NONCE` as the next account nonce on both refreshes:
			// once for the initial submission, and again on the post-drop refresh — which
			// only happens if the drop invalidated `best_nonce`. `.times(2)` therefore also
			// asserts that the second submission actually refreshed instead of doing `+1`.
			mock_rpc_api
				.expect_next_account_nonce()
				.times(2)
				.returning(move |_| Ok(GAP_NONCE));

			// First submission is dropped by the pool; the second sits in the pool.
			let submit_calls = Arc::new(AtomicU8::new(0));
			mock_rpc_api.expect_submit_and_watch_extrinsic().times(2).returning(move |_| {
				match submit_calls.fetch_add(1, Ordering::Relaxed) {
					0 => Ok(Box::pin(stream::iter(vec![Ok(TransactionStatus::Dropped)]))
						as WatchExtrinsicStream),
					_ => Ok(Box::pin(stream::empty()) as WatchExtrinsicStream),
				}
			});

			let (mut watcher, mut requests) = SubmissionWatcher::new(
				scope,
				signer::PairSigner::new(sp_core::Pair::generate().0),
				GAP_NONCE,
				0,
				finalized_watch(0),
				Default::default(),
				H256::default(),
				SIGNED_EXTRINSIC_LIFETIME,
				Arc::new(mock_rpc_api),
			);

			// First strictly-one submission lands on the gap nonce.
			watcher
				.new_request(
					&mut requests,
					test_call(),
					oneshot::channel().0,
					oneshot::channel().0,
					RequestStrategy::StrictlyOneSubmission(oneshot::channel().0),
				)
				.await?;
			assert_eq!(watcher.best_nonce, Some(GAP_NONCE));
			assert!(watcher.submissions_by_nonce.contains_key(&GAP_NONCE));

			// The pool drops it.
			let submission_details = watcher.watch_for_submission_in_block().await;
			watcher.on_submission_in_block(&mut requests, submission_details).await?;

			// The drop must have vacated the nonce *and* invalidated the optimistic cache.
			assert!(watcher.submissions_by_nonce.is_empty());
			assert_eq!(
				watcher.best_nonce, None,
				"a dropped submission must invalidate the optimistic nonce cache"
			);

			// The next submission must refill the vacated nonce (refresh -> GAP_NONCE),
			// not skip to GAP_NONCE + 1.
			watcher
				.new_request(
					&mut requests,
					test_call(),
					oneshot::channel().0,
					oneshot::channel().0,
					RequestStrategy::StrictlyOneSubmission(oneshot::channel().0),
				)
				.await?;

			assert_eq!(
				watcher.best_nonce,
				Some(GAP_NONCE),
				"next submission must reuse the vacated nonce, not skip past it"
			);
			assert!(
				watcher.submissions_by_nonce.contains_key(&GAP_NONCE),
				"the gap must be refilled"
			);
			assert!(
				!watcher.submissions_by_nonce.contains_key(&(GAP_NONCE + 1)),
				"next submission must not skip the gap"
			);

			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap();
}

/// Create a new watcher and submit a dummy extrinsic.
async fn new_watcher_and_submit_test_extrinsic<'a, 'env>(
	scope: &'a Scope<'env, anyhow::Error>,
	mock_rpc_api: MockBaseRpcApi,
) -> SubmissionWatcher<'a, 'env, MockBaseRpcApi> {
	let mut watcher = new_watcher_with_mock_rpc_api(scope, mock_rpc_api, INITIAL_NONCE, 0).await;
	let mut request = new_test_request(0, false);

	let _ = watcher.submit_extrinsic(&mut request).await;

	watcher
}

// A watch seeded with a finalized block at `number` (sender dropped; `borrow()` still returns it).
fn finalized_watch(number: state_chain_runtime::BlockNumber) -> watch::Receiver<BlockInfo> {
	watch::channel(BlockInfo { parent_hash: H256::default(), hash: H256::default(), number }).1
}

async fn new_watcher_with_mock_rpc_api<'a, 'env>(
	scope: &'a Scope<'env, anyhow::Error>,
	mock_rpc_api: MockBaseRpcApi,
	finalized_nonce: state_chain_runtime::Nonce,
	finalized_block_number: state_chain_runtime::BlockNumber,
) -> SubmissionWatcher<'a, 'env, MockBaseRpcApi> {
	let (watcher, _requests) = SubmissionWatcher::new(
		scope,
		signer::PairSigner::new(sp_core::Pair::generate().0),
		finalized_nonce,
		finalized_block_number,
		finalized_watch(finalized_block_number),
		Default::default(),
		H256::default(),
		SIGNED_EXTRINSIC_LIFETIME,
		Arc::new(mock_rpc_api),
	);

	watcher
}

fn new_test_request(id: RequestID, strictly_one_submission: bool) -> Request {
	Request {
		id,
		next_submission_id: 0,
		pending_submissions: Default::default(),
		strictly_one_submission,
		resubmit_window: ..=1,
		call: test_call(),
		until_in_block_sender: Some(oneshot::channel().0),
		until_finalized_sender: oneshot::channel().0,
	}
}
