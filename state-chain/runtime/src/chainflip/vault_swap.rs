use crate::{
	chainflip::{AddressConverter, ChainAddressConverter, SolEnvironment},
	runtime_apis::{DispatchErrorWithMessage, VaultSwapDetails},
	AccountId, BitcoinThresholdSigner, BlockNumber, Environment, EpochKey, Runtime, Swapping,
};

use cf_chains::{
	address::EncodedAddress,
	btc::{
		deposit_address::DepositAddress,
		vault_swap_encoding::{
			encode_swap_params_in_nulldata_payload, SharedCfParameters, UtxoEncodedData,
			MAX_AFFILIATES as BTC_MAX_AFFILIATES,
		},
	},
	ccm_checker::{
		check_ccm_for_blacklisted_accounts, CcmValidityCheck, CcmValidityChecker,
		DecodedCcmAdditionalData,
	},
	sol::{
		api::SolanaEnvironment, instruction_builder::SolanaInstructionBuilder, SolAmount, SolPubkey,
	},
	CcmChannelMetadata, VaultSwapExtraParametersEncoded,
};
use cf_primitives::{
	AffiliateAndFee, Affiliates, Asset, AssetAmount, BasisPoints, DcaParameters, MAX_AFFILIATES,
	SWAP_DELAY_BLOCKS,
};
use cf_traits::{AffiliateRegistry, KeyProvider};

use frame_support::pallet_prelude::{ConstU32, Get};
use scale_info::prelude::string::String;
use sp_runtime::{BoundedVec, DispatchError};
use sp_std::{vec, vec::Vec};

fn to_affiliate_and_fees<MaxAffiliates: Get<u32>>(
	broker_id: AccountId,
	affiliates: Affiliates<AccountId>,
) -> Result<BoundedVec<AffiliateAndFee, MaxAffiliates>, DispatchErrorWithMessage> {
	let affiliates_and_fees = affiliates
		.into_iter()
		.map(|beneficiary| {
			Result::<AffiliateAndFee, DispatchErrorWithMessage>::Ok(AffiliateAndFee {
				affiliate: Swapping::get_short_id(&broker_id, &beneficiary.account)
					.ok_or(pallet_cf_swapping::Error::<Runtime>::AffiliateNotRegistered)?,
				fee: beneficiary
					.bps
					.try_into()
					.map_err(|_| pallet_cf_swapping::Error::<Runtime>::AffiliateFeeTooHigh)?,
			})
		})
		.collect::<Result<Vec<AffiliateAndFee>, _>>()?;

	<BoundedVec<AffiliateAndFee, MaxAffiliates>>::try_from(affiliates_and_fees)
		.map_err(|_| DispatchErrorWithMessage::from("Too many affiliates provided"))
}

pub fn bitcoin_vault_swap(
	broker_id: AccountId,
	destination_asset: Asset,
	destination_address: EncodedAddress,
	broker_commission: BasisPoints,
	min_output_amount: AssetAmount,
	retry_duration: BlockNumber,
	boost_fee: BasisPoints,
	affiliate_fees: Affiliates<AccountId>,
	dca_parameters: Option<DcaParameters>,
) -> Result<VaultSwapDetails<String>, DispatchErrorWithMessage> {
	let private_channel_id =
		pallet_cf_swapping::BrokerPrivateBtcChannels::<Runtime>::get(&broker_id)
			.ok_or(pallet_cf_swapping::Error::<Runtime>::NoPrivateChannelExistsForBroker)?;
	let params = UtxoEncodedData {
		output_asset: destination_asset,
		output_address: destination_address,
		parameters: SharedCfParameters {
			retry_duration: retry_duration
				.try_into()
				.map_err(|_| pallet_cf_swapping::Error::<Runtime>::SwapRequestDurationTooLong)?,
			min_output_amount,
			number_of_chunks: dca_parameters
				.as_ref()
				.map(|params| params.number_of_chunks)
				.unwrap_or(1)
				.try_into()
				.map_err(|_| pallet_cf_swapping::Error::<Runtime>::InvalidDcaParameters)?,
			chunk_interval: dca_parameters
				.as_ref()
				.map(|params| params.chunk_interval)
				.unwrap_or(SWAP_DELAY_BLOCKS)
				.try_into()
				.map_err(|_| pallet_cf_swapping::Error::<Runtime>::InvalidDcaParameters)?,
			boost_fee: boost_fee
				.try_into()
				.map_err(|_| pallet_cf_swapping::Error::<Runtime>::BoostFeeTooHigh)?,
			broker_fee: broker_commission
				.try_into()
				.map_err(|_| pallet_cf_swapping::Error::<Runtime>::BrokerFeeTooHigh)?,
			affiliates: to_affiliate_and_fees::<ConstU32<BTC_MAX_AFFILIATES>>(
				broker_id,
				affiliate_fees,
			)?,
		},
	};

	let EpochKey { key, .. } = BitcoinThresholdSigner::active_epoch_key()
		.expect("We should always have a key for the current epoch.");
	let deposit_address = DepositAddress::new(
		key.current,
		private_channel_id.try_into().map_err(
			// TODO: Ensure this can't happen.
			|_| DispatchErrorWithMessage::from("Private channel id out of bounds."),
		)?,
	)
	.script_pubkey()
	.to_address(&Environment::network_environment().into());

	Ok(VaultSwapDetails::Bitcoin {
		nulldata_payload: encode_swap_params_in_nulldata_payload(params),
		deposit_address,
	})
}

pub fn solana_vault_swap(
	broker_id: AccountId,
	source_asset: Asset,
	destination_asset: Asset,
	destination_address: EncodedAddress,
	broker_commission: BasisPoints,
	extra_parameters: VaultSwapExtraParametersEncoded,
	channel_metadata: Option<CcmChannelMetadata>,
	boost_fee: BasisPoints,
	affiliate_fees: Affiliates<AccountId>,
	dca_parameters: Option<DcaParameters>,
) -> Result<VaultSwapDetails<String>, DispatchErrorWithMessage> {
	// Load up environment variables.
	let api_environment = SolEnvironment::api_environment()
		.map_err(|_| DispatchErrorWithMessage::from("Failed to load Solana API environment"))?;

	let agg_key = SolEnvironment::current_agg_key()
		.map_err(|_| DispatchErrorWithMessage::from("Failed to load Solana Agg key"))?
		.into();

	// Ensure CCM message is valid
	if let Some(ccm) = channel_metadata.as_ref() {
		if let DecodedCcmAdditionalData::Solana(ccm_accounts) =
			CcmValidityChecker::check_and_decode(ccm, destination_asset)
				.map_err(|e| DispatchErrorWithMessage::from(DispatchError::from(e)))?
		{
			// Ensure the CCM parameters do not contain blacklisted accounts.
			check_ccm_for_blacklisted_accounts(
				&ccm_accounts,
				vec![api_environment.token_vault_pda_account.into(), agg_key],
			)
			.map_err(|e| DispatchErrorWithMessage::from(DispatchError::from(e)))
		} else {
			Err(DispatchErrorWithMessage::from("Solana Ccm additional data is invalid"))
		}?;
	}

	let processed_affiliate_fees =
		to_affiliate_and_fees::<ConstU32<MAX_AFFILIATES>>(broker_id.clone(), affiliate_fees)?;

	if let VaultSwapExtraParametersEncoded::Solana {
		from,
		event_data_account,
		input_amount,
		refund_parameters,
		from_token_account,
	} = extra_parameters
	{
		let from = SolPubkey::try_from(from)
			.map_err(|_| DispatchErrorWithMessage::from("Invalid Solana Address: from"))?;
		let refund_parameters = refund_parameters.try_map_address(|addr| {
			ChainAddressConverter::try_from_encoded_address(addr)
				.map_err(|_| "Invalid refund address".into())
		})?;
		let event_data_account = SolPubkey::try_from(event_data_account).map_err(|_| {
			DispatchErrorWithMessage::from("Invalid Solana Address: event_data_account")
		})?;
		let input_amount = SolAmount::try_from(input_amount)
			.map_err(|_| DispatchErrorWithMessage::from("Input amount exceeded MAX"))?;

		Ok(VaultSwapDetails::Solana {
			instruction: match source_asset {
				Asset::Sol => SolanaInstructionBuilder::x_swap_native(
					api_environment,
					agg_key,
					destination_asset,
					destination_address,
					broker_id,
					broker_commission,
					refund_parameters,
					boost_fee,
					processed_affiliate_fees,
					dca_parameters,
					from,
					event_data_account,
					input_amount,
					channel_metadata,
				),
				Asset::SolUsdc => {
					let token_supported_account =
						cf_chains::sol::sol_tx_core::address_derivation::derive_token_supported_account(
							api_environment.vault_program,
							api_environment.usdc_token_mint_pubkey,
						)
						.map_err(|_| DispatchErrorWithMessage::from("Failed to derive supported token account"))?;

					let from_token_account = SolPubkey::try_from(from_token_account.ok_or(
						DispatchErrorWithMessage::from(
							"From token account is required for SolUsdc swaps",
						),
					)?)
					.map_err(|_| {
						DispatchErrorWithMessage::from("Invalid Solana Address: from_token_account")
					})?;

					SolanaInstructionBuilder::x_swap_usdc(
						api_environment,
						destination_asset,
						destination_address,
						broker_id,
						broker_commission,
						refund_parameters,
						boost_fee,
						processed_affiliate_fees,
						dca_parameters,
						from,
						from_token_account,
						event_data_account,
						token_supported_account.address.into(),
						input_amount,
						channel_metadata,
					)
				},
				_ => unreachable!("None-Solana assets should never use this branch of logic"),
			},
		})
	} else {
		Err(DispatchErrorWithMessage::from(
			"Extra parameters provided do not match the input asset",
		))
	}
}
