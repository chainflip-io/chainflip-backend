use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::{Config, DepositChannelDetails};

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub mod old {
	use crate::BoostStatus;
	use cf_chains::{ChannelRefundParameters, DepositChannel, ForeignChainAddress};
	use cf_primitives::Beneficiaries;
	use frame_support::{pallet_prelude::OptionQuery, Twox64Concat};

	use super::*;

	#[derive(PartialEq, Eq, Encode, Decode)]
	pub struct DepositChannelDetails<T: Config<I>, I: 'static> {
		pub owner: T::AccountId,
		pub deposit_channel: DepositChannel<T::TargetChain>,
		pub opened_at: TargetChainBlockNumber<T, I>,
		pub expires_at: TargetChainBlockNumber<T, I>,
		pub action: ChannelAction<T::AccountId>,
		pub boost_fee: BasisPoints,
		pub boost_status: BoostStatus<TargetChainAmount<T, I>>,
	}

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub struct CcmChannelMetadata {
		pub message: CcmMessage,
		pub gas_budget: AssetAmount,
		pub ccm_additional_data: CcmAdditionalData,
	}

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub enum ChannelAction<AccountId> {
		Swap {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			broker_fees: Beneficiaries<AccountId>,
			refund_params: Option<ChannelRefundParameters>,
			dca_params: Option<DcaParameters>,
		},
		LiquidityProvision {
			lp_account: AccountId,
			refund_address: Option<ForeignChainAddress>,
		},
		CcmTransfer {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			broker_fees: Beneficiaries<AccountId>,
			channel_metadata: CcmChannelMetadata,
			refund_params: Option<ChannelRefundParameters>,
			dca_params: Option<DcaParameters>,
		},
	}

	#[frame_support::storage_alias]
	pub type DepositChannelLookup<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		TargetChainAccount<T, I>,
		DepositChannelDetails<T, I>,
		OptionQuery,
	>;
}

pub struct DepositChannelDetailsMigration<T: Config<I>, I: 'static = ()>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for DepositChannelDetailsMigration<T, I> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok((old::DepositChannelLookup::<T, I>::iter_keys().count() as u64).encode())
	}

	fn on_runtime_upgrade() -> Weight {
		crate::DepositChannelLookup::<T, I>::translate_values::<old::DepositChannelDetails<T, I>, _>(
			|old_deposit_channel_details| {
				let action = match old_deposit_channel_details.action {
					old::ChannelAction::LiquidityProvision { lp_account, refund_address } =>
						ChannelAction::LiquidityProvision { lp_account, refund_address },
					old::ChannelAction::Swap {
						destination_asset,
						destination_address,
						broker_fees,
						refund_params,
						dca_params,
					} => ChannelAction::Swap {
						destination_asset,
						destination_address,
						broker_fees,
						channel_metadata: None,
						refund_params,
						dca_params,
					},
					old::ChannelAction::CcmTransfer {
						destination_asset,
						destination_address,
						broker_fees,
						channel_metadata,
						refund_params,
						dca_params,
					} => ChannelAction::Swap {
						destination_asset,
						destination_address,
						broker_fees,
						channel_metadata: Some(crate::CcmChannelMetadata {
							message: channel_metadata.message,
							gas_budget: channel_metadata.gas_budget,
							ccm_additional_data: channel_metadata.ccm_additional_data,
						}),
						refund_params,
						dca_params,
					},
				};

				Some(DepositChannelDetails::<T, I> {
					owner: old_deposit_channel_details.owner,
					deposit_channel: old_deposit_channel_details.deposit_channel,
					opened_at: old_deposit_channel_details.opened_at,
					expires_at: old_deposit_channel_details.expires_at,
					action,
					boost_fee: old_deposit_channel_details.boost_fee,
					boost_status: old_deposit_channel_details.boost_status,
				})
			},
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_deposit_channel_lookup_count = <u64>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_deposit_channel_lookup_count =
			crate::DepositChannelLookup::<T, I>::iter().count() as u64;

		assert_eq!(pre_deposit_channel_lookup_count, post_deposit_channel_lookup_count);
		Ok(())
	}
}
