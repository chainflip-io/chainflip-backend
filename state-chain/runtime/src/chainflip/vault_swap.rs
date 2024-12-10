use crate::{
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
};
use cf_primitives::{
	AffiliateAndFee, Affiliates, Asset, AssetAmount, BasisPoints, DcaParameters, SWAP_DELAY_BLOCKS,
};
use cf_traits::{AffiliateRegistry, KeyProvider};

use frame_support::pallet_prelude::{ConstU32, Get};
use scale_info::prelude::string::String;
use sp_runtime::BoundedVec;
use sp_std::vec::Vec;

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
		.map_err(|_| DispatchErrorWithMessage::Other("Too many affiliates provided".into()))
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
			|_| DispatchErrorWithMessage::Other("Private channel id out of bounds.".into()),
		)?,
	)
	.script_pubkey()
	.to_address(&Environment::network_environment().into());

	Ok(VaultSwapDetails::Bitcoin {
		nulldata_payload: encode_swap_params_in_nulldata_payload(params),
		deposit_address,
	})
}
