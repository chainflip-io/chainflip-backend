use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::ConstU32;

use anyhow::{anyhow, Result};
use cf_chains::{CcmAdditionalData, ChannelRefundParameters, MAX_CCM_ADDITIONAL_DATA_LENGTH};
use cf_primitives::{BasisPoints, DcaParameters};
use frame_support::sp_runtime::BoundedVec;

pub const MAX_VAULT_SWAP_PARAMETERS_LENGTH: u32 = 1_000;
pub const MAX_CF_PARAM_LENGTH: u32 =
	MAX_CCM_ADDITIONAL_DATA_LENGTH + MAX_VAULT_SWAP_PARAMETERS_LENGTH;
pub type CfParameters = BoundedVec<u8, ConstU32<MAX_CF_PARAM_LENGTH>>;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub struct VaultSwapCfParameters {
	pub ccm_additional_data: Option<CcmAdditionalData>,
	pub vault_swap_parameters: Option<VaultSwapParameters>,
}

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub struct VaultSwapParameters {
	pub refund_params: Option<ChannelRefundParameters>,
	pub dca_params: Option<DcaParameters>,
	pub boost_fee: Option<BasisPoints>,
}

pub trait CfParametersDecode {
	fn decode_vault_swap_parameters(self) -> Result<VaultSwapParameters>;
	fn decode_vault_swap_cf_parameters(self) -> Result<(CcmAdditionalData, VaultSwapParameters)>;
}

// CfParameters is swap data encoded in Vault Swaps that is to be decoded into the adequate
// parameters to pass to the State Chain along with the contract swap. This applies to EVM
// chains and Solana. BTC has it's own format for VaultSwapParameters and does not
// support initiating CCM swaps via vault swaps.
impl CfParametersDecode for CfParameters {
	fn decode_vault_swap_parameters(self) -> Result<VaultSwapParameters> {
		if self.is_empty() {
			Ok(VaultSwapParameters { refund_params: None, dca_params: None, boost_fee: None })
		} else {
			let parameters: VaultSwapParameters = VaultSwapParameters::decode(&mut &self[..])
				.map_err(|_| anyhow!("Failed to decode to `VaultSwapParameters`"))?;
			Ok(parameters)
		}
	}

	fn decode_vault_swap_cf_parameters(self) -> Result<(CcmAdditionalData, VaultSwapParameters)> {
		if self.is_empty() {
			// Return the empty vector since the CCM additional data is required
			Ok((
				CcmAdditionalData::default(),
				VaultSwapParameters { refund_params: None, dca_params: None, boost_fee: None },
			))
		} else {
			let vault_swap_cf_parameters: VaultSwapCfParameters =
				VaultSwapCfParameters::decode(&mut &self[..])
					.map_err(|_| anyhow!("Failed to decode to `VaultSwapCfParameters`"))?;

			Ok((
				// Default to empty CcmAdditionalData vector if not present
				vault_swap_cf_parameters.ccm_additional_data.unwrap_or_default(),
				vault_swap_cf_parameters.vault_swap_parameters.unwrap_or(VaultSwapParameters {
					refund_params: None,
					dca_params: None,
					boost_fee: None,
				}),
			))
		}
	}
}
