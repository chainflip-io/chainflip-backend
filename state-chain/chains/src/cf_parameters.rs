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
	ccm_checker::DecodedCcmAdditionalData, CcmAdditionalData, CcmChannelMetadataChecked,
	ChannelRefundParameters, ChannelRefundParametersUnchecked,
};
use cf_primitives::{
	AccountId, AffiliateAndFee, BasisPoints, Beneficiary, DcaParameters, MAX_AFFILIATES,
};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::BoundedVec;
use sp_std::prelude::Vec;

/// The default type for CcmData is `()`, which is used for the (default) non-ccm case (ie. regular
/// swaps).
#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub enum VersionedCfParameters<RefundAddress, CcmData = ()> {
	V0(CfParametersV0<RefundAddress, CcmData>),
	V1(CfParametersV1<RefundAddress, CcmData>),
}

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub struct CfParameters<VaultSwapParam, CcmData> {
	/// CCMs may require additional data (e.g. CCMs to Solana requires a list of addresses).
	pub ccm_additional_data: CcmData,
	pub vault_swap_parameters: VaultSwapParam,
}

pub type CfParametersV0<RefundAddress, CcmData = ()> =
	CfParameters<VaultSwapParametersV0<RefundAddress>, CcmData>;
pub type CfParametersV1<RefundAddress, CcmData = ()> =
	CfParameters<VaultSwapParametersV1<RefundAddress>, CcmData>;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub struct VaultSwapParameters<R> {
	pub refund_params: R,
	pub dca_params: Option<DcaParameters>,
	pub boost_fee: u8,
	pub broker_fee: Beneficiary<AccountId>,
	pub affiliate_fees: BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>>,
}

/// Original version of `VaultSwapParameters` that does not include
/// and refund CCM metadata.
pub type VaultSwapParametersV0<RefundAddress> =
	VaultSwapParameters<ChannelRefundParameters<RefundAddress, ()>>;

/// New version of `VaultSwapParameters` that includes refund CCM metadata.
pub type VaultSwapParametersV1<RefundAddress> =
	VaultSwapParameters<ChannelRefundParametersUnchecked<RefundAddress>>;

impl<RefundAddress> From<VaultSwapParametersV0<RefundAddress>>
	for VaultSwapParametersV1<RefundAddress>
{
	fn from(value: VaultSwapParametersV0<RefundAddress>) -> Self {
		VaultSwapParametersV1 {
			refund_params: ChannelRefundParameters {
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
///
/// NOTE: the ccm data can be:
///   - `None` if the CCM data is not required (ie. it's a normal swap with no ccm).
///   - Some(metadata) where the additional data is the variant NotRequired, which means that there
///     *is* a CCM message, but it does not require any additional data (e.g. EVM chains).
///   - Some(metadata) where the additional data is the variant Solana.
pub fn build_and_encode_cf_parameters<RefundAddress: Encode>(
	refund_parameters: ChannelRefundParametersUnchecked<RefundAddress>,
	dca_parameters: Option<DcaParameters>,
	boost_fee: u8,
	broker_id: AccountId,
	broker_commission: BasisPoints,
	affiliate_fees: BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>>,
	ccm: Option<&CcmChannelMetadataChecked>,
) -> Vec<u8> {
	let vault_swap_parameters = VaultSwapParametersV1 {
		refund_params: refund_parameters,
		dca_params: dca_parameters,
		boost_fee,
		broker_fee: Beneficiary { account: broker_id, bps: broker_commission },
		affiliate_fees,
	};

	match ccm.map(|ccm| ccm.ccm_additional_data.clone()) {
		Some(DecodedCcmAdditionalData::Solana(sol_ccm)) =>
			VersionedCfParameters::V1(CfParametersV1 {
				ccm_additional_data: CcmAdditionalData(
					sol_ccm
						.encode()
						.try_into()
						.expect("Checked CCM additional data is guaranteed to be valid."),
				),
				vault_swap_parameters,
			})
			.encode(),
		Some(DecodedCcmAdditionalData::NotRequired) => VersionedCfParameters::V1(CfParametersV1 {
			ccm_additional_data: CcmAdditionalData::default(),
			vault_swap_parameters,
		})
		.encode(),
		None => VersionedCfParameters::V1(CfParametersV1 {
			ccm_additional_data: (),
			vault_swap_parameters,
		})
		.encode(),
	}
}

pub fn decode_cf_parameters<RefundAddress: Decode, CcmData: Default + Decode>(
	data: &[u8],
) -> Result<(VaultSwapParametersV1<RefundAddress>, CcmData), &'static str> {
	VersionedCfParameters::<RefundAddress, CcmData>::decode(&mut &data[..])
		.map(|decoded| match decoded {
			#[allow(deprecated)]
			VersionedCfParameters::V0(CfParametersV0 {
				ccm_additional_data,
				vault_swap_parameters,
			}) => (vault_swap_parameters.into(), ccm_additional_data),
			VersionedCfParameters::V1(CfParametersV1 {
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
		eth,
		sol::{SolAddress, SolCcmAccounts, SolCcmAddress, SolPubkey},
		CcmChannelMetadataChecked, CcmChannelMetadataUnchecked, ForeignChainAddress,
		MAX_CCM_ADDITIONAL_DATA_LENGTH, MAX_CCM_MSG_LENGTH,
	};

	const MAX_VAULT_SWAP_PARAMETERS_LENGTH_V0: u32 = 1_000;
	const MAX_CF_PARAM_LENGTH_V0: u32 =
		MAX_CCM_ADDITIONAL_DATA_LENGTH + MAX_VAULT_SWAP_PARAMETERS_LENGTH_V0;

	const MAX_VAULT_SWAP_PARAMETERS_LENGTH_V1: u32 =
		MAX_VAULT_SWAP_PARAMETERS_LENGTH_V0 + MAX_CCM_ADDITIONAL_DATA_LENGTH + MAX_CCM_MSG_LENGTH;
	const MAX_CF_PARAM_LENGTH_V1: u32 =
		MAX_CCM_ADDITIONAL_DATA_LENGTH + MAX_VAULT_SWAP_PARAMETERS_LENGTH_V1;

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

	fn vault_swap_parameters() -> VaultSwapParametersV1<ForeignChainAddress> {
		VaultSwapParametersV1 {
			refund_params: ChannelRefundParameters {
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
				VaultSwapParametersV0::<MaxAccountLength>::max_encoded_len()
		);
		assert!(
			MAX_CF_PARAM_LENGTH_V0 as usize >=
				CfParametersV0::<MaxAccountLength>::max_encoded_len()
		);
	}

	#[test]
	fn test_cf_parameters_max_length_v1() {
		// Pessimistic assumption of some chain with 64 bytes of account data.
		#[derive(Encode, Decode, MaxEncodedLen)]
		struct MaxAccountLength([u8; 64]);
		assert!(
			MAX_VAULT_SWAP_PARAMETERS_LENGTH_V1 as usize >=
				VaultSwapParametersV1::<MaxAccountLength>::max_encoded_len()
		);
		assert!(
			MAX_CF_PARAM_LENGTH_V1 as usize >=
				CfParametersV1::<MaxAccountLength>::max_encoded_len()
		);
	}

	#[test]
	#[allow(deprecated)]
	fn test_versioned_cf_parameters_v0() {
		let cf_parameters_v0 = CfParametersV0 {
			ccm_additional_data: (),
			vault_swap_parameters: vault_swap_parameters_v0(),
		};
		let no_ccm_v0_encoded = VersionedCfParameters::V0(cf_parameters_v0).encode();

		let expected_encoded: Vec<u8> =
			hex::decode("00010000000002020202020202020202020202020202020202020000000000000000000000000000000000000000000000000000000000000000010100000003000000640000000000000000000000000000000000000000000000000000000000000000010000").unwrap();
		assert_eq!(no_ccm_v0_encoded, expected_encoded);

		let ccm_cf_parameters_v0 = CfParametersV0 {
			ccm_additional_data: CcmAdditionalData(
				vec![0xF0, 0xF1, 0xF2, 0xF3].try_into().unwrap(),
			),
			vault_swap_parameters: vault_swap_parameters_v0(),
		};
		let ccm_v0_encoded = VersionedCfParameters::V0(ccm_cf_parameters_v0).encode();
		assert_eq!(ccm_v0_encoded, hex::decode("0010f0f1f2f3010000000002020202020202020202020202020202020202020000000000000000000000000000000000000000000000000000000000000000010100000003000000640000000000000000000000000000000000000000000000000000000000000000010000").unwrap());
	}

	#[test]
	fn can_decode_vault_swap_no_ccm() {
		let vault_swap_parameters = vault_swap_parameters();

		let encoded = build_and_encode_cf_parameters(
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
	fn can_decode_cf_parameters_no_additional_data() {
		let vault_swap_parameters = vault_swap_parameters();

		let encoded = build_and_encode_cf_parameters(
			vault_swap_parameters.refund_params.clone(),
			vault_swap_parameters.dca_params.clone(),
			vault_swap_parameters.boost_fee,
			vault_swap_parameters.broker_fee.account.clone(),
			vault_swap_parameters.broker_fee.bps,
			vault_swap_parameters.affiliate_fees.clone(),
			Some(&CcmChannelMetadataChecked {
				message: b"SOME_MESSAGE".to_vec().try_into().unwrap(),
				gas_budget: 1_000,
				ccm_additional_data: DecodedCcmAdditionalData::NotRequired,
			}),
		);

		assert_eq!(
			decode_cf_parameters(&encoded[..]),
			Ok((vault_swap_parameters.clone(), CcmAdditionalData::default()))
		);
	}

	#[test]
	fn can_decode_cf_parameters_with_solana_additional_data() {
		let vault_swap_parameters = vault_swap_parameters();

		let ccm_additional_data = VersionedSolanaCcmAdditionalData::V1 {
			ccm_accounts: SolCcmAccounts {
				cf_receiver: SolCcmAddress { pubkey: SolPubkey([0x01; 32]), is_writable: true },
				additional_accounts: vec![],
				fallback_address: SolPubkey([0x02; 32]),
			},
			alts: vec![SolAddress([0x00; 32])],
		};

		let encoded = build_and_encode_cf_parameters(
			vault_swap_parameters.refund_params.clone(),
			vault_swap_parameters.dca_params.clone(),
			vault_swap_parameters.boost_fee,
			vault_swap_parameters.broker_fee.account.clone(),
			vault_swap_parameters.broker_fee.bps,
			vault_swap_parameters.affiliate_fees.clone(),
			Some(&CcmChannelMetadataChecked {
				message: b"SOME_MESSAGE".to_vec().try_into().unwrap(),
				gas_budget: 1_000,
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

	#[test]
	fn can_decode_cf_parameters_with_ccm_evm() {
		use crate::Asset;

		let vault_swap_parameters = vault_swap_parameters();

		let evm_additional_data: CcmChannelMetadataChecked = (CcmChannelMetadataUnchecked {
			message: Default::default(),
			gas_budget: Default::default(),
			ccm_additional_data: Default::default(),
		})
		.to_checked(Asset::Eth, ForeignChainAddress::Eth([2; 20].into()))
		.unwrap();

		let encoded = build_and_encode_cf_parameters(
			vault_swap_parameters.refund_params.clone(),
			vault_swap_parameters.dca_params.clone(),
			vault_swap_parameters.boost_fee,
			vault_swap_parameters.broker_fee.account.clone(),
			vault_swap_parameters.broker_fee.bps,
			vault_swap_parameters.affiliate_fees.clone(),
			Some(&evm_additional_data),
		);

		assert_eq!(
			decode_cf_parameters(&encoded[..]),
			Ok((vault_swap_parameters, DecodedCcmAdditionalData::NotRequired))
		);
	}

	// Add tests to ensure backwards compatibility
	#[test]
	fn can_decode_live_cf_parameters() {
		// Without CCM
		// https://scan.chainflip.io/swaps/582949
		// https://etherscan.io/tx/0xb635d442ed7394fd352ecb854a05ecc92ee135e281009713ca44069499cc6812#eventlog
		let encoded: Vec<u8> =
			hex::decode("0064000000000256E2D1E11B03CDFC4BC0821AA90F4D735A1684B4C7AC477BB6644AFA7FFBF84700000000000000000000000000000000000000000070D0CD75A367987344A3896A18E1510E5429CA5E88357B6C2A2E306B3877380D000000").unwrap();

		// Check that it decodes correctly
		match decode_cf_parameters::<eth::Address, ()>(&encoded[..]) {
			Ok((decoded_vault_swap_parameters, _ccm_additional_data)) => {
				assert_eq!(decoded_vault_swap_parameters.refund_params.retry_duration, 100);
			},
			Err(e) => panic!("Failed to decode cf parameters: {}", e),
		}
	}

	#[test]
	fn can_decode_live_cf_parameters_ccm() {
		let encoded: Vec<u8> =
			hex::decode("000064000000000256E2D1E11B03CDFC4BC0821AA90F4D735A1684B4C7AC477BB6644AFA7FFBF84700000000000000000000000000000000000000000070D0CD75A367987344A3896A18E1510E5429CA5E88357B6C2A2E306B3877380D000000").unwrap();
		match decode_cf_parameters::<eth::Address, DecodedCcmAdditionalData>(&encoded[..]) {
			Ok((decoded_vault_swap_parameters, ccm_additional_data)) => {
				assert_eq!(decoded_vault_swap_parameters.refund_params.retry_duration, 100);
				assert_eq!(ccm_additional_data, DecodedCcmAdditionalData::NotRequired);
			},
			Err(e) => panic!("Failed to decode cf parameters: {}", e),
		}
	}
}
