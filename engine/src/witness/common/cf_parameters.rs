use cf_chains::{CcmAdditionalData, ChannelRefundParameters};
use cf_primitives::{AccountId, Affiliates, BasisPoints, Beneficiary, DcaParameters};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

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

// TODO: Define this / implement it on the SC - PRO-1743.
pub type ShortId = u8;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub struct VaultSwapParameters {
	pub refund_params: ChannelRefundParameters,
	pub dca_params: Option<DcaParameters>,
	pub boost_fee: Option<BasisPoints>,
	// TODO: Create BrokerAndFee instead so fee is also a u8?
	pub broker_fees: Beneficiary<AccountId>,
	// TODO: Use AffiliateAndFee in PRO-1751
	pub affiliate_fees: Affiliates<ShortId>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use cf_chains::{ChannelRefundParameters, ForeignChainAddress, MAX_CCM_ADDITIONAL_DATA_LENGTH};

	const MAX_VAULT_SWAP_PARAMETERS_LENGTH: u32 = 1_000;
	const MAX_CF_PARAM_LENGTH: u32 =
		MAX_CCM_ADDITIONAL_DATA_LENGTH + MAX_VAULT_SWAP_PARAMETERS_LENGTH;

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
	fn test_encoding() {
		let vault_swap_parameters = VaultSwapParameters {
			refund_params: ChannelRefundParameters {
				retry_duration: 1,
				refund_address: ForeignChainAddress::Eth(sp_core::H160::from([2; 20])),
				min_price: Default::default(),
			},
			dca_params: None,
			boost_fee: None,
			broker_fees: Beneficiary { account: AccountId::new([3; 32]), bps: 4 },
			affiliate_fees: Default::default(),
		};

		let cf_parameters = CfParameters {
			ccm_additional_data: CcmAdditionalData::default(),
			vault_swap_parameters,
		};

		let encoded = VersionedCfParameters::V0(cf_parameters).encode();

		let expected_encoded: Vec<u8> = vec![
			0, 0, 1, 0, 0, 0, 0, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
			3, 3, 3, 3, 3, 3, 4, 0, 0,
		];

		assert_eq!(encoded, expected_encoded);
	}
}
