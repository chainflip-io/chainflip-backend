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

use sp_std::collections::btree_map::BTreeMap;

use crate::{
	swapping::{PriceLimitsAndExpiry, SwapExecutionProgress, SwapOutputAction, SwapRequestType},
	EgressApi, SwapRequestHandler,
};
use cf_chains::{Chain, SwapOrigin};
use cf_primitives::{Asset, AssetAmount, Beneficiaries, DcaParameters, SwapRequestId};
use codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;

use crate::mocks::MockPalletStorage;

use super::MockPallet;

/// Simple mock that applies 1:1 swap ratio to all pairs.
pub struct MockSwapRequestHandler<T>(sp_std::marker::PhantomData<T>);

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct MockSwapRequest {
	pub input_asset: Asset,
	pub output_asset: Asset,
	pub input_amount: AssetAmount,
	pub remaining_input_amount: AssetAmount,
	pub accumulated_output_amount: AssetAmount,
	pub swap_type: SwapRequestType<u64>,
	pub broker_fees: Beneficiaries<u64>,
	pub origin: SwapOrigin<u64>,
	pub price_limits_and_expiry: Option<PriceLimitsAndExpiry<u64>>,
	pub dca_params: Option<DcaParameters>,
}

impl<T> MockPallet for MockSwapRequestHandler<T> {
	const PREFIX: &'static [u8] = b"MockSwapRequestHandler";
}

const SWAP_REQUESTS: &[u8] = b"SWAP_REQUESTS";
const NEXT_REQ_ID: &[u8] = b"NEXT_REQ_ID";

type SwapRequestsStorageType = BTreeMap<SwapRequestId, MockSwapRequest>;

impl<T> MockSwapRequestHandler<T> {
	pub fn get_swap_requests() -> SwapRequestsStorageType {
		Self::get_value::<SwapRequestsStorageType>(SWAP_REQUESTS).unwrap_or_default()
	}

	pub fn set_swap_request_progress(
		swap_request_id: SwapRequestId,
		progress: SwapExecutionProgress,
	) {
		Self::mutate_value(SWAP_REQUESTS, |swaps: &mut Option<SwapRequestsStorageType>| {
			let swaps = swaps.as_mut().expect("must have swap requests");

			let swap = swaps.get_mut(&swap_request_id).expect("must contain requested swap");
			swap.remaining_input_amount = progress.remaining_input_amount;
			swap.accumulated_output_amount = progress.accumulated_output_amount;
		});
	}

	fn get_next_swap_request_id() -> SwapRequestId {
		Self::mutate_value(NEXT_REQ_ID, |maybe_id: &mut Option<SwapRequestId>| {
			let stored_id = maybe_id.get_or_insert_default();
			let to_return = *stored_id;
			*stored_id = (stored_id.0 + 1).into();
			to_return
		})
	}
}

impl<C: Chain, E: EgressApi<C>> SwapRequestHandler for MockSwapRequestHandler<(C, E)>
where
	Asset: TryInto<C::ChainAsset>,
{
	type AccountId = u64;

	fn init_swap_request(
		input_asset: Asset,
		input_amount: AssetAmount,
		output_asset: Asset,
		swap_type: SwapRequestType<Self::AccountId>,
		broker_fees: Beneficiaries<Self::AccountId>,
		price_limits_and_expiry: Option<PriceLimitsAndExpiry<Self::AccountId>>,
		dca_params: Option<DcaParameters>,
		origin: SwapOrigin<Self::AccountId>,
	) -> SwapRequestId {
		let swap_request_id =
			Self::mutate_value(SWAP_REQUESTS, |swaps: &mut Option<SwapRequestsStorageType>| {
				let swaps = swaps.get_or_insert_default();
				let id = Self::get_next_swap_request_id();
				swaps.insert(
					id,
					MockSwapRequest {
						input_asset,
						output_asset,
						input_amount,
						swap_type: swap_type.clone(),
						broker_fees,
						origin,
						remaining_input_amount: input_amount,
						accumulated_output_amount: 0,
						price_limits_and_expiry,
						dca_params,
					},
				);
				id
			});

		match swap_type {
			SwapRequestType::Regular { output_action } => match output_action {
				SwapOutputAction::Egress { ccm_deposit_metadata, output_address } => {
					let _ = E::schedule_egress(
						output_asset.try_into().unwrap_or_else(|_| panic!("Unable to convert")),
						input_amount.try_into().unwrap_or_else(|_| panic!("Unable to convert")),
						output_address.try_into().unwrap_or_else(|_| panic!("Unable to convert")),
						ccm_deposit_metadata,
					);
				},
				SwapOutputAction::CreditOnChain { .. } => {
					// do nothing: this behaviour is tested by the swapping pallet's tests
				},
				SwapOutputAction::CreditLendingPool { .. } => {
					// do nothing: for now it is the test's responsibility to manually call
					// process_loan_swap_outcome where required
				},
				SwapOutputAction::CreditFlipAndTransferToGateway { .. } => {
					// do nothing: this behaviour is tested by the swapping pallet's tests
				},
			},
			_ => { /* do nothing */ },
		};

		swap_request_id
	}

	fn inspect_swap_request(swap_request_id: SwapRequestId) -> Option<SwapExecutionProgress> {
		let swap_requests: SwapRequestsStorageType =
			Self::get_value(SWAP_REQUESTS).unwrap_or_default();

		swap_requests.get(&swap_request_id).map(|swap| SwapExecutionProgress {
			remaining_input_amount: swap.remaining_input_amount,
			accumulated_output_amount: swap.accumulated_output_amount,
		})
	}

	fn abort_swap_request(swap_request_id: SwapRequestId) -> Option<SwapExecutionProgress> {
		Self::mutate_value(SWAP_REQUESTS, |swaps: &mut Option<SwapRequestsStorageType>| {
			let swaps = swaps.as_mut().expect("must have swap requests");

			swaps.remove(&swap_request_id).map(|swap| SwapExecutionProgress {
				remaining_input_amount: swap.remaining_input_amount,
				accumulated_output_amount: swap.accumulated_output_amount,
			})
		})
	}
}
