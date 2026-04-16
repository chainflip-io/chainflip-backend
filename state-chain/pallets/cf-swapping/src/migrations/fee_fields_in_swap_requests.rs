// Copyright 2026 Chainflip Labs GmbH
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

use crate::*;
use frame_support::{traits::UncheckedOnRuntimeUpgrade, weights::Weight};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

#[cfg(feature = "try-runtime")]
#[derive(Encode, Decode)]
struct PreUpgradeData {
	swap_request_count: u64,
	/// Sum of `rate.deconstruct()` (parts-per-million) across all in-flight UserSwap network
	/// fee trackers.
	total_network_fee_ppm: u64,
	/// Sum of broker fee basis points across all beneficiaries of all in-flight UserSwap
	/// requests.
	total_broker_fee_bps: u64,
	/// Total number of broker beneficiary entries across all in-flight UserSwap requests.
	total_beneficiary_count: u64,
}

pub mod old {
	use super::*;
	use cf_primitives::{Beneficiaries, SwapRequestId};
	use frame_support::Twox64Concat;

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct NetworkFeeTracker {
		pub network_fee: FeeRateAndMinimum,
		pub accumulated_stable_amount: AssetAmount,
		pub accumulated_fee: AssetAmount,
	}

	#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub enum FeeType<T: Config> {
		NetworkFee(NetworkFeeTracker),
		BrokerFee(Beneficiaries<T::AccountId>),
	}

	#[expect(clippy::large_enum_variant)]
	#[derive(DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub enum SwapRequestState<T: Config> {
		UserSwap {
			price_limits_and_expiry: Option<PriceLimitsAndExpiry<T::AccountId>>,
			output_action: SwapOutputAction<T::AccountId>,
			dca_state: DcaState,
			fees: Vec<FeeType<T>>,
		},
		NetworkFee,
		IngressEgressFee,
	}

	#[derive(DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T))]
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
		let mut data = PreUpgradeData {
			swap_request_count: 0,
			total_network_fee_ppm: 0,
			total_broker_fee_bps: 0,
			total_beneficiary_count: 0,
		};

		for swap_request in old::SwapRequests::<T>::iter_values() {
			data.swap_request_count += 1;
			if let old::SwapRequestState::UserSwap { fees, .. } = swap_request.state {
				for fee in fees {
					match fee {
						old::FeeType::NetworkFee(tracker) =>
							data.total_network_fee_ppm +=
								tracker.network_fee.rate.deconstruct() as u64,
						old::FeeType::BrokerFee(beneficiaries) => {
							data.total_beneficiary_count += beneficiaries.len() as u64;
							data.total_broker_fee_bps +=
								beneficiaries.iter().map(|b| b.bps as u64).sum::<u64>();
						},
					}
				}
			}
		}

		Ok(data.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let mut translated: u64 = 0;

		crate::SwapRequests::<T>::translate_values::<old::SwapRequest<T>, _>(|old| {
			translated += 1;
			Some(SwapRequest {
				id: old.id,
				input_asset: old.input_asset,
				output_asset: old.output_asset,
				state: match old.state {
					old::SwapRequestState::UserSwap {
						price_limits_and_expiry,
						output_action,
						dca_state,
						fees,
					} => {
						let mut network_fee_tracker = None;
						let mut broker_beneficiaries = None;

						for fee in fees {
							match fee {
								old::FeeType::NetworkFee(old_tracker) => {
									network_fee_tracker = Some(NetworkFeeTracker {
										// Because the minimum network fee changed from USDC to
										// input asset, we will ignore the minimum for
										// these swaps to keep things simple.
										// This may result in some small undercharging of network
										// fees for small swaps.
										network_fee: FeeRateAndMinimum {
											rate: old_tracker.network_fee.rate,
											minimum: 0,
										},
										// Setting the tracking vars to 0 is safe because the
										// minimum is 0 and both are set to 0 at the same time. They
										// will start tracking using the rate with no
										// difference to the events.
										processed_asset_amount: 0,
										accumulated_fee: 0,
									});
								},
								old::FeeType::BrokerFee(beneficiaries) => {
									broker_beneficiaries = Some(beneficiaries);
								},
							}
						}

						SwapRequestState::UserSwap {
							price_limits_and_expiry,
							output_action,
							dca_state,
							network_fee_tracker: network_fee_tracker.unwrap_or_else(|| {
								log_or_panic!(
									"Missing NetworkFee in fees for swap request {:?}",
									old.id
								);
								NetworkFeeTracker::new(Pallet::<T>::get_network_fee_for_swap(
									old.input_asset,
									old.output_asset,
									false, // not internal
								))
							}),
							// Starting a new tracker for broker fees is safe.
							// Brokers have already gotten usdc fees payed out for any already
							// processed chunks. We will just start tracking from 0 and pay
							// out via a swap to usdc for the rest.
							broker_fees_tracker: BrokerFeesTracker::new(
								broker_beneficiaries.unwrap_or_default(),
							),
						}
					},
					old::SwapRequestState::NetworkFee => SwapRequestState::NetworkFee,
					old::SwapRequestState::IngressEgressFee => SwapRequestState::IngressEgressFee,
				},
			})
		});

		log::info!("✅ Migrated {translated} SwapRequests to split fee fields.");

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre = PreUpgradeData::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::Other("Failed to decode pre-upgrade state"))?;

		let mut post_count: u64 = 0;
		let mut total_network_fee_ppm: u64 = 0;
		let mut total_broker_fee_bps: u64 = 0;
		let mut total_beneficiary_count: u64 = 0;

		for swap_request in crate::SwapRequests::<T>::iter_values() {
			post_count += 1;
			if let SwapRequestState::UserSwap { network_fee_tracker, broker_fees_tracker, .. } =
				swap_request.state
			{
				total_network_fee_ppm +=
					network_fee_tracker.network_fee().rate.deconstruct() as u64;
				total_beneficiary_count += broker_fees_tracker.iter().count() as u64;
				total_broker_fee_bps += broker_fees_tracker.sum_fee_bps() as u64;
			}
		}

		frame_support::ensure!(
			pre.swap_request_count == post_count,
			"Post-upgrade: SwapRequests count mismatch"
		);
		frame_support::ensure!(
			pre.total_network_fee_ppm == total_network_fee_ppm,
			"Post-upgrade: total network fee ppm mismatch"
		);
		frame_support::ensure!(
			pre.total_broker_fee_bps == total_broker_fee_bps,
			"Post-upgrade: total broker fee bps mismatch"
		);
		frame_support::ensure!(
			pre.total_beneficiary_count == total_beneficiary_count,
			"Post-upgrade: total beneficiary count mismatch"
		);

		log::info!(
			"✅ Post-upgrade: SwapRequests migration verified. count={}, network_fee_ppm={}, broker_fee_bps={}, beneficiaries={}.",
			post_count,
			total_network_fee_ppm,
			total_broker_fee_bps,
			total_beneficiary_count,
		);

		Ok(())
	}
}
