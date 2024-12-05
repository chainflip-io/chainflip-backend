use frame_support::traits::{OnRuntimeUpgrade, UncheckedOnRuntimeUpgrade};

use pallet_cf_ingress_egress::DepositChannelDetails;

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub mod old {
	use cf_chains::{
		CcmChannelMetadata, ChannelRefundParameters, DepositChannel, ForeignChainAddress,
	};
	use cf_primitives::{AccountId, Beneficiaries};
	use frame_support::{pallet_prelude::OptionQuery, Twox64Concat};
	use pallet_cf_ingress_egress::BoostStatus;

	use super::*;

	#[derive(PartialEq, Eq, Encode, Decode)]
	pub struct DepositChannelDetails<T: Config<I>, I: 'static> {
		/// The owner of the deposit channel.
		pub owner: T::AccountId,
		pub deposit_channel: DepositChannel<T::TargetChain>,
		/// The block number at which the deposit channel was opened, expressed as a block number
		/// on the external Chain.
		pub opened_at: TargetChainBlockNumber<T, I>,
		/// The last block on the target chain that the witnessing will witness it in. If funds are
		/// sent after this block, they will not be witnessed.
		pub expires_at: TargetChainBlockNumber<T, I>,
		/// The action to be taken when the DepositChannel is deposited to.
		pub action: ChannelAction<T::AccountId>,
		/// The boost fee
		pub boost_fee: BasisPoints,
		/// Boost status, indicating whether there is pending boost on the channel
		pub boost_status: BoostStatus<TargetChainAmount<T, I>>,
	}

	/// Determines the action to take when a deposit is made to a channel.
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

	pub type DepositChannelLookup<T: pallet_cf_ingress_egress::Config<I>, I: 'static> = StorageMap<
		_,
		Twox64Concat,
		TargetChainAccount<T, I>,
		DepositChannelDetails<T, I>,
		OptionQuery,
	>;
}

pub struct DepositChannelDetailsMigration;

impl OnRuntimeUpgrade for DepositChannelDetailsMigration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok((old::DepositChannelLookup::iter().count() as u64).encode())
	}

	fn on_runtime_upgrade() -> Weight {
		pallet_cf_ingress_egress::DepositChannelLookup::<Runtime, _>::translate_values::<
			old::DepositChannelDetails,
			_,
		>(|old_deposit_channel_details| {
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
				} => ChannelAction::CcmTransfer {
					destination_asset,
					destination_address,
					broker_fees,
					channel_metadata,
					refund_params,
					dca_params,
				},
			};

			Some(DepositChannelDetails::<Runtime, _> {
				owner: old_deposit_channel_details.owner,
				deposit_channel: old_deposit_channel_details.deposit_channel,
				opened_at: old_deposit_channel_details.opened_at,
				expires_at: old_deposit_channel_details.expires_at,
				action: ChannelAction::Swap {
					destination_asset: old_deposit_channel_details.action.destination_asset,
					destination_address: old_deposit_channel_details.action.destination_address,
					broker_fees: old_deposit_channel_details.action.broker_fees,
					channel_metadata: None,
					refund_params: old_deposit_channel_details.action.refund_params,
					dca_params: old_deposit_channel_details.action.dca_params,
				},
				boost_fee: old_deposit_channel_details.boost_fee,
				boost_status: old_deposit_channel_details.boost_status,
			})
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_deposit_channel_lookup_count = <u64>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_deposit_channel_lookup_count =
			pallet_cf_ingress_egress::DepositChannelLookup::<Runtime, _>::iter().count() as u64;

		assert_eq!(pre_deposit_channel_lookup_count, post_deposit_channel_lookup_count);
		Ok(())
	}
}

pub struct NoopUpgrade;

impl OnRuntimeUpgrade for NoopUpgrade {
	fn on_runtime_upgrade() -> Weight {
		Weight::zero()
	}
}
