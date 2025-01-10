use crate::{
	chainflip::{
		address_derivation::btc::derive_btc_vault_deposit_address, AddressConverter,
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
	cf_parameters::build_cf_parameters,
	evm::api::{EvmCall, EvmEnvironmentProvider},
	sol::{
		api::SolanaEnvironment, instruction_builder::SolanaInstructionBuilder, SolAmount, SolPubkey,
	},
	Arbitrum, CcmChannelMetadata, ChannelRefundParametersEncoded, Ethereum, ForeignChain,
};
use cf_primitives::{
	AffiliateAndFee, Affiliates, Asset, AssetAmount, BasisPoints, DcaParameters, SWAP_DELAY_BLOCKS,
};
use cf_traits::AffiliateRegistry;
use scale_info::prelude::string::String;
use sp_core::U256;
use sp_std::vec::Vec;

fn to_affiliate_and_fees(
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
					.ok_or(pallet_cf_swapping::Error::<Runtime>::AffiliateNotRegistered)?,
				fee: beneficiary
					.bps
					.try_into()
					.map_err(|_| pallet_cf_swapping::Error::<Runtime>::AffiliateFeeTooHigh)?,
			})
		})
		.collect::<Result<Vec<AffiliateAndFee>, _>>()
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
	expires_at: u64,
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
		deposit_address: derive_btc_vault_deposit_address(private_channel_id),
		expires_at,
	})
}

pub fn evm_vault_swap<A>(
	broker_id: AccountId,
	source_asset: Asset,
	amount: AssetAmount,
	destination_asset: Asset,
	destination_address: EncodedAddress,
	broker_commission: BasisPoints,
	refund_params: ChannelRefundParametersEncoded,
	boost_fee: u8,
	affiliate_fees: Affiliates<AccountId>,
	dca_parameters: Option<DcaParameters>,
	channel_metadata: Option<cf_chains::CcmChannelMetadata>,
) -> Result<VaultSwapDetails<A>, DispatchErrorWithMessage> {
	let refund_params = refund_params.try_map_address(|addr| {
		ChainAddressConverter::try_from_encoded_address(addr)
			.map_err(|_| "Invalid refund address".into())
	})?;
	let processed_affiliate_fees = to_affiliate_and_fees(&broker_id, affiliate_fees)?
		.try_into()
		.map_err(|_| "Too many affiliates.")?;

	let cf_parameters = build_cf_parameters(
		refund_params,
		dca_parameters,
		boost_fee,
		broker_id,
		broker_commission,
		processed_affiliate_fees,
		channel_metadata.as_ref(),
	);

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
			let source_token_address = match source_asset {
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
			.ok_or(DispatchErrorWithMessage::from("Failed to look up EVM token address"))?;

			if let Some(ccm) = channel_metadata {
				Ok(cf_chains::evm::api::x_call_token::XCallToken::new(
					destination_address,
					destination_asset,
					ccm.message.to_vec(),
					ccm.gas_budget,
					source_token_address,
					amount,
					cf_parameters,
				)
				.abi_encoded_payload())
			} else {
				Ok(cf_chains::evm::api::x_swap_token::XSwapToken::new(
					destination_address,
					destination_asset,
					source_token_address,
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
		})),
		ForeignChain::Arbitrum => Ok(VaultSwapDetails::arbitrum(EvmVaultSwapDetails {
			calldata,
			// Only return `amount` for native currently. 0 for Tokens
			value: (source_asset == Asset::ArbEth)
				.then_some(U256::from(amount))
				.unwrap_or_default(),
			to: Environment::arb_vault_address(),
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
	refund_parameters: ChannelRefundParametersEncoded,
	channel_metadata: Option<CcmChannelMetadata>,
	boost_fee: u8,
	affiliate_fees: Affiliates<AccountId>,
	dca_parameters: Option<DcaParameters>,
	from: EncodedAddress,
	event_data_account: EncodedAddress,
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
	let refund_parameters = refund_parameters.try_map_address(|addr| {
		ChainAddressConverter::try_from_encoded_address(addr)
			.map_err(|_| "Invalid refund address".into())
	})?;
	let event_data_account = SolPubkey::try_from(event_data_account)
		.map_err(|_| "Invalid Solana Address: event_data_account")?;
	let input_amount =
		SolAmount::try_from(input_amount).map_err(|_| "Input amount exceeded MAX")?;
	let cf_parameters = build_cf_parameters(
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

				let from_token_account = SolPubkey::try_from(
					from_token_account.ok_or("From token account is required for SolUsdc swaps")?,
				)
				.map_err(|_| "Invalid Solana Address: from_token_account")?;

				SolanaInstructionBuilder::x_swap_usdc(
					api_environment,
					destination_asset,
					destination_address,
					from,
					from_token_account,
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
