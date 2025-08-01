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

#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]
#![feature(extract_if)]

mod benchmarking;
mod mock;
mod tests;

pub mod migrations;
pub mod weights;

use cf_chains::{
	address::IntoForeignChainAddress, ApiCall, Chain, ChainCrypto, FeeRefundCalculator,
	RequiresSignatureRefresh, RetryPolicy, TransactionBuilder, TransactionMetadata as _,
};
use cf_primitives::{BroadcastId, ThresholdSignatureRequestId};
use cf_traits::{
	impl_pallet_safe_mode, offence_reporting::OffenceReporter, BroadcastNomination, Broadcaster,
	CfeBroadcastRequest, Chainflip, ElectionEgressWitnesser, EpochInfo, GetBlockHeight,
	RotationBroadcastsPending, ThresholdSigner,
};
use cfe_events::TxBroadcastRequest;
use codec::{Decode, Encode, MaxEncodedLen};
use derive_where::derive_where;
use frame_support::{
	pallet_prelude::{ensure, DispatchResult, RuntimeDebug},
	sp_runtime::{
		traits::{One, Saturating},
		DispatchError,
	},
	traits::{Defensive, Get, OriginTrait, StorageVersion, UnfilteredDispatchable},
	Twox64Concat,
};
use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};
use generic_typeinfo_derive::GenericTypeInfo;
pub use pallet::*;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_set::BTreeSet, marker::PhantomData, prelude::*};
pub use weights::WeightInfo;

type AggKey<T, I> =
	<<<T as pallet::Config<I>>::TargetChain as Chain>::ChainCrypto as ChainCrypto>::AggKey;

impl_pallet_safe_mode! {
	PalletSafeMode<I>;
	retry_enabled,
	egress_witnessing_enabled
}

/// The number of broadcast attempts that were made before this one.
pub type AttemptCount = u32;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletOffence {
	FailedToBroadcastTransaction,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletConfigUpdate {
	BroadcastTimeout { blocks: u32 },
}

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(13);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::{benchmarking_value::BenchmarkValue, instances::PalletInstanceAlias};
	use cf_traits::{AccountRoleRegistry, BroadcastNomination, LiabilityTracker, OnBroadcastReady};
	use frame_support::{
		pallet_prelude::{OptionQuery, *},
		traits::EnsureOrigin,
	};

	/// Type alias for the instance's configured Transaction.
	pub type TransactionFor<T, I> = <<T as Config<I>>::TargetChain as Chain>::Transaction;

	/// Type alias for the instance's configured SignerId.
	pub type SignerIdFor<T, I> = <<T as Config<I>>::TargetChain as Chain>::ChainAccount;

	/// Type alias for the threshold signature
	pub type ThresholdSignatureFor<T, I> =
		<<<T as Config<I>>::TargetChain as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature;

	pub type TransactionOutIdFor<T, I> =
		<<<T as Config<I>>::TargetChain as Chain>::ChainCrypto as ChainCrypto>::TransactionOutId;

	pub type TransactionRefFor<T, I> = <<T as Config<I>>::TargetChain as Chain>::TransactionRef;

	/// Type alias for the instance's configured Payload.
	pub type PayloadFor<T, I> =
		<<<T as Config<I>>::TargetChain as Chain>::ChainCrypto as ChainCrypto>::Payload;

	/// Type alias for the instance's configured transaction Metadata.
	pub type TransactionMetadataFor<T, I> =
		<<T as Config<I>>::TargetChain as Chain>::TransactionMetadata;

	pub type ChainBlockNumberFor<T, I> =
		<<T as Config<I>>::TargetChain as cf_chains::Chain>::ChainBlockNumber;

	/// Type alias for the Amount type of a particular chain.
	pub type ChainAmountFor<T, I> =
		<<T as Config<I>>::TargetChain as cf_chains::Chain>::ChainAmount;

	/// Type alias for the Amount type of a particular chain.
	pub type TransactionFeeFor<T, I> =
		<<T as Config<I>>::TargetChain as cf_chains::Chain>::TransactionFee;

	/// Type alias for the instance's configured ApiCall.
	pub type ApiCallFor<T, I> = <T as Config<I>>::ApiCall;

	/// All data contained in a Broadcast
	#[derive(RuntimeDebug, PartialEq, Eq, Encode, Decode, GenericTypeInfo, CloneNoBound)]
	#[expand_name_with(<T::TargetChain as PalletInstanceAlias>::TYPE_INFO_SUFFIX)]
	pub struct BroadcastData<T: Config<I>, I: 'static> {
		#[skip_name_expansion]
		pub broadcast_id: BroadcastId,
		pub transaction_payload: TransactionFor<T, I>,
		pub threshold_signature_payload: PayloadFor<T, I>,
		pub transaction_out_id: TransactionOutIdFor<T, I>,
		#[skip_name_expansion]
		pub nominee: Option<T::ValidatorId>,
	}

	#[derive(
		RuntimeDebug,
		PartialEqNoBound,
		EqNoBound,
		TypeInfo,
		CloneNoBound,
		Serialize,
		Deserialize,
		Encode,
		Decode,
	)]
	#[derive_where(PartialOrd, Ord;
		TransactionOutIdFor<T, I> : PartialOrd + Ord,
	TransactionFeeFor<T, I> : PartialOrd + Ord,
	TransactionMetadataFor<T, I>: PartialOrd + Ord,
	TransactionRefFor<T, I>: PartialOrd + Ord
	)]
	#[scale_info(skip_type_params(T, I))]
	#[serde(bound(serialize = "", deserialize = ""))]
	pub struct TransactionConfirmation<T: Config<I>, I: 'static> {
		pub tx_out_id: TransactionOutIdFor<T, I>,
		pub signer_id: SignerIdFor<T, I>,
		pub tx_fee: TransactionFeeFor<T, I>,
		pub tx_metadata: TransactionMetadataFor<T, I>,
		pub transaction_ref: TransactionRefFor<T, I>,
	}

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The pallet dispatches calls, so it depends on the runtime's aggregated Call type.
		type RuntimeCall: From<Call<Self, I>> + IsType<<Self as frame_system::Config>::RuntimeCall>;

		/// The top-level origin type of the runtime.
		type RuntimeOrigin: From<Origin<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeOrigin>
			+ Into<Result<Origin<Self, I>, <Self as Config<I>>::RuntimeOrigin>>;

		/// The call type that is used to dispatch a broadcast callback.
		type BroadcastCallable: Member
			+ Parameter
			+ UnfilteredDispatchable<RuntimeOrigin = <Self as frame_system::Config>::RuntimeOrigin>;

		/// Offences that can be reported in this runtime.
		type Offence: From<PalletOffence>;

		/// A marker trait identifying the chain that we are broadcasting to.
		type TargetChain: Chain + PalletInstanceAlias;

		/// The api calls supported by this broadcaster.
		type ApiCall: ApiCall<<<Self as pallet::Config<I>>::TargetChain as Chain>::ChainCrypto>
			+ BenchmarkValue
			+ Send
			+ Sync;

		/// Builds the transaction according to the chain's environment settings.
		type TransactionBuilder: TransactionBuilder<Self::TargetChain, Self::ApiCall>;

		/// A threshold signer that can sign calls for this chain, and dispatch callbacks into this
		/// pallet.
		type ThresholdSigner: ThresholdSigner<
			<Self::TargetChain as Chain>::ChainCrypto,
			Callback = <Self as Config<I>>::RuntimeCall,
		>;

		/// Signer nomination.
		type BroadcastSignerNomination: BroadcastNomination<BroadcasterId = Self::ValidatorId>;

		/// For reporting bad actors.
		type OffenceReporter: OffenceReporter<
			ValidatorId = Self::ValidatorId,
			Offence = Self::Offence,
		>;

		/// Ensure that only threshold signature consensus can trigger a broadcast.
		type EnsureThresholdSigned: EnsureOrigin<<Self as frame_system::Config>::RuntimeOrigin>;

		type BroadcastReadyProvider: OnBroadcastReady<Self::TargetChain, ApiCall = Self::ApiCall>;

		/// Get the latest block height of the target chain via Chain Tracking.
		type ChainTracking: GetBlockHeight<Self::TargetChain>;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode<I>>;

		/// The safe mode block margin
		type SafeModeBlockMargin: Get<BlockNumberFor<Self>>;

		/// The safe mode block margin. During safe mode, the timeout is pushed back
		/// by this number of blocks every time it runs out.
		type SafeModeChainBlockMargin: Get<ChainBlockNumberFor<Self, I>>;

		/// The policy on which decide when we slow down the retry of a broadcast.
		type RetryPolicy: RetryPolicy<
			BlockNumber = BlockNumberFor<Self>,
			AttemptCount = AttemptCount,
		>;

		type ElectionEgressWitnesser: ElectionEgressWitnesser<
			Chain = <Self::TargetChain as Chain>::ChainCrypto,
		>;

		type CfeBroadcastRequest: CfeBroadcastRequest<Self, Self::TargetChain>;

		type LiabilityTracker: LiabilityTracker;

		/// The weights for the pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		pub broadcast_timeout: ChainBlockNumberFor<T, I>,
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			BroadcastTimeout::<T, I>::put(self.broadcast_timeout);
		}
	}

	impl<T: Config<I>, I: 'static> Default for GenesisConfig<T, I> {
		fn default() -> Self {
			Self { broadcast_timeout: Default::default() }
		}
	}

	#[pallet::origin]
	#[derive(PartialEq, Eq, Copy, Clone, RuntimeDebug, Encode, Decode, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T, I))]
	pub struct Origin<T: Config<I>, I: 'static = ()>(pub(super) PhantomData<(T, I)>);

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	/// A counter for incrementing the broadcast id.
	#[pallet::storage]
	pub type BroadcastIdCounter<T, I = ()> = StorageValue<_, BroadcastId, ValueQuery>;

	/// Callbacks to be dispatched when the SignatureAccepted event has been witnessed.
	#[pallet::storage]
	#[pallet::getter(fn request_success_callback)]
	pub type RequestSuccessCallbacks<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, <T as Config<I>>::BroadcastCallable>;

	/// Callbacks to be dispatched when a broadcast failure has been witnessed.
	#[pallet::storage]
	#[pallet::getter(fn request_failed_callback)]
	pub type RequestFailureCallbacks<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, <T as Config<I>>::BroadcastCallable>;

	/// Contains a Set of the authorities that have failed to sign a particular broadcast.
	#[pallet::storage]
	pub type FailedBroadcasters<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, BTreeSet<T::ValidatorId>, ValueQuery>;

	/// Live transaction broadcast requests.
	#[pallet::storage]
	pub type AwaitingBroadcast<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, BroadcastData<T, I>, OptionQuery>;

	/// Lookup table between TransactionOutId -> Broadcast.
	/// This storage item is used by the CFE to track which broadcasts/egresses it needs to
	/// witness.
	#[pallet::storage]
	pub type TransactionOutIdToBroadcastId<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		TransactionOutIdFor<T, I>,
		(BroadcastId, ChainBlockNumberFor<T, I>),
		OptionQuery,
	>;

	/// Maps broadcast id to multiple transaction out ids.
	/// A broadcast id can have multiple transaction out ids if it is refreshed/resigned.
	#[pallet::storage]
	pub type BroadcastIdToTransactionOutIds<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, Vec<TransactionOutIdFor<T, I>>, ValueQuery>;

	/// The list of failed broadcasts that will be retried in some future block.
	#[pallet::storage]
	pub type DelayedBroadcastRetryQueue<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BlockNumberFor<T>, BTreeSet<BroadcastId>, ValueQuery>;

	/// A vector containing broadcast_ids, together with the chain block numbers they time out at.
	#[pallet::storage]
	pub type Timeouts<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<(ChainBlockNumberFor<T, I>, BroadcastId, T::ValidatorId)>, ValueQuery>;

	/// Stores the signed external API Call for a broadcast.
	#[pallet::storage]
	#[pallet::getter(fn threshold_signature_data)]
	pub type PendingApiCalls<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, ApiCallFor<T, I>, OptionQuery>;

	/// Stores metadata related to a transaction.
	#[pallet::storage]
	pub type TransactionMetadata<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, BroadcastId, TransactionMetadataFor<T, I>>;

	/// Whether or not broadcasts are paused for broadcast ids greater than the given broadcast id.
	#[pallet::storage]
	#[pallet::getter(fn broadcast_barriers)]
	pub type BroadcastBarriers<T, I = ()> = StorageValue<_, BTreeSet<BroadcastId>, ValueQuery>;

	/// List of broadcasts that are initiated but not witnessed on the external chain.
	#[pallet::storage]
	#[pallet::getter(fn pending_broadcasts)]
	pub type PendingBroadcasts<T, I = ()> = StorageValue<_, BTreeSet<BroadcastId>, ValueQuery>;

	/// List of broadcasts that have been aborted because they were unsuccessful to be broadcast
	/// after many retries.
	#[pallet::storage]
	#[pallet::getter(fn aborted_broadcasts)]
	pub type AbortedBroadcasts<T, I = ()> = StorageValue<_, BTreeSet<BroadcastId>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn incoming_key_broadcast_id)]
	pub type IncomingKeyAndBroadcastId<T, I = ()> =
		StorageValue<_, (AggKey<T, I>, BroadcastId), OptionQuery>;

	/// We need to store the current Onchain key to know when to resign txs in edge cases around a
	/// rotation. Note that the on chain key is different than the current AggKey stored in
	/// threshold signature pallet. This is because we rotate the AggKey optimistically which means
	/// that the key in threshold signature pallet is rotated as soon as the rotation tx is created,
	/// without waiting for it the tx to actually go through onchain.
	#[pallet::storage]
	#[pallet::getter(fn current_on_chain_key)]
	pub type CurrentOnChainKey<T, I = ()> = StorageValue<_, AggKey<T, I>, OptionQuery>;

	/// The current timeout duration for the broadcast, measured in number of blocks.
	#[pallet::storage]
	pub type BroadcastTimeout<T: Config<I>, I: 'static = ()> =
		StorageValue<_, ChainBlockNumberFor<T, I>, ValueQuery, DefaultBroadcastTimeout<T, I>>;

	const DEFAULT_BROADCAST_TIMEOUT: u32 = 100;

	pub struct DefaultBroadcastTimeout<T, I>(PhantomData<(T, I)>);

	impl<T: Config<I>, I: 'static> Get<ChainBlockNumberFor<T, I>> for DefaultBroadcastTimeout<T, I> {
		fn get() -> ChainBlockNumberFor<T, I> {
			DEFAULT_BROADCAST_TIMEOUT.into()
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// A request to a specific authority to sign a transaction.
		TransactionBroadcastRequest {
			broadcast_id: BroadcastId,
			nominee: T::ValidatorId,
			transaction_payload: TransactionFor<T, I>,
			transaction_out_id: TransactionOutIdFor<T, I>,
		},
		/// A failed broadcast has been scheduled for retry.
		BroadcastRetryScheduled { broadcast_id: BroadcastId, retry_block: BlockNumberFor<T> },
		/// A broadcast has timed out.
		BroadcastTimeout { broadcast_id: BroadcastId },
		/// A broadcast has been aborted after all authorities have failed to broadcast it.
		BroadcastAborted { broadcast_id: BroadcastId },
		/// A broadcast has successfully been completed.
		BroadcastSuccess {
			broadcast_id: BroadcastId,
			transaction_out_id: TransactionOutIdFor<T, I>,
			transaction_ref: TransactionRefFor<T, I>,
		},
		/// A broadcast's threshold signature is invalid, we will attempt to re-sign it.
		ThresholdSignatureInvalid { broadcast_id: BroadcastId },
		/// A signature accepted event on the target chain has been witnessed and the callback was
		/// executed.
		BroadcastCallbackExecuted { broadcast_id: BroadcastId, result: DispatchResult },
		/// The fee paid for broadcasting a transaction has been recorded.
		TransactionFeeDeficitRecorded {
			beneficiary: SignerIdFor<T, I>,
			amount: ChainAmountFor<T, I>,
		},
		/// The fee paid for broadcasting a transaction has been refused.
		TransactionFeeDeficitRefused { beneficiary: SignerIdFor<T, I> },
		/// A Call has been re-threshold-signed, and its signature data is inserted into storage.
		CallResigned { broadcast_id: BroadcastId },
		/// Some pallet configuration has been updated.
		PalletConfigUpdated { update: PalletConfigUpdate },
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The provided payload is invalid.
		InvalidPayload,
		/// The provided broadcast id is invalid.
		InvalidBroadcastId,
		/// A threshold signature was expected but not available.
		ThresholdSignatureUnavailable,
		/// Pending broadcasts cannot be re-signed.
		BroadcastStillPending,
		/// The broadcast's api call is no longer available.
		ApiCallUnavailable,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		/// Process any broadcasts that expired are treated as failed.
		/// Re-try any broadcasts in the Delayed Retry queue.
		/// If safe mode prevents retrying, the broadcasts are added to future blocks.
		fn on_initialize(block_number: BlockNumberFor<T>) -> frame_support::weights::Weight {
			// We treat a time out here as a Broadcast Failure. This is handled the same way - the
			// current broadcaster is reported as Failed to broadcast, and a new broadcaster is
			// nominated. If there are no more broadcaster available, then the broadcast is aborted.

			let current_chain_block = T::ChainTracking::get_block_height();
			let mut expiries = BTreeSet::new();

			// take all broadcasts which have timed out. Since iterators break if we update
			// the map during iteration, we do this in two steps: first collect all expired_keys
			// and separately the expired values, after that iterate again to delete all collected
			// keys from storage.
			Timeouts::<T, I>::mutate(|timeouts| {
				timeouts.retain(|(expiry_block, broadcast_id, nominee)| {
					if *expiry_block <= current_chain_block {
						expiries.insert((*broadcast_id, nominee.clone()));
						false
					} else {
						true
					}
				});
			});

			let pending_broadcasts = PendingBroadcasts::<T, I>::get();
			let mut delayed_retries = DelayedBroadcastRetryQueue::<T, I>::take(block_number);
			let expiry_count = expiries.len();
			let retries_count = delayed_retries.len();
			if T::SafeMode::get().retry_enabled {
				for (broadcast_id, nominee) in expiries {
					if pending_broadcasts.contains(&broadcast_id) {
						Self::deposit_event(Event::<T, I>::BroadcastTimeout { broadcast_id });
						if let Err(e) = Self::handle_broadcast_failure(broadcast_id, nominee) {
							log::warn!("Error when handling broadcast failure: Broadcast ID:{}, Error: {:?}", broadcast_id, e);
						}
					}
				}

				// Retry broadcast (allowed by broadcast barrier)
				let next_block = block_number.saturating_add(One::one());
				let id_limit =
					BroadcastBarriers::<T, I>::get().first().copied().unwrap_or(BroadcastId::MAX);
				delayed_retries.retain(|broadcast_id| {
					if *broadcast_id <= id_limit {
						// If retry is allowed by the barrier - start the retry.
						Self::start_next_broadcast_attempt(*broadcast_id);
						false
					} else {
						true
					}
				});
				if !delayed_retries.is_empty() {
					DelayedBroadcastRetryQueue::<T, I>::mutate(next_block, |current| {
						current.append(&mut delayed_retries)
					});
				}
			} else {
				for (broadcast_id, nominee) in expiries {
					Timeouts::<T, I>::append((
						current_chain_block.saturating_add(T::SafeModeChainBlockMargin::get()),
						broadcast_id,
						nominee,
					))
				}
				// Timeouts::<T, I>::append((
				// 	current_chain_block.saturating_add(T::SafeModeChainBlockMargin::get()),
				// 	expiries,
				// ));
				DelayedBroadcastRetryQueue::<T, I>::mutate(
					block_number.saturating_add(T::SafeModeBlockMargin::get()),
					|current| current.append(&mut delayed_retries),
				);
			}
			T::WeightInfo::on_initialize(expiry_count as u32, retries_count as u32)
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// A callback to be used when a threshold signature request completes. Retrieves the
		/// requested signature, uses the configured [TransactionBuilder] to build the transaction.
		/// Initiates the broadcast sequence if `should_broadcast` is set to true, otherwise insert
		/// the signature result into the `PendingApiCalls` storage.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::on_signature_ready())]
		pub fn on_signature_ready(
			origin: OriginFor<T>,
			threshold_request_id: ThresholdSignatureRequestId,
			threshold_signature_payload: PayloadFor<T, I>,
			api_call: Box<<T as Config<I>>::ApiCall>,
			broadcast_id: BroadcastId,
			initiated_at: ChainBlockNumberFor<T, I>,
			should_broadcast: bool,
		) -> DispatchResult {
			let _ = T::EnsureThresholdSigned::ensure_origin(origin)?;

			let (signer, signature_result) =
				T::ThresholdSigner::signature_result(threshold_request_id);
			let signature = signature_result
				.ready_or_else(|r| {
					log::error!(
						"Signature not found for threshold request {:?}. Request status: {:?}",
						threshold_request_id,
						r
					);
					Error::<T, I>::ThresholdSignatureUnavailable
				})?
				.expect("signature can not be unavailable");

			let signed_api_call = api_call.signed(&signature, signer);

			PendingApiCalls::<T, I>::insert(broadcast_id, signed_api_call.clone());

			// If a signed call already exists, update the storage and do not broadcast.
			if should_broadcast {
				let transaction_out_id = signed_api_call.transaction_out_id();

				T::BroadcastReadyProvider::on_broadcast_ready(&signed_api_call);

				let _ = T::ElectionEgressWitnesser::watch_for_egress_success(
					transaction_out_id.clone(),
				);

				// The Engine uses this.
				TransactionOutIdToBroadcastId::<T, I>::insert(
					&transaction_out_id,
					(broadcast_id, initiated_at),
				);

				BroadcastIdToTransactionOutIds::<T, I>::append(broadcast_id, &transaction_out_id);

				let broadcast_data = BroadcastData::<T, I> {
					broadcast_id,
					transaction_payload: T::TransactionBuilder::build_transaction(&signed_api_call),
					threshold_signature_payload,
					transaction_out_id,
					nominee: None,
				};
				AwaitingBroadcast::<T, I>::insert(broadcast_id, broadcast_data.clone());

				if BroadcastBarriers::<T, I>::get()
					.first()
					.is_some_and(|broadcast_barrier_id| broadcast_id > *broadcast_barrier_id)
				{
					Self::schedule_for_retry(broadcast_id);
				} else {
					Self::start_broadcast_attempt(broadcast_data);
				}
			} else {
				Self::deposit_event(Event::<T, I>::CallResigned { broadcast_id });
			}

			Ok(())
		}

		/// Nodes have witnessed that a signature was accepted on the target chain.
		///
		/// We add to the deficit to later be refunded, and clean up storage related to
		/// this broadcast, reporting any nodes who failed this particular broadcast before
		/// this success.
		#[pallet::weight(T::WeightInfo::transaction_succeeded())]
		#[pallet::call_index(2)]
		pub fn transaction_succeeded(
			origin: OriginFor<T>,
			tx_out_id: TransactionOutIdFor<T, I>,
			signer_id: SignerIdFor<T, I>,
			tx_fee: TransactionFeeFor<T, I>,
			tx_metadata: TransactionMetadataFor<T, I>,
			transaction_ref: TransactionRefFor<T, I>,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin.clone())?;

			Self::egress_success(origin, tx_out_id, signer_id, tx_fee, tx_metadata, transaction_ref)
		}

		#[pallet::weight(T::WeightInfo::stress_test(*how_many))]
		#[pallet::call_index(3)]
		pub fn stress_test(origin: OriginFor<T>, how_many: u32) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			let payload = PayloadFor::<T, I>::decode(&mut &[0xcf; 32][..])
				.map_err(|_| Error::<T, I>::InvalidPayload)?;
			for _ in 0..how_many {
				T::ThresholdSigner::request_signature(payload.clone());
			}

			Ok(())
		}

		/// Submitted by the nominated node to signal that they were unable to broadcast the
		/// transaction.
		#[pallet::call_index(4)]
		#[pallet::weight((T::WeightInfo::transaction_failed(), DispatchClass::Operational))]
		pub fn transaction_failed(
			origin: OriginFor<T>,
			broadcast_id: BroadcastId,
		) -> DispatchResult {
			let reporter = T::AccountRoleRegistry::ensure_validator(origin.clone())?;

			Self::handle_broadcast_failure(broadcast_id, reporter.into())?;
			Ok(())
		}

		/// Re-sign and optionally re-send some broadcast requests.
		/// This is intended for cases where a transaction is valid, but the signature has become
		/// invalid due to a rotation, and so we need to resign the payload with the new key so that
		/// it can be broadcast.
		///
		/// Requires governance origin.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::re_sign_aborted_broadcasts(broadcast_ids.len() as u32))]
		pub fn re_sign_aborted_broadcasts(
			origin: OriginFor<T>,
			broadcast_ids: Vec<BroadcastId>,
			request_broadcast: bool,
			refresh_replay_protection: bool,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			for broadcast_id in broadcast_ids {
				Self::re_sign_broadcast(
					broadcast_id,
					request_broadcast,
					refresh_replay_protection,
				)?;
			}
			Ok(())
		}

		/// [GOVERNANCE] Update a pallet config item.
		///
		/// The dispatch origin of this function must be governance.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::update_pallet_config())]
		pub fn update_pallet_config(
			origin: OriginFor<T>,
			update: PalletConfigUpdate,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			match update {
				PalletConfigUpdate::BroadcastTimeout { blocks } =>
					BroadcastTimeout::<T, I>::set(blocks.into()),
			}

			Self::deposit_event(Event::PalletConfigUpdated { update });

			Ok(())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	pub fn egress_success(
		origin: OriginFor<T>,
		tx_out_id: TransactionOutIdFor<T, I>,
		signer_id: SignerIdFor<T, I>,
		tx_fee: TransactionFeeFor<T, I>,
		tx_metadata: TransactionMetadataFor<T, I>,
		transaction_ref: TransactionRefFor<T, I>,
	) -> DispatchResult {
		let (broadcast_id, _initiated_at) = TransactionOutIdToBroadcastId::<T, I>::take(&tx_out_id)
			.ok_or(Error::<T, I>::InvalidPayload)?;

		Self::remove_pending_broadcast(&broadcast_id);
		AbortedBroadcasts::<T, I>::mutate(|aborted| {
			aborted.remove(&broadcast_id);
		});

		if IncomingKeyAndBroadcastId::<T, I>::exists() {
			let (incoming_key, rotation_broadcast_id) =
				IncomingKeyAndBroadcastId::<T, I>::get().unwrap();
			if rotation_broadcast_id == broadcast_id {
				CurrentOnChainKey::<T, I>::put(incoming_key);
				IncomingKeyAndBroadcastId::<T, I>::kill();
			}
		}

		if let Some(expected_tx_metadata) = TransactionMetadata::<T, I>::take(broadcast_id) {
			if tx_metadata.verify_metadata(&expected_tx_metadata) {
				if let Some(broadcast_data) = AwaitingBroadcast::<T, I>::get(broadcast_id) {
					let to_refund = broadcast_data.transaction_payload.return_fee_refund(tx_fee);

					let address_to_refund = <SignerIdFor<T, I> as IntoForeignChainAddress<
						T::TargetChain,
					>>::into_foreign_chain_address(signer_id.clone());

					use cf_traits::LiabilityTracker;
					T::LiabilityTracker::record_liability(
						address_to_refund,
						<T::TargetChain as Chain>::GAS_ASSET.into(),
						to_refund.into(),
					);

					Self::deposit_event(Event::<T, I>::TransactionFeeDeficitRecorded {
						beneficiary: signer_id,
						amount: to_refund,
					});
				} else {
					log::warn!(
						"Unable to attribute transaction fee refund for broadcast {}.",
						broadcast_id
					);
				}
			} else {
				Self::deposit_event(Event::<T, I>::TransactionFeeDeficitRefused {
					beneficiary: signer_id,
				});
				log::warn!(
				"Transaction metadata verification failed for broadcast {}. Deficit will not be recorded.",
				broadcast_id
			);
			}
		} else {
			log::error!(
				"Transaction metadata not found for broadcast {}. Deficit will be ignored.",
				broadcast_id
			);
		}

		if let Some(callback) = RequestSuccessCallbacks::<T, I>::take(broadcast_id) {
			RequestFailureCallbacks::<T, I>::remove(broadcast_id);
			Self::deposit_event(Event::<T, I>::BroadcastCallbackExecuted {
				broadcast_id,
				result: callback.dispatch_bypass_filter(origin.clone()).map(|_| ()).map_err(|e| {
					log::warn!("Callback execution has failed for broadcast {}.", broadcast_id);
					e.error
				}),
			});
		}

		// Report the people who failed to broadcast this tx during its whole lifetime.
		let failed_broadcasters = FailedBroadcasters::<T, I>::take(broadcast_id);
		if !failed_broadcasters.is_empty() {
			T::OffenceReporter::report_many(
				PalletOffence::FailedToBroadcastTransaction,
				failed_broadcasters,
			);
		}

		Self::clean_up_broadcast_storage(broadcast_id);

		Self::deposit_event(Event::<T, I>::BroadcastSuccess {
			broadcast_id,
			transaction_out_id: tx_out_id,
			transaction_ref,
		});
		Ok(())
	}

	pub fn clean_up_broadcast_storage(broadcast_id: BroadcastId) -> Option<ApiCallFor<T, I>> {
		AwaitingBroadcast::<T, I>::remove(broadcast_id);
		TransactionMetadata::<T, I>::remove(broadcast_id);
		for transaction_out_id in BroadcastIdToTransactionOutIds::<T, I>::take(broadcast_id) {
			TransactionOutIdToBroadcastId::<T, I>::remove(transaction_out_id);
		}
		PendingApiCalls::<T, I>::take(broadcast_id)
	}

	pub fn remove_pending_broadcast(broadcast_id: &BroadcastId) {
		PendingBroadcasts::<T, I>::mutate(|pending_broadcasts| {
			if !pending_broadcasts.remove(broadcast_id) {
				log::warn!("Expected broadcast with id {} to still be pending.", broadcast_id);
			}
			while let Some(broadcast_barrier_id) = BroadcastBarriers::<T, I>::get().first() {
				if pending_broadcasts.first().is_none_or(|id| *id > *broadcast_barrier_id) {
					BroadcastBarriers::<T, I>::mutate(|broadcast_barriers| {
						broadcast_barriers.pop_first();
					});
				} else {
					break
				}
			}
		});
	}

	/// Request a threshold signature, providing [Call::on_signature_ready] as the callback.
	pub fn threshold_sign_and_broadcast(
		api_call: <T as Config<I>>::ApiCall,
		maybe_success_callback: Option<<T as Config<I>>::BroadcastCallable>,
		maybe_failed_callback_generator: impl FnOnce(
			BroadcastId,
		) -> Option<<T as Config<I>>::BroadcastCallable>,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		let broadcast_id = Self::next_broadcast_id();

		PendingBroadcasts::<T, I>::append(broadcast_id);

		if let Some(callback) = maybe_success_callback {
			RequestSuccessCallbacks::<T, I>::insert(broadcast_id, callback);
		}
		if let Some(callback) = maybe_failed_callback_generator(broadcast_id) {
			RequestFailureCallbacks::<T, I>::insert(broadcast_id, callback);
		}

		(broadcast_id, Self::threshold_sign(api_call, broadcast_id, true))
	}

	/// Signs a API call, use `Call::on_signature_ready` as the callback, and returns the signature
	/// request ID.
	fn threshold_sign(
		api_call: <T as Config<I>>::ApiCall,
		broadcast_id: BroadcastId,
		should_broadcast: bool,
	) -> ThresholdSignatureRequestId {
		// We must set this here because after the threshold signature is requested, it's
		// possible that an authority submits the transaction themselves, not going through the
		// standard path. This protects against that, to ensure we always set the earliest possible
		// block number we could have broadcast at, so that we can ensure we witness it.
		let initiated_at = T::ChainTracking::get_block_height();

		let threshold_signature_payload = api_call.threshold_signature_payload();
		T::ThresholdSigner::request_signature_with_callback(
			threshold_signature_payload.clone(),
			|threshold_request_id| {
				Call::on_signature_ready {
					threshold_request_id,
					threshold_signature_payload,
					api_call: Box::new(api_call),
					broadcast_id,
					initiated_at,
					should_broadcast,
				}
				.into()
			},
		)
	}

	fn start_next_broadcast_attempt(broadcast_id: BroadcastId) {
		if !PendingBroadcasts::<T, I>::get().contains(&broadcast_id) {
			log::warn!(
				"Broadcast already succeeded or aborted, retry is ignored. broadcast_id: {}",
				broadcast_id
			);
			return
		}

		if let Some(broadcast_data) = AwaitingBroadcast::<T, I>::get(broadcast_id) {
			// If the broadcast is not pending, we should not retry.
			if let Some(mut api_call) = PendingApiCalls::<T, I>::get(broadcast_id) {
				if let RequiresSignatureRefresh::True(maybe_modified_apicall) =
					T::TransactionBuilder::requires_signature_refresh(
						&api_call,
						&broadcast_data.threshold_signature_payload,
						CurrentOnChainKey::<T, I>::get(),
					) {
					Self::deposit_event(Event::<T, I>::ThresholdSignatureInvalid { broadcast_id });
					if let Some(modified_apicall) = maybe_modified_apicall {
						PendingApiCalls::<T, I>::insert(broadcast_id, modified_apicall.clone());
						api_call = modified_apicall;
					}
					Self::threshold_sign(api_call, broadcast_id, true);
					log::info!(
						"Signature is invalid -> rescheduled threshold signature for broadcast id {}.",
						broadcast_id
					);
				} else {
					Self::start_broadcast_attempt(broadcast_data);
				}
			} else {
				log::error!("No threshold signature data are available for broadcast: {:?}. Retry is aborted.", broadcast_id);
			};
		} else {
			log::error!(
				"Broadcast data not found for broadcast: {:?}. Retry is aborted.",
				broadcast_id
			);
		}
	}

	fn start_broadcast_attempt(mut broadcast_data: BroadcastData<T, I>) {
		let broadcast_id = broadcast_data.broadcast_id;
		T::TransactionBuilder::refresh_unsigned_data(&mut broadcast_data.transaction_payload);
		TransactionMetadata::<T, I>::insert(
			broadcast_id,
			<<T::TargetChain as Chain>::TransactionMetadata>::extract_metadata(
				&broadcast_data.transaction_payload,
			),
		);

		// Pass in the current block number as part of the seed to achieve pseudo-randomness.
		if let Some(nominated_signer) = T::BroadcastSignerNomination::nominate_broadcaster(
			(broadcast_id, frame_system::Pallet::<T>::block_number()),
			FailedBroadcasters::<T, I>::get(broadcast_id),
		) {
			// Overwrite the old entry with updated broadcast data.
			broadcast_data.nominee = Some(nominated_signer.clone());
			AwaitingBroadcast::<T, I>::insert(broadcast_id, broadcast_data.clone());

			Timeouts::<T, I>::append((
				T::ChainTracking::get_block_height() + BroadcastTimeout::<T, I>::get(),
				broadcast_id,
				nominated_signer.clone(),
			));

			T::CfeBroadcastRequest::tx_broadcast_request(TxBroadcastRequest {
				broadcast_id,
				nominee: nominated_signer.clone(),
				payload: broadcast_data.transaction_payload.clone(),
			});

			Self::deposit_event(Event::<T, I>::TransactionBroadcastRequest {
				broadcast_id,
				nominee: nominated_signer,
				transaction_payload: broadcast_data.transaction_payload,
				transaction_out_id: broadcast_data.transaction_out_id,
			});
		} else {
			log::debug!(
				"Failed to nominate a broadcaster, but not all validators have reported failure. Broadcast is scheduled for retry. Broadcast Id: {}",
				broadcast_id
			);
			// Schedule for retry later, when more broadcasters become available.
			Self::schedule_for_retry(broadcast_id);
		}
	}

	fn schedule_for_retry(broadcast_id: BroadcastId) {
		// If no delay, retry in the next block.
		let retry_block = frame_system::Pallet::<T>::block_number().saturating_add(
			T::RetryPolicy::next_attempt_delay(Self::attempt_count(broadcast_id))
				.unwrap_or(One::one()),
		);

		DelayedBroadcastRetryQueue::<T, I>::append(retry_block, broadcast_id);

		Self::deposit_event(Event::<T, I>::BroadcastRetryScheduled { broadcast_id, retry_block });
	}

	// Advance the broadcast ID in storage by 1 and return the result.
	fn next_broadcast_id() -> BroadcastId {
		BroadcastIdCounter::<T, I>::mutate(|id| {
			*id += 1;
			*id
		})
	}

	/// Handles a broadcast failure. The reporter is added to a list of FailedBroadcasters to be
	/// slashed later. If no reporter is given, the Nominated broadcast is used instead.
	/// The broadcast will then be retried.
	fn handle_broadcast_failure(
		broadcast_id: BroadcastId,
		failed_broadcaster: T::ValidatorId,
	) -> DispatchResult {
		ensure!(
			PendingBroadcasts::<T, I>::get().contains(&broadcast_id),
			Error::<T, I>::InvalidBroadcastId
		);

		if let Ok(attempt_count) =
			FailedBroadcasters::<T, I>::try_mutate(broadcast_id, |failed_broadcasters| {
				if failed_broadcasters.insert(failed_broadcaster.clone()) {
					Ok(failed_broadcasters.len())
				} else {
					Err(())
				}
			}) {
			// Abort the broadcast if all validators reported failure, Retry otherwise.
			if attempt_count >= T::EpochInfo::current_authority_count() as usize {
				Self::abort_broadcast(broadcast_id);
			} else {
				Self::schedule_for_retry(broadcast_id);
			}
		} else {
			// Do nothing since this failure has already been reported.
			log::warn!(
				"Broadcast failure by {:?} already recorded for Broadcast ID: {}",
				failed_broadcaster,
				broadcast_id
			);
		}

		Ok(())
	}

	/// Called when all validators have failed to broadcast this call. We abort to prevent infinite
	/// retries. The failed callback is dispatched (if any), and data is kept in storage for
	/// potential future governance functions.
	fn abort_broadcast(broadcast_id: BroadcastId) {
		log::warn!(
			"All authorities failed to broadcast, broadcast is aborted. Broadcast_id {:?}.",
			broadcast_id
		);

		// We want to keep the broadcast details, but we don't need the list of failed
		// broadcasters any more.
		FailedBroadcasters::<T, I>::remove(broadcast_id);

		// Call the failed callback and clean up the callback storage.
		if let Some(callback) = RequestFailureCallbacks::<T, I>::get(broadcast_id) {
			Self::deposit_event(Event::<T, I>::BroadcastCallbackExecuted {
				broadcast_id,
				result: callback.dispatch_bypass_filter(OriginTrait::root()).map(|_| ()).map_err(
					|e| {
						log::error!(
							"Broadcast failure callback execution has failed for broadcast {}.",
							broadcast_id
						);
						e.error
					},
				),
			});
		}

		Self::deposit_event(Event::<T, I>::BroadcastAborted { broadcast_id });
		Self::remove_pending_broadcast(&broadcast_id);
		AbortedBroadcasts::<T, I>::append(broadcast_id);
	}

	pub fn attempt_count(broadcast_id: BroadcastId) -> AttemptCount {
		// NOTE: decode_non_dedup_len is correct here only as long as we *don't* use `append` to
		// insert items.
		FailedBroadcasters::<T, I>::decode_non_dedup_len(broadcast_id).unwrap_or_default() as u32
	}

	/// Returns the ApiCall from a `transaction_out_id`.
	pub fn pending_api_call_from_out_id(
		tx_out_id: TransactionOutIdFor<T, I>,
	) -> Option<(BroadcastId, ApiCallFor<T, I>)> {
		TransactionOutIdToBroadcastId::<T, I>::get(tx_out_id).and_then(|(broadcast_id, _)| {
			PendingApiCalls::<T, I>::get(broadcast_id).map(|api_call| (broadcast_id, api_call))
		})
	}

	pub fn broadcast_success(egress: TransactionConfirmation<T, I>) {
		if let Err(err) = Self::egress_success(
			OriginFor::<T>::none(),
			egress.tx_out_id.clone(),
			egress.signer_id,
			egress.tx_fee,
			egress.tx_metadata,
			egress.transaction_ref,
		) {
			log::error!(
				"Failed to execute egress success: TxOutId: {:?}, Error: {:?}",
				egress.tx_out_id,
				err
			)
		}
	}
}

impl<T: Config<I>, I: 'static> Broadcaster<T::TargetChain> for Pallet<T, I> {
	type ApiCall = T::ApiCall;
	type Callback = <T as Config<I>>::BroadcastCallable;

	fn threshold_sign_and_broadcast(
		api_call: Self::ApiCall,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		Self::threshold_sign_and_broadcast(api_call, None, |_| None)
	}

	fn threshold_sign_and_broadcast_with_callback(
		api_call: Self::ApiCall,
		success_callback: Option<Self::Callback>,
		failed_callback_generator: impl FnOnce(BroadcastId) -> Option<Self::Callback>,
	) -> BroadcastId {
		Self::threshold_sign_and_broadcast(api_call, success_callback, failed_callback_generator).0
	}

	fn threshold_sign(api_call: Self::ApiCall) -> (BroadcastId, ThresholdSignatureRequestId) {
		let broadcast_id = Self::next_broadcast_id();
		(broadcast_id, Self::threshold_sign(api_call, broadcast_id, false))
	}

	fn re_sign_broadcast(
		broadcast_id: BroadcastId,
		request_broadcast: bool,
		refresh_replay_protection: bool,
	) -> Result<ThresholdSignatureRequestId, DispatchError> {
		AbortedBroadcasts::<T, I>::mutate(|aborted| {
			aborted.remove(&broadcast_id);
		});

		let mut api_call =
			PendingApiCalls::<T, I>::get(broadcast_id).ok_or(Error::<T, I>::ApiCallUnavailable)?;

		PendingBroadcasts::<T, I>::try_mutate(|pending| {
			if pending.contains(&broadcast_id) {
				Err(Error::<T, I>::BroadcastStillPending)
			} else {
				if request_broadcast {
					pending.insert(broadcast_id);
				}
				Ok(())
			}
		})?;

		if refresh_replay_protection {
			api_call.refresh_replay_protection();
		}
		Ok(Self::threshold_sign(api_call, broadcast_id, request_broadcast))
	}

	fn expire_broadcast(broadcast_id: BroadcastId) {
		// These would otherwise be cleaned up when the broadcast succeeds or aborts.
		RequestSuccessCallbacks::<T, I>::remove(broadcast_id);
		RequestFailureCallbacks::<T, I>::remove(broadcast_id);
		Self::clean_up_broadcast_storage(broadcast_id);
	}

	fn threshold_sign_and_broadcast_rotation_tx(
		api_call: Self::ApiCall,
		new_key: AggKey<T, I>,
	) -> (BroadcastId, ThresholdSignatureRequestId) {
		let (broadcast_id, request_id) =
			<Self as Broadcaster<_>>::threshold_sign_and_broadcast(api_call);

		if let Some(earliest_pending_broadcast_id) = PendingBroadcasts::<T, I>::get()
			.first()
			.defensive_proof("Broadcast ID was just inserted, so at least this one must exist.")
		{
			for barrier in <<<T as pallet::Config<I>>::TargetChain as Chain>::ChainCrypto as ChainCrypto>::maybe_broadcast_barriers_on_rotation(broadcast_id) {
					if barrier >= *earliest_pending_broadcast_id {
						BroadcastBarriers::<T, I>::append(barrier);
					}
				}
		}

		IncomingKeyAndBroadcastId::<T, I>::put((new_key, broadcast_id));

		(broadcast_id, request_id)
	}
}

impl<T: Config<I>, I: 'static> RotationBroadcastsPending for Pallet<T, I> {
	fn rotation_broadcasts_pending() -> bool {
		IncomingKeyAndBroadcastId::<T, I>::exists()
	}
}
