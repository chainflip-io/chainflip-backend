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

pub mod x_call_native;
pub mod x_call_token;
pub mod x_swap_native;
pub mod x_swap_token;

/// Some test values and utility functions used within the Vault swap call module.
#[cfg(test)]
pub mod test_utils {
	use crate::{
		cf_parameters::*, eth::Address as EthAddress, CcmChannelMetadataUnchecked,
		ChannelRefundParameters,
	};
	use cf_primitives::{
		chains::Ethereum, AccountId, AffiliateAndFee, AffiliateShortId, Beneficiary, DcaParameters,
		MAX_AFFILIATES,
	};
	use frame_support::pallet_prelude::ConstU32;
	use sp_runtime::BoundedVec;

	pub fn refund_address() -> EthAddress {
		[0xF0; 20].into()
	}
	pub fn dca_parameter() -> DcaParameters {
		DcaParameters { number_of_chunks: 10u32, chunk_interval: 5u32 }
	}
	pub fn affiliate_fees() -> BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>> {
		vec![AffiliateAndFee { affiliate: AffiliateShortId(1u8), fee: 10u8 }]
			.try_into()
			.unwrap()
	}
	pub fn broker_fee() -> Beneficiary<AccountId> {
		Beneficiary { account: AccountId::from([0xF2; 32]), bps: 1u16 }
	}
	pub fn channel_metadata() -> CcmChannelMetadataUnchecked {
		CcmChannelMetadataUnchecked {
			message: vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06].try_into().unwrap(),
			gas_budget: 1_000_000u128,
			ccm_additional_data: vec![0x11, 0x22, 0x33, 0x44].try_into().unwrap(),
		}
	}
	pub const BOOST_FEE: u8 = 100u8;
	pub const BROKER_FEE: u8 = 150u8;

	pub fn dummy_cf_parameter(with_ccm: bool) -> Vec<u8> {
		build_cf_parameters::<Ethereum>(
			ChannelRefundParameters {
				retry_duration: 1u32,
				refund_address: refund_address(),
				min_price: Default::default(),
				refund_ccm_metadata: Default::default(),
			},
			Some(dca_parameter()),
			BOOST_FEE,
			broker_fee().account,
			broker_fee().bps,
			affiliate_fees(),
			with_ccm.then_some(&channel_metadata()),
		)
	}
}
