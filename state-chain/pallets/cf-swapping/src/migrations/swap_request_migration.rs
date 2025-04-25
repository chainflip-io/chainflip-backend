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

struct FeeMigrationData {
	pub accumulated_output_amount: AssetAmount,
	pub network_fee_collected: AssetAmount,
	pub is_internal: bool,
}
impl FeeMigrationData {
	fn new_for_fee_swap() -> Self {
		Self {
			accumulated_output_amount: AssetAmount::zero(),
			network_fee_collected: AssetAmount::zero(),
			is_internal: false,
		}
	}
}

impl NetworkFeeTracker {
	fn migrate<T: Config>(enforce_minimum: bool, migration_data: &FeeMigrationData) -> Self {
		let network_fee = if migration_data.is_internal {
			InternalSwapNetworkFee::<T>::get()
		} else {
			NetworkFee::<T>::get()
		};
		let test = Self {
			network_fee: FeeRateAndMinimum {
				minimum: if enforce_minimum { network_fee.minimum } else { AssetAmount::zero() },
				rate: network_fee.rate,
			},
			accumulated_stable_amount: migration_data.accumulated_output_amount,
			accumulated_fee: migration_data.network_fee_collected,
		};
		log::info!("	Migrated network fee: {:?}", test);
		test
	}
}

pub mod old {
	use super::*;
	use cf_primitives::{Asset, Beneficiaries};
	use frame_support::Twox64Concat;

	#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum FeeType<T: Config> {
		// Changing this NetworkFee a new struct
		NetworkFee { min_fee_enforced: bool },
		BrokerFee(Beneficiaries<T::AccountId>),
	}

	#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct Swap<T: Config> {
		pub swap_id: SwapId,
		pub swap_request_id: SwapRequestId,
		pub from: Asset,
		pub to: Asset,
		pub input_amount: AssetAmount,
		pub fees: Vec<FeeType<T>>,
		pub refund_params: Option<SwapRefundParameters>,
	}

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub struct DcaState {
		pub status: DcaStatus,
		pub remaining_input_amount: AssetAmount,
		pub remaining_chunks: u32,
		pub chunk_interval: u32,
		pub accumulated_output_amount: AssetAmount,
		// Moving these 2 fields to the swaps FeeType struct
		pub network_fee_collected: AssetAmount,
		pub accumulated_stable_amount: AssetAmount,
	}

	#[allow(clippy::large_enum_variant)]
	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub enum SwapRequestState<T: Config> {
		UserSwap {
			refund_params: Option<RefundParametersExtended<T::AccountId>>,
			output_action: SwapOutputAction<T::AccountId>,
			dca_state: DcaState,
			// Removing this field
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

	#[frame_support::storage_alias]
	pub type SwapQueue<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, BlockNumberFor<T>, Vec<Swap<T>>, ValueQuery>;
}

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let swap_request_count = old::SwapRequests::<T>::iter().count() as u64;
		Ok(swap_request_count.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let mut fee_migration_data: BTreeMap<SwapRequestId, FeeMigrationData> = BTreeMap::new();

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
						// Removing this field
						broker_fees: _broker_fees,
					} => {
						// Grab the tracking data, also need to know if the swap is internal or not
						fee_migration_data.insert(
							old_swap_request.id,
							FeeMigrationData {
								accumulated_output_amount: dca_state.accumulated_output_amount,
								network_fee_collected: dca_state.network_fee_collected,
								is_internal: matches!(
									output_action,
									SwapOutputAction::CreditOnChain { .. }
								),
							},
						);
						SwapRequestState::UserSwap {
							refund_params,
							output_action,
							dca_state: DcaState {
								status: dca_state.status,
								remaining_input_amount: dca_state.remaining_input_amount,
								remaining_chunks: dca_state.remaining_chunks,
								chunk_interval: dca_state.chunk_interval,
								accumulated_output_amount: dca_state.accumulated_output_amount,
							},
						}
					},
					old::SwapRequestState::NetworkFee => {
						fee_migration_data
							.insert(old_swap_request.id, FeeMigrationData::new_for_fee_swap());
						SwapRequestState::NetworkFee
					},
					old::SwapRequestState::IngressEgressFee => {
						fee_migration_data
							.insert(old_swap_request.id, FeeMigrationData::new_for_fee_swap());
						SwapRequestState::IngressEgressFee
					},
				},
			})
		});

		crate::SwapQueue::<T>::translate_values::<Vec<old::Swap<T>>, _>(|old_swaps| {
			old_swaps
				.into_iter()
				.map(|old_swap| {
					let tracking_data = fee_migration_data
						.get(&old_swap.swap_request_id)
						.expect("Tracking data should exist");
					let fees = old_swap
						.fees
						.iter()
						.map(|fee| match fee {
							old::FeeType::NetworkFee { min_fee_enforced } => FeeType::NetworkFee(
								NetworkFeeTracker::migrate::<T>(*min_fee_enforced, tracking_data),
							),
							old::FeeType::BrokerFee(beneficiaries) =>
								FeeType::BrokerFee(beneficiaries.clone()),
						})
						.collect();

					Some(Swap {
						swap_id: old_swap.swap_id,
						swap_request_id: old_swap.swap_request_id,
						from: old_swap.from,
						to: old_swap.to,
						input_amount: old_swap.input_amount,
						fees,
						refund_params: old_swap.refund_params,
					})
				})
				.collect()
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
