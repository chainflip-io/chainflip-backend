//! This file contains a Instruction builder for the Solana chain.
//!
//! The builder is used to build single Solana Instruction used for Vault Swap.
//!
//! Such Instruction can be signed and sent to the Program on Solana directly to invoke
//! certain functions.

use codec::Encode;
use sol_prim::consts::SYSTEM_PROGRAM_ID;
use sp_core::ConstU32;

use crate::{
	address::EncodedAddress,
	cf_parameters::*,
	sol::{
		api::{SolanaEnvironment, SolanaTransactionBuildingError},
		sol_tx_core::program_instructions::swap_endpoints::{
			SwapEndpointProgram, SwapNativeParams,
		},
		SolAddress, SolAmount, SolInstruction, SolPubkey,
	},
	CcmChannelMetadata, ChannelRefundParameters,
};
use cf_primitives::{
	chains::assets::any::Asset, AccountId, AffiliateAndFee, BasisPoints, Beneficiary,
	DcaParameters, MAX_AFFILIATES,
};
use sp_runtime::BoundedVec;
use sp_std::marker::PhantomData;

fn system_program_id() -> SolAddress {
	SYSTEM_PROGRAM_ID
}

pub struct SolanaInstructionBuilder<Environment>(PhantomData<Environment>);

impl<Environment: SolanaEnvironment> SolanaInstructionBuilder<Environment> {
	pub fn x_swap_native(
		destination_asset: Asset,
		destination_address: EncodedAddress,
		broker_id: AccountId,
		broker_commission: BasisPoints,
		refund_parameters: ChannelRefundParameters,
		boost_fee: BasisPoints,
		affiliate_fees: BoundedVec<AffiliateAndFee, ConstU32<MAX_AFFILIATES>>,
		dca_parameters: Option<DcaParameters>,
		from: SolPubkey,
		event_data_account: SolPubkey,
		input_amount: SolAmount,
		ccm: Option<CcmChannelMetadata>,
	) -> Result<SolInstruction, SolanaTransactionBuildingError> {
		let sol_api_environment = Environment::api_environment()?;
		let agg_key = Environment::current_agg_key()?;

		let vault_swap_parameters = VaultSwapParameters {
			refund_params: refund_parameters,
			dca_params: dca_parameters,
			boost_fee: boost_fee.try_into().unwrap_or(u8::MAX),
			broker_fee: Beneficiary { account: broker_id, bps: broker_commission },
			affiliate_fees,
		};

		let cf_parameters = match ccm.as_ref() {
			Some(ccm) => VersionedCcmCfParameters::V0(CfParameters {
				ccm_additional_data: ccm.ccm_additional_data.clone(),
				vault_swap_parameters,
			})
			.encode(),
			None => VersionedCfParameters::V0(CfParameters {
				ccm_additional_data: (),
				vault_swap_parameters,
			})
			.encode(),
		};

		Ok(SwapEndpointProgram::with_id(sol_api_environment.swap_endpoint_program).x_swap_native(
			SwapNativeParams {
				amount: input_amount,
				dst_chain: destination_address.chain() as u32,
				dst_address: destination_address.into_vec(),
				dst_token: destination_asset as u32,
				ccm_parameters: ccm.map(|metadata| metadata.into()),
				cf_parameters,
			},
			sol_api_environment.vault_program_data_account,
			agg_key,
			from,
			event_data_account,
			sol_api_environment.swap_endpoint_program_data_account,
			system_program_id(),
		))
	}
}
