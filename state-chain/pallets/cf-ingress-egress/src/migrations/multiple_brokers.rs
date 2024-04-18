use crate::*;
use cf_chains::DepositChannel;
use frame_support::traits::OnRuntimeUpgrade;

mod old {
	use super::*;

	#[derive(CloneNoBound, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct DepositChannelDetails<T: Config<I>, I: 'static> {
		pub deposit_channel: DepositChannel<T::TargetChain>,
		/// The block number at which the deposit channel was opened, expressed as a block number
		/// on the external Chain.
		pub opened_at: TargetChainBlockNumber<T, I>,
		/// The last block on the target chain that the witnessing will witness it in. If funds are
		/// sent after this block, they will not be witnessed.
		pub expires_at: TargetChainBlockNumber<T, I>,

		/// The action to be taken when the DepositChannel is deposited to.
		pub action: old::ChannelAction<T::AccountId>,
		/// The boost fee
		pub boost_fee: BasisPoints,
		/// Boost status, indicating whether there is pending boost on the channel
		pub boost_status: BoostStatus,
	}

	#[frame_support::storage_alias]
	pub type DepositChannelLookup<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		TargetChainAccount<T, I>,
		old::DepositChannelDetails<T, I>,
		OptionQuery,
	>;

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum ChannelAction<AccountId> {
		Swap {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			broker_id: AccountId,
			broker_commission_bps: BasisPoints,
		},
		LiquidityProvision {
			lp_account: AccountId,
		},
		CcmTransfer {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			channel_metadata: CcmChannelMetadata,
		},
	}
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		DepositChannelLookup::<T, I>::translate(
			|_address, old_channel: old::DepositChannelDetails<T, I>| match old_channel.action {
				old::ChannelAction::Swap {
					destination_asset,
					destination_address,
					broker_id,
					broker_commission_bps,
				} => Some(DepositChannelDetails::<T, I> {
					deposit_channel: old_channel.deposit_channel,
					opened_at: old_channel.opened_at,
					expires_at: old_channel.expires_at,
					action: ChannelAction::Swap {
						destination_asset,
						destination_address,
						broker_commission_bps: vec![Beneficiary {
							account: broker_id,
							bps: broker_commission_bps,
						}],
					},
					boost_fee: old_channel.boost_fee,
					boost_status: old_channel.boost_status,
				}),
				old::ChannelAction::LiquidityProvision { lp_account } =>
					Some(DepositChannelDetails::<T, I> {
						deposit_channel: old_channel.deposit_channel,
						opened_at: old_channel.opened_at,
						expires_at: old_channel.expires_at,
						action: ChannelAction::LiquidityProvision { lp_account },
						boost_fee: old_channel.boost_fee,
						boost_status: old_channel.boost_status,
					}),
				old::ChannelAction::CcmTransfer {
					destination_asset,
					destination_address,
					channel_metadata,
				} => Some(DepositChannelDetails::<T, I> {
					deposit_channel: old_channel.deposit_channel,
					opened_at: old_channel.opened_at,
					expires_at: old_channel.expires_at,
					action: ChannelAction::CcmTransfer {
						destination_asset,
						destination_address,
						channel_metadata,
					},
					boost_fee: old_channel.boost_fee,
					boost_status: old_channel.boost_status,
				}),
			},
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let number_of_channels_in_lookup =
			old::DepositChannelLookup::<T, I>::iter_keys().count() as u32;

		Ok(number_of_channels_in_lookup.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let number_of_channels_in_lookup_pre_migration =
			<u32>::decode(&mut &state[..]).expect("Pre-migration should encode a u32.");
		ensure!(
			DepositChannelLookup::<T, I>::iter_keys().count() as u32 ==
				number_of_channels_in_lookup_pre_migration,
			"DepositChannelLookup migration failed."
		);
		Ok(())
	}
}
