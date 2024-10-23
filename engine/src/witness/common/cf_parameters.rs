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
pub struct VaultCfParameters {
	pub ccm_additional_data: Option<CcmAdditionalData>,
	pub vault_swap_parameters: VaultSwapParameters,
}

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Debug)]
pub struct VaultSwapParameters {
	pub refund_params: ChannelRefundParameters,
	pub dca_params: Option<DcaParameters>,
	pub boost_fee: Option<BasisPoints>,
	// TODO: Should we make broker mandatory? Should we have a separate field?
	pub broker_fees: Option<BoundedVec<u8, ConstU32<{ cf_primitives::MAX_AFFILIATES + 1 }>>>,
	// pub broker_fees: Option<cf_primitives::Beneficiaries<u8>>,
}

pub trait CfParametersDecode {
	fn decode_into_swap_parameters(self) -> Result<VaultSwapParameters>;
	fn decode_into_ccm_swap_parameters(self) -> Result<VaultCfParameters>;
}

// CfParameters is swap data encoded in Vault Swaps that is to be decoded into the adequate
// parameters to pass to the State Chain along with the contract swap. This applies to EVM
// chains and Solana. BTC has it's own format for VaultSwapParameters and does not
// support initiating CCM swaps via vault swaps.
impl CfParametersDecode for CfParameters {
	fn decode_into_swap_parameters(self) -> Result<VaultSwapParameters> {
		let parameters: VaultSwapParameters = VaultSwapParameters::decode(&mut &self[..])
			.map_err(|_| anyhow!("Failed to decode to `VaultSwapParameters`"))?;
		Ok(parameters)
	}

	fn decode_into_ccm_swap_parameters(self) -> Result<VaultCfParameters> {
		let vault_swap_cf_parameters: VaultCfParameters = VaultCfParameters::decode(&mut &self[..])
			.map_err(|_| anyhow!("Failed to decode to `VaultCfParameters`"))?;
		Ok(vault_swap_cf_parameters)
	}
}
