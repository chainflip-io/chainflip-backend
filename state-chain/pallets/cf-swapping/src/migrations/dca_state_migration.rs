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
	use cf_primitives::{Asset, Beneficiaries};
	use frame_support::Twox64Concat;

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub struct DcaState {
		pub status: DcaStatus,
		pub remaining_input_amount: AssetAmount,
		pub remaining_chunks: u32,
		pub chunk_interval: u32,
		pub accumulated_output_amount: AssetAmount,
		// Migration will add the 2 new fields here
	}

	#[allow(clippy::large_enum_variant)]
	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub enum SwapRequestState<T: Config> {
		UserSwap {
			refund_params: Option<RefundParametersExtended<T::AccountId>>,
			output_action: SwapOutputAction<T::AccountId>,
			dca_state: DcaState,
			broker_fees: Beneficiaries<T::AccountId>,
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
						dca_state,
						broker_fees,
					} => SwapRequestState::UserSwap {
						refund_params,
						output_action,
						dca_state: DcaState {
							status: dca_state.status,
							remaining_input_amount: dca_state.remaining_input_amount,
							remaining_chunks: dca_state.remaining_chunks,
							chunk_interval: dca_state.chunk_interval,
							accumulated_output_amount: dca_state.accumulated_output_amount,
							// Setting the new fields to 0. This will cause the network fee to be
							// calculated as normal for all remaining chunks. It may also
							// cause the minimum network fee to be charged for the next chunk, but
							// that is acceptable.
							network_fee_collected: 0,
							accumulated_input_amount: 0,
						},
						broker_fees,
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
