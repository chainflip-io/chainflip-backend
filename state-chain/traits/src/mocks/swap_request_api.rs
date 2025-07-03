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

use crate::{
	swapping::{SwapOutputAction, SwapRequestType},
	EgressApi, SwapRequestHandler,
};
use cf_chains::{Chain, ChannelRefundParametersCheckedInternal, SwapOrigin};
use cf_primitives::{Asset, AssetAmount, Beneficiaries, DcaParameters, SwapRequestId};
use codec::{Decode, Encode};
use scale_info::TypeInfo;

use crate::mocks::MockPalletStorage;

use super::MockPallet;

/// Simple mock that applies 1:1 swap ratio to all pairs.
pub struct MockSwapRequestHandler<T>(sp_std::marker::PhantomData<T>);

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockSwapRequest {
	pub input_asset: Asset,
	pub output_asset: Asset,
	pub input_amount: AssetAmount,
	pub swap_type: SwapRequestType<u64>,
	pub broker_fees: Beneficiaries<u64>,
	pub origin: SwapOrigin<u64>,
}

impl<T> MockPallet for MockSwapRequestHandler<T> {
	const PREFIX: &'static [u8] = b"MockSwapRequestHandler";
}

const SWAP_REQUESTS: &[u8] = b"SWAP_REQUESTS";

impl<T> MockSwapRequestHandler<T> {
	pub fn get_swap_requests() -> Vec<MockSwapRequest> {
		Self::get_value(SWAP_REQUESTS).unwrap_or_default()
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
		_refund_params: Option<ChannelRefundParametersCheckedInternal<Self::AccountId>>,
		_dca_params: Option<DcaParameters>,
		origin: SwapOrigin<Self::AccountId>,
	) -> SwapRequestId {
		let id = Self::mutate_value(SWAP_REQUESTS, |swaps: &mut Option<Vec<MockSwapRequest>>| {
			let swaps = swaps.get_or_insert(vec![]);
			let id = swaps.len();
			swaps.push(MockSwapRequest {
				input_asset,
				output_asset,
				input_amount,
				swap_type: swap_type.clone(),
				broker_fees,
				origin,
			});
			id
		});

		match swap_type {
			SwapRequestType::Regular { output_action } => match output_action {
				SwapOutputAction::Egress { ccm_deposit_metadata, output_address } => {
					let _ = E::schedule_egress(
						output_asset.try_into().unwrap_or_else(|_| panic!("Unable to convert")),
						input_amount.try_into().unwrap_or_else(|_| panic!("Unable to convert")),
						output_address.try_into().unwrap_or_else(|_| {
							panic!(
								"Unable to
						convert"
							)
						}),
						ccm_deposit_metadata,
					);
				},
				SwapOutputAction::CreditOnChain { .. } => {
					// do nothing: this behaviour is tested by the swapping pallet's tests
				},
			},
			_ => { /* do nothing */ },
		};

		(id as u64).into()
	}
}
