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
	chainflip::{
		address_derivation::btc::derive_btc_vault_deposit_addresses, AddressConverter,
		ChainAddressConverter, EvmEnvironment, SolEnvironment,
	},
	runtime_apis::{DispatchErrorWithMessage, EvmVaultSwapDetails, VaultSwapDetails},
	AccountId, BlockNumber, Environment, Runtime, Swapping,
};

use cf_chains::{
	address::EncodedAddress,
	btc::vault_swap_encoding::{
		encode_swap_params_in_nulldata_payload, BtcCfParameters, UtxoEncodedData,
	},
	ccm_checker::{check_ccm_for_blacklisted_accounts, DecodedCcmAdditionalData},
	cf_parameters::build_and_encode_cf_parameters,
	evm::api::{EvmCall, EvmEnvironmentProvider},
	sol::{
		api::SolanaEnvironment, instruction_builder::SolanaInstructionBuilder,
		sol_tx_core::address_derivation::derive_associated_token_account, DecodedXSwapParams,
		SolAmount, SolInstruction, SolPubkey,
	},
	Arbitrum, CcmChannelMetadataChecked, CcmChannelMetadataUnchecked,
	ChannelRefundParametersUncheckedEncoded, Ethereum, ForeignChain, VaultSwapExtraParameters,
	VaultSwapInputEncoded,
};
use cf_primitives::{
	AffiliateAndFee, Affiliates, Asset, AssetAmount, BasisPoints, Beneficiary, DcaParameters,
	SWAP_DELAY_BLOCKS,
};
use cf_traits::{AffiliateRegistry, SwapParameterValidation};
use codec::Decode;
use scale_info::prelude::string::String;

use frame_support::pallet_prelude::DispatchError;
use sp_core::U256;
use sp_std::{vec, vec::Vec};

pub fn to_affiliate_and_fees(
	broker_id: &AccountId,
	affiliates: Affiliates<AccountId>,
) -> Result<Vec<AffiliateAndFee>, DispatchErrorWithMessage> {
	let mapping = <Swapping as AffiliateRegistry>::reverse_mapping(broker_id);
	affiliates
		.into_iter()
		.map(|beneficiary| {
			Ok(AffiliateAndFee {
				affiliate: *mapping
					.get(&beneficiary.account)
					.ok_or(pallet_cf_swapping::Error::<Runtime>::AffiliateNotRegisteredForBroker)?,
				fee: beneficiary
					.bps
					.try_into()
					.map_err(|_| pallet_cf_swapping::Error::<Runtime>::AffiliateFeeTooHigh)?,
			})
		})
		.collect::<Result<Vec<AffiliateAndFee>, _>>()
}

fn from_affiliate_and_fees(
	broker_id: &AccountId,
	affiliates_and_fees: Vec<AffiliateAndFee>,
) -> Result<Affiliates<AccountId>, DispatchErrorWithMessage> {
	affiliates_and_fees
		.into_iter()
		.map(|affiliate_and_fee| {
			Ok(Beneficiary {
				account: pallet_cf_swapping::AffiliateIdMapping::<Runtime>::get(
					broker_id,
					affiliate_and_fee.affiliate,
				)
				.ok_or(pallet_cf_swapping::Error::<Runtime>::AffiliateNotRegisteredForBroker)?,
				bps: affiliate_and_fee.fee as BasisPoints,
			})
		})
		.collect::<Result<Vec<Beneficiary<AccountId>>, DispatchErrorWithMessage>>()?
		.try_into()
		.map_err(|_| "Too many affiliates".into())
}

pub fn bitcoin_vault_swap(
	broker_id: AccountId,
	destination_asset: Asset,
	destination_address: EncodedAddress,
	broker_commission: BasisPoints,
	min_output_amount: AssetAmount,
	retry_duration: BlockNumber,
	boost_fee: u8,
	affiliate_fees: Affiliates<AccountId>,
	dca_parameters: Option<DcaParameters>,
) -> Result<VaultSwapDetails<String>, DispatchErrorWithMessage> {
	let private_channel_id =
		pallet_cf_swapping::BrokerPrivateBtcChannels::<Runtime>::get(&broker_id)
			.ok_or(pallet_cf_swapping::Error::<Runtime>::NoPrivateChannelExistsForBroker)?;
	let params = UtxoEncodedData {
		output_asset: destination_asset,
		output_address: destination_address,
		parameters: BtcCfParameters {
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
			boost_fee,
			broker_fee: broker_commission
				.try_into()
				.map_err(|_| pallet_cf_swapping::Error::<Runtime>::BrokerFeeTooHigh)?,
			affiliates: to_affiliate_and_fees(&broker_id, affiliate_fees)?
				.try_into()
				.map_err(|_| "Too many affiliates.")?,
		},
	};

	Ok(VaultSwapDetails::Bitcoin {
		nulldata_payload: encode_swap_params_in_nulldata_payload(params),
		deposit_address: derive_btc_vault_deposit_addresses(private_channel_id).current_address(),
	})
}

pub fn evm_vault_swap<A>(
	broker_id: AccountId,
	source_asset: Asset,
	amount: AssetAmount,
	destination_asset: Asset,
	destination_address: EncodedAddress,
	broker_commission: BasisPoints,
	refund_params: ChannelRefundParametersUncheckedEncoded,
	boost_fee: u8,
	affiliate_fees: Affiliates<AccountId>,
	dca_parameters: Option<DcaParameters>,
	channel_metadata: Option<CcmChannelMetadataChecked>,
) -> Result<VaultSwapDetails<A>, DispatchErrorWithMessage> {
	let refund_params = refund_params.try_map_address(|addr| match addr {
		EncodedAddress::Eth(inner) | EncodedAddress::Arb(inner) => Ok(inner),
		_ => Err(DispatchErrorWithMessage::from("Refund address must be an EVM address")),
	})?;
	let processed_affiliate_fees = to_affiliate_and_fees(&broker_id, affiliate_fees)?
		.try_into()
		.map_err(|_| "Too many affiliates.")?;

	let cf_parameters = match ForeignChain::from(source_asset) {
		ForeignChain::Ethereum | ForeignChain::Arbitrum => build_and_encode_cf_parameters(
			refund_params,
			dca_parameters,
			boost_fee,
			broker_id,
			broker_commission,
			processed_affiliate_fees,
			channel_metadata.as_ref(),
		),
		_ => Err(DispatchErrorWithMessage::from("Unsupported source chain for EVM vault swap"))?,
	};

	let mut source_token_address = None;
	let calldata = match source_asset {
		Asset::Eth | Asset::ArbEth =>
			if let Some(ccm) = channel_metadata {
				Ok(cf_chains::evm::api::x_call_native::XCallNative::new(
					destination_address,
					destination_asset,
					ccm.message.to_vec(),
					ccm.gas_budget,
					cf_parameters,
				)
				.abi_encoded_payload())
			} else {
				Ok(cf_chains::evm::api::x_swap_native::XSwapNative::new(
					destination_address,
					destination_asset,
					cf_parameters,
				)
				.abi_encoded_payload())
			},
		Asset::Flip | Asset::Usdc | Asset::Usdt | Asset::ArbUsdc => {
			// Lookup Token addresses depending on the Chain
			let source_token_address_ref = source_token_address.insert(
				match source_asset {
					Asset::Flip | Asset::Usdc | Asset::Usdt =>
						<EvmEnvironment as EvmEnvironmentProvider<Ethereum>>::token_address(
							source_asset.try_into().expect("Only Ethereum asset is processed here"),
						),
					Asset::ArbUsdc =>
						<EvmEnvironment as EvmEnvironmentProvider<Arbitrum>>::token_address(
							cf_chains::assets::arb::Asset::ArbUsdc,
						),
					_ => unreachable!("Unreachable for non-Ethereum/Arbitrum assets"),
				}
				.ok_or(DispatchErrorWithMessage::from("Failed to look up EVM token address"))?,
			);

			if let Some(ccm) = channel_metadata {
				Ok(cf_chains::evm::api::x_call_token::XCallToken::new(
					destination_address,
					destination_asset,
					ccm.message.to_vec(),
					ccm.gas_budget,
					*source_token_address_ref,
					amount,
					cf_parameters,
				)
				.abi_encoded_payload())
			} else {
				Ok(cf_chains::evm::api::x_swap_token::XSwapToken::new(
					destination_address,
					destination_asset,
					*source_token_address_ref,
					amount,
					cf_parameters,
				)
				.abi_encoded_payload())
			}
		},
		_ => Err(DispatchErrorWithMessage::from(
			"Only EVM chains should execute this branch of logic. This error should never happen",
		)),
	}?;

	match source_asset.into() {
		ForeignChain::Ethereum => Ok(VaultSwapDetails::ethereum(EvmVaultSwapDetails {
			calldata,
			// Only return `amount` for native currently. 0 for Tokens
			value: (source_asset == Asset::Eth).then_some(U256::from(amount)).unwrap_or_default(),
			to: Environment::eth_vault_address(),
			source_token_address,
		})),
		ForeignChain::Arbitrum => Ok(VaultSwapDetails::arbitrum(EvmVaultSwapDetails {
			calldata,
			// Only return `amount` for native currently. 0 for Tokens
			value: (source_asset == Asset::ArbEth)
				.then_some(U256::from(amount))
				.unwrap_or_default(),
			to: Environment::arb_vault_address(),
			source_token_address,
		})),
		_ => Err(DispatchErrorWithMessage::from(
			"Only EVM chains should execute this branch of logic. This error should never happen",
		)),
	}
}

pub fn solana_vault_swap<A>(
	broker_id: AccountId,
	input_amount: AssetAmount,
	source_asset: Asset,
	destination_asset: Asset,
	destination_address: EncodedAddress,
	broker_commission: BasisPoints,
	refund_parameters: ChannelRefundParametersUncheckedEncoded,
	channel_metadata: Option<CcmChannelMetadataChecked>,
	boost_fee: u8,
	affiliate_fees: Affiliates<AccountId>,
	dca_parameters: Option<DcaParameters>,
	from: EncodedAddress,
	seed: cf_chains::sol::SolSeed,
	from_token_account: Option<EncodedAddress>,
) -> Result<VaultSwapDetails<A>, DispatchErrorWithMessage> {
	// Load up environment variables.
	let api_environment =
		SolEnvironment::api_environment().map_err(|_| "Failed to load Solana API environment")?;

	let swap_endpoint_native_vault =
		cf_chains::sol::sol_tx_core::address_derivation::derive_swap_endpoint_native_vault_account(
			api_environment.swap_endpoint_program,
		)
		.map_err(|_| "Failed to derive swap_endpoint_native_vault")?
		.address;

	let processed_affiliate_fees = to_affiliate_and_fees(&broker_id, affiliate_fees)?
		.try_into()
		.map_err(|_| "Too many affiliates")?;

	let from = SolPubkey::try_from(from).map_err(|_| "Invalid Solana Address: from")?;
	let refund_parameters = refund_parameters.try_map_address(|addr| match addr {
		EncodedAddress::Sol(inner) => Ok(inner.into()),
		_ => Err(DispatchErrorWithMessage::from("Invalid refund address.")),
	})?;
	let event_data_account =
		cf_chains::sol::sol_tx_core::address_derivation::derive_vault_swap_account(
			api_environment.swap_endpoint_program,
			from.into(),
			&seed,
		)
		.map_err(|_| "Failed to derive swap_endpoint_native_vault")?
		.address
		.into();
	let input_amount =
		SolAmount::try_from(input_amount).map_err(|_| "Input amount exceeded MAX")?;
	let cf_parameters = build_and_encode_cf_parameters::<sol_prim::Address>(
		refund_parameters,
		dca_parameters,
		boost_fee,
		broker_id,
		broker_commission,
		processed_affiliate_fees,
		channel_metadata.as_ref(),
	);

	Ok(VaultSwapDetails::Solana {
		instruction: match source_asset {
			Asset::Sol => SolanaInstructionBuilder::x_swap_native(
				api_environment,
				swap_endpoint_native_vault.into(),
				destination_asset,
				destination_address,
				from,
				seed,
				event_data_account,
				input_amount,
				cf_parameters,
				channel_metadata,
			),
			Asset::SolUsdc => {
				let token_supported_account =
						cf_chains::sol::sol_tx_core::address_derivation::derive_token_supported_account(
							api_environment.vault_program,
							api_environment.usdc_token_mint_pubkey,
						)
						.map_err(|_| "Failed to derive supported token account")?;

				let from_token_account = match from_token_account {
					Some(token_account) => SolPubkey::try_from(token_account)
						.map_err(|_| "Failed to decode the source token account")?,
					// Defaulting to the user's associated token account
					None => derive_associated_token_account(
						from.into(),
						api_environment.usdc_token_mint_pubkey,
					)
					.map_err(|_| "Failed to derive the associated token account")?
					.address
					.into(),
				};

				SolanaInstructionBuilder::x_swap_usdc(
					api_environment,
					destination_asset,
					destination_address,
					from,
					from_token_account,
					seed,
					event_data_account,
					token_supported_account.address.into(),
					input_amount,
					cf_parameters,
					channel_metadata,
				)
			},
			_ => Err("Invalid source_asset: Not a Solana asset.")?,
		}
		.into(),
	})
}

pub fn decode_bitcoin_vault_swap(
	broker_id: AccountId,
	nulldata_payload: Vec<u8>,
) -> Result<VaultSwapInputEncoded, DispatchErrorWithMessage> {
	let UtxoEncodedData {
		output_asset,
		output_address,
		parameters:
			BtcCfParameters {
				retry_duration,
				min_output_amount,
				number_of_chunks,
				chunk_interval,
				boost_fee,
				broker_fee,
				affiliates,
			},
	} = UtxoEncodedData::decode(&mut &nulldata_payload[..])
		.map_err(|_| "Failed to decode Bitcoin Null data Payload")?;

	Ok(VaultSwapInputEncoded {
		source_asset: Asset::Btc,
		destination_asset: output_asset,
		destination_address: output_address,
		broker_commission: broker_fee.into(),
		extra_parameters: VaultSwapExtraParameters::Bitcoin {
			min_output_amount,
			retry_duration: retry_duration.into(),
		},
		channel_metadata: None,
		boost_fee: boost_fee.into(),
		affiliate_fees: from_affiliate_and_fees(&broker_id, affiliates.to_vec())?,
		dca_parameters: Some(DcaParameters {
			number_of_chunks: number_of_chunks.into(),
			chunk_interval: chunk_interval.into(),
		}),
	})
}

pub fn decode_solana_vault_swap(
	instruction: SolInstruction,
) -> Result<VaultSwapInputEncoded, DispatchErrorWithMessage> {
	let DecodedXSwapParams {
		amount,
		src_asset,
		src_address,
		from_token_account,
		dst_address,
		dst_token,
		refund_parameters,
		dca_parameters,
		boost_fee,
		broker_id,
		broker_commission,
		affiliate_fees,
		ccm,
		seed,
	} = cf_chains::sol::decode_sol_instruction_data(&instruction)?;

	Ok(VaultSwapInputEncoded {
		source_asset: src_asset,
		destination_asset: dst_token,
		destination_address: dst_address,
		broker_commission,
		extra_parameters: VaultSwapExtraParameters::Solana {
			from: src_address.into(),
			seed,
			input_amount: amount,
			refund_parameters,
			from_token_account: from_token_account.map(|addr| addr.into()),
		},
		channel_metadata: ccm,
		boost_fee: boost_fee.into(),
		affiliate_fees: from_affiliate_and_fees(&broker_id, affiliate_fees)?,
		dca_parameters,
	})
}

pub fn validate_parameters(
	broker_id: &AccountId,
	source_chain: ForeignChain,
	destination_address: &EncodedAddress,
	destination_asset: Asset,
	dca_parameters: &Option<DcaParameters>,
	boost_fee: BasisPoints,
	broker_commission: BasisPoints,
	affiliate_fees: &Affiliates<AccountId>,
	retry_duration: BlockNumber,
	channel_metadata: &Option<CcmChannelMetadataUnchecked>,
) -> Result<Option<CcmChannelMetadataChecked>, DispatchErrorWithMessage> {
	let destination_chain = destination_address.chain();

	// Validate DCA parameters.
	if let Some(params) = dca_parameters.as_ref() {
		pallet_cf_swapping::Pallet::<Runtime>::validate_dca_params(params)?;
	}

	// Validate boost fee.
	if boost_fee > u8::MAX.into() {
		return Err(pallet_cf_swapping::Error::<Runtime>::BoostFeeTooHigh.into());
	}

	// Validate broker fee
	if broker_commission <
		pallet_cf_swapping::Pallet::<Runtime>::get_minimum_vault_swap_fee_for_broker(broker_id)
	{
		return Err(DispatchErrorWithMessage::from("Broker commission is too low"));
	}
	let _beneficiaries = pallet_cf_swapping::Pallet::<Runtime>::assemble_and_validate_broker_fees(
		broker_id.clone(),
		broker_commission,
		affiliate_fees.clone(),
	)?;

	// Validate refund duration.
	pallet_cf_swapping::Pallet::<Runtime>::validate_refund_params(retry_duration)?;

	// Ensure CCM message is valid
	let checked_ccm = channel_metadata
		.clone()
		.map(|ccm| {
			ChainAddressConverter::try_from_encoded_address(destination_address.clone())
				.map_err(|_| pallet_cf_swapping::Error::<Runtime>::InvalidDestinationAddress.into())
				.and_then(|dest_address| {
					ccm.to_checked(destination_asset, dest_address)
						.map_err(|_| DispatchErrorWithMessage::from("Invalid CCM"))
				})
		})
		.transpose()?;

	if let Some(ccm) = checked_ccm.as_ref() {
		if source_chain == ForeignChain::Bitcoin {
			return Err(DispatchErrorWithMessage::from(
				"Vault swaps with CCM are not supported for the Bitcoin Chain",
			));
		}
		if !destination_chain.ccm_support() {
			return Err(DispatchErrorWithMessage::from("Destination chain does not support CCM"));
		}

		// Do some additional checking for Solana ccms.
		if let DecodedCcmAdditionalData::Solana(decoded) = ccm.ccm_additional_data.clone() {
			let ccm_accounts = decoded.ccm_accounts();

			// Ensure the CCM parameters do not contain blacklisted accounts.
			// Load up environment variables.
			let api_environment = SolEnvironment::api_environment()
				.map_err(|_| "Failed to load Solana API environment")?;

			let agg_key: SolPubkey = SolEnvironment::current_agg_key()
				.map_err(|_| "Failed to load Solana Agg key")?
				.into();

			let on_chain_key: SolPubkey = SolEnvironment::current_on_chain_key()
				.map(|key| key.into())
				.unwrap_or_else(|_| agg_key);

			check_ccm_for_blacklisted_accounts(
				&ccm_accounts,
				vec![api_environment.token_vault_pda_account.into(), agg_key, on_chain_key],
			)
			.map_err(DispatchError::from)?;
		}
	}

	Ok(checked_ccm)
}
