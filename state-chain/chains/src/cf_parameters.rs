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

use crate::{CcmAdditionalData, CcmChannelMetadataUnchecked, Chain, ChannelRefundParameters};
use cf_primitives::{
	AccountId, AffiliateAndFee, BasisPoints, Beneficiary, DcaParameters, MAX_AFFILIATES,
};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::{BoundedVec, Vec};

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub enum VersionedCfParameters<RefundAddress, CcmData = ()> {
	V0(CfParameters<RefundAddress, CcmData>),
	V1(CfParametersRefundCcm<RefundAddress, CcmData>),
}

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub struct CfParametersGeneric<P, CcmData> {
	/// CCMs may require additional data (e.g. CCMs to Solana requires a list of addresses).
	pub ccm_additional_data: CcmData,
	pub vault_swap_parameters: P,
}

pub type CfParameters<RefundAddress, CcmData = ()> =
	CfParametersGeneric<VaultSwapParametersV0<RefundAddress>, CcmData>;
pub type CfParametersRefundCcm<RefundAddress, CcmData = ()> =
	CfParametersGeneric<VaultSwapParameters<RefundAddress>, CcmData>;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub struct VaultSwapParametersGeneric<R> {
	pub refund_params: R,
	pub dca_params: Option<DcaParameters>,
	pub boost_fee: u8,
	pub broker_fee: Beneficiary<AccountId>,
	pub affiliate_fees: BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>>,
}

pub type VaultSwapParametersV0<RefundAddress> =
	VaultSwapParametersGeneric<ChannelRefundParameters<RefundAddress, ()>>;
pub type VaultSwapParameters<RefundAddress> =
	VaultSwapParametersGeneric<ChannelRefundParameters<RefundAddress>>;

impl<RefundAddress> From<VaultSwapParametersV0<RefundAddress>>
	for VaultSwapParameters<RefundAddress>
{
	fn from(params: VaultSwapParametersV0<RefundAddress>) -> Self {
		VaultSwapParameters {
			refund_params: ChannelRefundParameters {
				retry_duration: params.refund_params.retry_duration,
				refund_address: params.refund_params.refund_address,
				min_price: params.refund_params.min_price,
				refund_ccm_metadata: None,
			},
			dca_params: params.dca_params,
			boost_fee: params.boost_fee,
			broker_fee: params.broker_fee,
			affiliate_fees: params.affiliate_fees,
		}
	}
}

pub type VersionedCcmCfParameters<RefundAddress> =
	VersionedCfParameters<RefundAddress, CcmAdditionalData>;

impl<RefundAddress> CfParametersRefundCcm<RefundAddress, CcmAdditionalData> {
	pub fn with_ccm_data(
		cf_parameters: CfParametersRefundCcm<RefundAddress, ()>,
		data: CcmAdditionalData,
	) -> Self {
		CfParametersRefundCcm {
			ccm_additional_data: data,
			vault_swap_parameters: cf_parameters.vault_swap_parameters,
		}
	}
}

/// Provide a function that builds and encodes `cf_parameters`.
/// The return type is encoded Vec<u8>, which circumvents the difference in return types depending
/// on if CCM data is available.
pub fn build_cf_parameters<C: Chain>(
	refund_parameters: ChannelRefundParameters<C::ChainAccount>,
	dca_parameters: Option<DcaParameters>,
	boost_fee: u8,
	broker_id: AccountId,
	broker_commission: BasisPoints,
	affiliate_fees: BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>>,
	ccm: Option<&CcmChannelMetadataUnchecked>,
) -> Vec<u8> {
	let vault_swap_parameters = VaultSwapParameters {
		refund_params: refund_parameters,
		dca_params: dca_parameters,
		boost_fee,
		broker_fee: Beneficiary { account: broker_id, bps: broker_commission },
		affiliate_fees,
	};

	match ccm {
		Some(ccm) => VersionedCcmCfParameters::V1(CfParametersRefundCcm {
			ccm_additional_data: ccm.ccm_additional_data.clone(),
			vault_swap_parameters,
		})
		.encode(),
		None => VersionedCfParameters::V1(CfParametersRefundCcm {
			ccm_additional_data: (),
			vault_swap_parameters,
		})
		.encode(),
	}
}

pub fn decode_cf_parameters<RefundAddress: Decode, CcmData: Default + Decode>(
	data: &[u8],
) -> Result<(VaultSwapParameters<RefundAddress>, CcmData), &'static str> {
	let VersionedCfParameters::V0(CfParameters { ccm_additional_data, vault_swap_parameters }) =
		VersionedCfParameters::decode(&mut &data[..])
			.map_err(|_| "Failed to decode cf_parameter")?;

	Ok((vault_swap_parameters.into(), ccm_additional_data))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{ForeignChainAddress, MAX_CCM_ADDITIONAL_DATA_LENGTH, MAX_CCM_MSG_LENGTH};
	use cf_primitives::chains::AnyChain;

	const MAX_VAULT_SWAP_PARAMETERS_LENGTH_V0: u32 = 1_000;
	const MAX_CF_PARAM_LENGTH_V0: u32 =
		MAX_CCM_ADDITIONAL_DATA_LENGTH + MAX_VAULT_SWAP_PARAMETERS_LENGTH_V0;

	const MAX_VAULT_SWAP_PARAMETERS_LENGTH_V1: u32 =
		MAX_VAULT_SWAP_PARAMETERS_LENGTH_V0 + MAX_CCM_ADDITIONAL_DATA_LENGTH + MAX_CCM_MSG_LENGTH;
	const MAX_CF_PARAM_LENGTH_V1: u32 =
		MAX_CCM_ADDITIONAL_DATA_LENGTH + MAX_VAULT_SWAP_PARAMETERS_LENGTH_V1;

	// This is without the enum byte nor CcmData
	const REFERENCE_EXPECTED_V0_ENCODED_HEX: &str = "01000000000202020202020202020202020202020202020202000000000000000000000000000000000000000000000000000000000000000000000303030303030303030303030303030303030303030303030303030303030303040000";
	const REFERENCE_EXPECTED_V1_ENCODED_HEX: &str = "0100000000020202020202020202020202020202020202020200000000000000000000000000000000000000000000000000000000000000000000000303030303030303030303030303030303030303030303030303030303030303040000";
	const V0_ENUM_BYTE: u8 = 0;
	const V1_ENUM_BYTE: u8 = 1;
	const ZERO_LENGTH_CCM_ADDITIONAL_DATA: u8 = 0;

	#[test]
	fn test_cf_parameters_max_length_v0() {
		// Pessimistic assumption of some chain with 64 bytes of account data.
		#[derive(Encode, Decode, MaxEncodedLen)]
		struct MaxAccountLength([u8; 64]);
		assert!(
			MAX_VAULT_SWAP_PARAMETERS_LENGTH_V0 as usize >=
				VaultSwapParametersV0::<MaxAccountLength>::max_encoded_len()
		);
		assert!(
			MAX_CF_PARAM_LENGTH_V0 as usize >= CfParameters::<MaxAccountLength>::max_encoded_len()
		);
	}

	#[test]
	fn test_cf_parameters_max_length_v1() {
		// Pessimistic assumption of some chain with 64 bytes of account data.
		#[derive(Encode, Decode, MaxEncodedLen)]
		struct MaxAccountLength([u8; 64]);
		assert!(
			MAX_VAULT_SWAP_PARAMETERS_LENGTH_V1 as usize >=
				VaultSwapParameters::<MaxAccountLength>::max_encoded_len()
		);
		assert!(
			MAX_CF_PARAM_LENGTH_V1 as usize >= CfParameters::<MaxAccountLength>::max_encoded_len()
		);
	}
	fn vault_swap_parameters_v0() -> VaultSwapParametersV0<ForeignChainAddress> {
		VaultSwapParametersV0 {
			refund_params: ChannelRefundParameters {
				retry_duration: 1,
				refund_address: ForeignChainAddress::Eth(sp_core::H160::from([2; 20])),
				min_price: Default::default(),
				refund_ccm_metadata: (),
			},
			dca_params: Some(DcaParameters { number_of_chunks: 1u32, chunk_interval: 3u32 }),
			boost_fee: 100u8,
			broker_fee: Beneficiary { account: AccountId::new([0x00; 32]), bps: 1u16 },
			affiliate_fees: sp_core::bounded_vec![],
		}
	}

	#[test]
	fn test_versioned_cf_parameters() {
		let cf_parameters = CfParameters {
			ccm_additional_data: (),
			vault_swap_parameters: vault_swap_parameters_v0(),
		};
		let no_ccm_v0_encoded = VersionedCfParameters::V0(cf_parameters).encode();

		let expected_encoded: Vec<u8> =
			hex::decode("00010000000002020202020202020202020202020202020202020000000000000000000000000000000000000000000000000000000000000000010100000003000000640000000000000000000000000000000000000000000000000000000000000000010000").unwrap();
		assert_eq!(no_ccm_v0_encoded, expected_encoded);

		let ccm_cf_parameters = CfParameters {
			ccm_additional_data: vec![0xF0, 0xF1, 0xF2, 0xF3].try_into().unwrap(),
			vault_swap_parameters: vault_swap_parameters_v0(),
		};
		let ccm_v0_encoded = VersionedCcmCfParameters::V0(ccm_cf_parameters).encode();
		assert_eq!(ccm_v0_encoded, hex::decode("0010f0f1f2f3010000000002020202020202020202020202020202020202020000000000000000000000000000000000000000000000000000000000000000010100000003000000640000000000000000000000000000000000000000000000000000000000000000010000").unwrap());
	}

	#[test]
	fn can_decode_cf_parameters() {
		let vault_swap_parameters = vault_swap_parameters_v0();

		let encoded = build_cf_parameters::<AnyChain>(
			vault_swap_parameters.refund_params.clone(),
			vault_swap_parameters.dca_params.clone(),
			vault_swap_parameters.boost_fee,
			vault_swap_parameters.broker_fee.account.clone(),
			vault_swap_parameters.broker_fee.bps,
			vault_swap_parameters.affiliate_fees.clone(),
			None,
		);

		assert_eq!(decode_cf_parameters(&encoded[..]), Ok((vault_swap_parameters.clone(), ())));

		let ccm_additional_data = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06];

		let encoded = build_cf_parameters::<AnyChain>(
			vault_swap_parameters.refund_params.clone(),
			vault_swap_parameters.dca_params.clone(),
			vault_swap_parameters.boost_fee,
			vault_swap_parameters.broker_fee.account.clone(),
			vault_swap_parameters.broker_fee.bps,
			vault_swap_parameters.affiliate_fees.clone(),
			Some(&CcmChannelMetadataUnchecked {
				message: Default::default(),
				gas_budget: Default::default(),
				ccm_additional_data: ccm_additional_data.clone().try_into().unwrap(),
			}),
		);

		assert_eq!(
			decode_cf_parameters(&encoded[..]),
			Ok((vault_swap_parameters, ccm_additional_data))
		);
	}
}
