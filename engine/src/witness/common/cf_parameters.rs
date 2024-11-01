use cf_chains::{CcmAdditionalData, ChannelRefundParameters};
use cf_primitives::{AffiliateShortId, BasisPoints, Beneficiaries, DcaParameters, ShortId};
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
pub type CcmCfParameters = CfParameters<CcmAdditionalData>;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub struct VaultSwapParameters {
	pub refund_params: ChannelRefundParameters,
	pub dca_params: Option<DcaParameters>,
	pub boost_fee: Option<BasisPoints>,
	pub broker_fees: Beneficiaries<AffiliateShortId>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use cf_chains::MAX_CCM_ADDITIONAL_DATA_LENGTH;

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
}
