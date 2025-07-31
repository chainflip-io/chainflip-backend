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

use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::Config;

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub mod old {
	use super::*;
	use cf_primitives::Price;

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum DcaStatus {
		ChunkToBeScheduled,
		ChunkScheduled(SwapId),
		AwaitingRefund,
		Completed,
	}

	impl DcaStatus {
		pub fn scheduled_chunks(&self) -> BTreeSet<SwapId> {
			match self {
				DcaStatus::ChunkToBeScheduled => BTreeSet::new(),
				DcaStatus::ChunkScheduled(chunk) => {
					log::info!("🧜‍♂️ migrating status with a chunk, id {chunk}");
					BTreeSet::from_iter([*chunk])
				},
				DcaStatus::AwaitingRefund | DcaStatus::Completed => BTreeSet::new(),
			}
		}
	}

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub struct DcaState {
		// Replacing this status field with a BTreeSet of scheduled swaps
		pub status: DcaStatus,
		pub remaining_input_amount: AssetAmount,
		pub remaining_chunks: u32,
		pub chunk_interval: u32,
		pub accumulated_output_amount: AssetAmount,
	}

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub struct RefundParametersExtendedGeneric<Address, AccountId> {
		pub retry_duration: cf_primitives::BlockNumber,
		pub refund_destination: AccountOrAddress<AccountId, Address>,
		pub min_price: Price,
		// Migration will also add a refund_ccm_metadata field
	}

	pub type RefundParametersExtended<AccountId> =
		RefundParametersExtendedGeneric<ForeignChainAddress, AccountId>;

	#[allow(clippy::large_enum_variant)]
	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub enum SwapRequestState<T: Config> {
		UserSwap {
			refund_params: Option<RefundParametersExtended<T::AccountId>>,
			output_action: SwapOutputAction<T::AccountId>,
			dca_state: DcaState,
		},
		NetworkFee,
		IngressEgressFee,
	}

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub struct SwapRequest<T: Config> {
		pub id: SwapRequestId,
		pub input_asset: Asset,
		pub output_asset: Asset,
		pub state: SwapRequestState<T>,
	}

	#[frame_support::storage_alias]
	pub type SwapRequests<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, SwapRequestId, SwapRequest<T>>;
}

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let swap_request_count = old::SwapRequests::<T>::iter().count() as u64;
		Ok(swap_request_count.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		crate::SwapRequests::<T>::translate_values::<old::SwapRequest<T>, _>(|old_swap_request| {
			Some(SwapRequest {
				id: old_swap_request.id,
				input_asset: old_swap_request.input_asset,
				output_asset: old_swap_request.output_asset,
				state: match old_swap_request.state {
					old::SwapRequestState::UserSwap {
						refund_params,
						output_action,
						dca_state: old_dca_state,
					} => SwapRequestState::UserSwap {
						refund_params: refund_params.map(|params| {
							cf_chains::ChannelRefundParametersChecked {
								retry_duration: params.retry_duration,
								refund_address: params.refund_destination,
								min_price: params.min_price,
								refund_ccm_metadata: None,
							}
						}),
						output_action,
						dca_state: DcaState {
							scheduled_chunks: old_dca_state.status.scheduled_chunks(),
							remaining_input_amount: old_dca_state.remaining_input_amount,
							remaining_chunks: old_dca_state.remaining_chunks,
							chunk_interval: old_dca_state.chunk_interval,
							accumulated_output_amount: old_dca_state.accumulated_output_amount,
						},
					},
					old::SwapRequestState::NetworkFee => SwapRequestState::NetworkFee,
					old::SwapRequestState::IngressEgressFee => SwapRequestState::IngressEgressFee,
				},
			})
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_swap_request_count = <u64>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_swap_request_count = crate::SwapRequests::<T>::iter().count() as u64;

		assert_eq!(pre_swap_request_count, post_swap_request_count);
		Ok(())
	}
}
