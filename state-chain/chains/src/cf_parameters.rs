use crate::{CcmAdditionalData, ChannelRefundParametersDecoded};
use cf_primitives::{AccountId, AffiliateAndFee, Beneficiary, DcaParameters, MAX_AFFILIATES};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::BoundedVec;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub enum VersionedCfParameters<CcmData = ()> {
	V0(CfParameters<CcmData>),
}

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub struct CfParameters<CcmData = ()> {
	/// CCMs may require additional data (e.g. CCMs to Solana requires a list of addresses).
	pub ccm_additional_data: CcmData,
	pub vault_swap_parameters: VaultSwapParameters,
}

pub type VersionedCcmCfParameters = VersionedCfParameters<CcmAdditionalData>;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub struct VaultSwapParameters {
	pub refund_params: ChannelRefundParametersDecoded,
	pub dca_params: Option<DcaParameters>,
	pub boost_fee: u8,
	pub broker_fee: Beneficiary<AccountId>,
	pub affiliate_fees: BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		ChannelRefundParametersDecoded, ForeignChainAddress, MAX_CCM_ADDITIONAL_DATA_LENGTH,
	};

	const MAX_VAULT_SWAP_PARAMETERS_LENGTH: u32 = 1_000;
	const MAX_CF_PARAM_LENGTH: u32 =
		MAX_CCM_ADDITIONAL_DATA_LENGTH + MAX_VAULT_SWAP_PARAMETERS_LENGTH;

	const REFERENCE_EXPECTED_ENCODED_HEX: &str = "0001000000000202020202020202020202020202020202020202000000000000000000000000000000000000000000000000000000000000000000000303030303030303030303030303030303030303030303030303030303030303040000";

	#[test]
	fn test_cf_parameters_max_length() {
		assert!(
			MAX_VAULT_SWAP_PARAMETERS_LENGTH as usize >= VaultSwapParameters::max_encoded_len()
		);
		assert!(MAX_CF_PARAM_LENGTH as usize >= CfParameters::<()>::max_encoded_len());
		assert!(
			MAX_VAULT_SWAP_PARAMETERS_LENGTH as usize >= VaultSwapParameters::max_encoded_len()
		);
	}

	#[test]
	fn test_versioned_cf_parameters() {
		let vault_swap_parameters = VaultSwapParameters {
			refund_params: ChannelRefundParametersDecoded {
				retry_duration: 1,
				refund_address: ForeignChainAddress::Eth(sp_core::H160::from([2; 20])),
				min_price: Default::default(),
			},
			dca_params: None,
			boost_fee: 0,
			broker_fee: Beneficiary { account: AccountId::new([3; 32]), bps: 4 },
			affiliate_fees: sp_core::bounded_vec![],
		};

		let cf_parameters = CfParameters::<()> {
			ccm_additional_data: (),
			vault_swap_parameters: vault_swap_parameters.clone(),
		};

		let mut encoded = VersionedCfParameters::V0(cf_parameters).encode();
		let expected_encoded: Vec<u8> =
			hex::decode(REFERENCE_EXPECTED_ENCODED_HEX).expect("Decoding hex string failed");
		assert_eq!(encoded, expected_encoded);

		let ccm_cf_parameters = CfParameters {
			ccm_additional_data: CcmAdditionalData::default(),
			vault_swap_parameters,
		};

		encoded = VersionedCcmCfParameters::V0(ccm_cf_parameters).encode();

		// Extra byte for the empty ccm metadata
		let expected_encoded_with_metadata = [vec![0], expected_encoded.clone()].concat();

		assert_eq!(encoded, expected_encoded_with_metadata);
	}
}