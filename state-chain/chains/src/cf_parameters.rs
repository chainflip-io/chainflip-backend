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
	ccm_checker::DecodedCcmAdditionalData, CcmAdditionalData, CcmChannelMetadataChecked, Chain,
	ChannelRefundParametersForChain, ChannelRefundParametersGeneric, ChannelRefundParametersLegacy,
};
use cf_primitives::{
	AccountId, AffiliateAndFee, BasisPoints, Beneficiary, DcaParameters, MAX_AFFILIATES,
};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::{BoundedVec, Vec};

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub enum VersionedCfParameters<RefundAddress, CcmData = ()> {
	#[deprecated]
	V0(CfParametersLegacy<RefundAddress, CcmData>),
	V1(CfParametersWithRefundCcm<RefundAddress, CcmData>),
}
pub type VersionedCcmCfParameters<RefundAddress> =
	VersionedCfParameters<RefundAddress, CcmAdditionalData>;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub struct CfParametersGeneric<VaultSwapParam, CcmData> {
	/// CCMs may require additional data (e.g. CCMs to Solana requires a list of addresses).
	pub ccm_additional_data: CcmData,
	pub vault_swap_parameters: VaultSwapParam,
}

pub type CfParametersLegacy<RefundAddress, CcmData = ()> =
	CfParametersGeneric<VaultSwapParametersLegacy<RefundAddress>, CcmData>;
pub type CfParametersWithRefundCcm<RefundAddress, CcmData = ()> =
	CfParametersGeneric<VaultSwapParameters<RefundAddress>, CcmData>;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub struct VaultSwapParametersGeneric<R> {
	pub refund_params: R,
	pub dca_params: Option<DcaParameters>,
	pub boost_fee: u8,
	pub broker_fee: Beneficiary<AccountId>,
	pub affiliate_fees: BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>>,
}
pub type VaultSwapParametersLegacy<RefundAddress> =
	VaultSwapParametersGeneric<ChannelRefundParametersLegacy<RefundAddress>>;
pub type VaultSwapParameters<RefundAddress> =
	VaultSwapParametersGeneric<ChannelRefundParametersGeneric<RefundAddress>>;

impl<RefundAddress> From<VaultSwapParametersLegacy<RefundAddress>>
	for VaultSwapParameters<RefundAddress>
{
	fn from(value: VaultSwapParametersLegacy<RefundAddress>) -> Self {
		VaultSwapParameters {
			refund_params: ChannelRefundParametersGeneric {
				retry_duration: value.refund_params.retry_duration,
				refund_address: value.refund_params.refund_address,
				min_price: value.refund_params.min_price,
				refund_ccm_metadata: None,
			},
			dca_params: value.dca_params,
			boost_fee: value.boost_fee,
			broker_fee: value.broker_fee,
			affiliate_fees: value.affiliate_fees,
		}
	}
}

/// Provide a function that builds and encodes `cf_parameters`.
/// The return type is encoded Vec<u8>, which circumvents the difference in return types depending
/// on if CCM data is available.
pub fn build_cf_parameters<C: Chain>(
	refund_parameters: ChannelRefundParametersForChain<C>,
	dca_parameters: Option<DcaParameters>,
	boost_fee: u8,
	broker_id: AccountId,
	broker_commission: BasisPoints,
	affiliate_fees: BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>>,
	ccm: Option<&CcmChannelMetadataChecked>,
) -> Vec<u8> {
	let vault_swap_parameters = VaultSwapParameters {
		refund_params: refund_parameters,
		dca_params: dca_parameters,
		boost_fee,
		broker_fee: Beneficiary { account: broker_id, bps: broker_commission },
		affiliate_fees,
	};

	match ccm.map(|ccm| ccm.ccm_additional_data.clone()) {
		Some(DecodedCcmAdditionalData::Solana(sol_ccm)) =>
			VersionedCcmCfParameters::V1(CfParametersWithRefundCcm {
				ccm_additional_data: CcmAdditionalData(
					sol_ccm
						.encode()
						.try_into()
						.expect("Checked CCM additional data is guaranteed to be valid."),
				),
				vault_swap_parameters,
			})
			.encode(),
		_ => VersionedCfParameters::V1(CfParametersWithRefundCcm {
			ccm_additional_data: (),
			vault_swap_parameters,
		})
		.encode(),
	}
}

pub fn decode_cf_parameters<RefundAddress: Decode, CcmData: Default + Decode>(
	data: &[u8],
) -> Result<(VaultSwapParameters<RefundAddress>, CcmData), &'static str> {
	VersionedCfParameters::<RefundAddress, CcmData>::decode(&mut &data[..])
		.map(|decoded| match decoded {
			#[allow(deprecated)]
			VersionedCfParameters::V0(CfParametersLegacy {
				ccm_additional_data,
				vault_swap_parameters,
			}) => (
				VaultSwapParameters {
					refund_params: ChannelRefundParametersGeneric {
						retry_duration: vault_swap_parameters.refund_params.retry_duration,
						refund_address: vault_swap_parameters.refund_params.refund_address,
						min_price: vault_swap_parameters.refund_params.min_price,
						refund_ccm_metadata: None,
					},
					dca_params: vault_swap_parameters.dca_params,
					boost_fee: vault_swap_parameters.boost_fee,
					broker_fee: vault_swap_parameters.broker_fee,
					affiliate_fees: vault_swap_parameters.affiliate_fees,
				},
				ccm_additional_data,
			),
			VersionedCfParameters::V1(CfParametersWithRefundCcm {
				ccm_additional_data,
				vault_swap_parameters,
			}) => (vault_swap_parameters, ccm_additional_data),
		})
		.map_err(|_| "Failed to decode cf_parameter")
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		ccm_checker::{DecodedCcmAdditionalData, VersionedSolanaCcmAdditionalData},
		sol::{SolAddress, SolCcmAccounts, SolCcmAddress, SolPubkey},
		CcmChannelMetadataChecked, CcmChannelMetadataUnchecked, ForeignChainAddress,
		MAX_CCM_ADDITIONAL_DATA_LENGTH, MAX_CCM_MSG_LENGTH,
	};
	use cf_primitives::chains::AnyChain;

	const MAX_VAULT_SWAP_PARAMETERS_LENGTH_V0: u32 = 1_000;
	const MAX_CF_PARAM_LENGTH_V0: u32 =
		MAX_CCM_ADDITIONAL_DATA_LENGTH + MAX_VAULT_SWAP_PARAMETERS_LENGTH_V0;

	const MAX_VAULT_SWAP_PARAMETERS_LENGTH_V1: u32 =
		MAX_VAULT_SWAP_PARAMETERS_LENGTH_V0 + MAX_CCM_ADDITIONAL_DATA_LENGTH + MAX_CCM_MSG_LENGTH;
	const MAX_CF_PARAM_LENGTH_V1: u32 =
		MAX_CCM_ADDITIONAL_DATA_LENGTH + MAX_VAULT_SWAP_PARAMETERS_LENGTH_V1;

	fn vault_swap_parameters_v0() -> VaultSwapParametersLegacy<ForeignChainAddress> {
		VaultSwapParametersLegacy {
			refund_params: ChannelRefundParametersLegacy {
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

	fn vault_swap_parameters() -> VaultSwapParameters<ForeignChainAddress> {
		VaultSwapParameters {
			refund_params: ChannelRefundParametersGeneric {
				retry_duration: 1,
				refund_address: ForeignChainAddress::Eth([2; 20].into()),
				min_price: Default::default(),
				refund_ccm_metadata: Some(CcmChannelMetadataUnchecked {
					message: vec![0x01, 0x02, 0x03].try_into().unwrap(),
					gas_budget: 1_000,
					ccm_additional_data: VersionedSolanaCcmAdditionalData::V1 {
						ccm_accounts: SolCcmAccounts {
							cf_receiver: SolCcmAddress {
								pubkey: SolPubkey([0x03; 32]),
								is_writable: true,
							},
							additional_accounts: vec![SolCcmAddress {
								pubkey: SolPubkey([0x04; 32]),
								is_writable: false,
							}],
							fallback_address: SolPubkey([0x05; 32]),
						},
						alts: vec![SolAddress([0x01; 32]), SolAddress([0x02; 32])],
					}
					.encode()
					.try_into()
					.unwrap(),
				}),
			},
			dca_params: Some(DcaParameters { number_of_chunks: 1u32, chunk_interval: 3u32 }),
			boost_fee: 100u8,
			broker_fee: Beneficiary { account: AccountId::new([0x00; 32]), bps: 1u16 },
			affiliate_fees: sp_core::bounded_vec![],
		}
	}

	#[test]
	fn test_cf_parameters_max_length_v0() {
		// Pessimistic assumption of some chain with 64 bytes of account data.
		#[derive(Encode, Decode, MaxEncodedLen)]
		struct MaxAccountLength([u8; 64]);
		assert!(
			MAX_VAULT_SWAP_PARAMETERS_LENGTH_V0 as usize >=
				VaultSwapParametersLegacy::<MaxAccountLength>::max_encoded_len()
		);
		assert!(
			MAX_CF_PARAM_LENGTH_V0 as usize >=
				CfParametersLegacy::<MaxAccountLength>::max_encoded_len()
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
			MAX_CF_PARAM_LENGTH_V1 as usize >=
				CfParametersWithRefundCcm::<MaxAccountLength>::max_encoded_len()
		);
	}

	#[test]
	#[allow(deprecated)]
	fn test_versioned_cf_parameters_v0() {
		let cf_parameters_v0 = CfParametersLegacy {
			ccm_additional_data: (),
			vault_swap_parameters: vault_swap_parameters_v0(),
		};
		let no_ccm_v0_encoded = VersionedCfParameters::V0(cf_parameters_v0).encode();

		let expected_encoded: Vec<u8> =
			hex::decode("00010000000002020202020202020202020202020202020202020000000000000000000000000000000000000000000000000000000000000000010100000003000000640000000000000000000000000000000000000000000000000000000000000000010000").unwrap();
		assert_eq!(no_ccm_v0_encoded, expected_encoded);

		let ccm_cf_parameters_v0 = CfParametersLegacy {
			ccm_additional_data: vec![0xF0, 0xF1, 0xF2, 0xF3].try_into().unwrap(),
			vault_swap_parameters: vault_swap_parameters_v0(),
		};
		let ccm_v0_encoded = VersionedCcmCfParameters::V0(ccm_cf_parameters_v0).encode();
		assert_eq!(ccm_v0_encoded, hex::decode("0010f0f1f2f3010000000002020202020202020202020202020202020202020000000000000000000000000000000000000000000000000000000000000000010100000003000000640000000000000000000000000000000000000000000000000000000000000000010000").unwrap());
	}

	#[test]
	fn can_decode_cf_parameters_no_ccm() {
		let vault_swap_parameters = vault_swap_parameters();

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
	}

	#[test]
	fn can_decode_cf_parameters_with_ccm() {
		let vault_swap_parameters = vault_swap_parameters();

		let ccm_additional_data = VersionedSolanaCcmAdditionalData::V1 {
			ccm_accounts: SolCcmAccounts {
				cf_receiver: SolCcmAddress { pubkey: SolPubkey([0x01; 32]), is_writable: true },
				additional_accounts: vec![],
				fallback_address: SolPubkey([0x02; 32]),
			},
			alts: vec![SolAddress([0x00; 32])],
		};

		let encoded = build_cf_parameters::<AnyChain>(
			vault_swap_parameters.refund_params.clone(),
			vault_swap_parameters.dca_params.clone(),
			vault_swap_parameters.boost_fee,
			vault_swap_parameters.broker_fee.account.clone(),
			vault_swap_parameters.broker_fee.bps,
			vault_swap_parameters.affiliate_fees.clone(),
			Some(&CcmChannelMetadataChecked {
				message: Default::default(),
				gas_budget: Default::default(),
				ccm_additional_data: DecodedCcmAdditionalData::Solana(ccm_additional_data.clone()),
			}),
		);

		assert_eq!(
			decode_cf_parameters(&encoded[..]),
			Ok((
				vault_swap_parameters,
				CcmAdditionalData(ccm_additional_data.encode().try_into().unwrap())
			))
		);
	}
}
