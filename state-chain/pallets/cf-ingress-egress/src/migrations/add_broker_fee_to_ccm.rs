use crate::*;
use frame_support::traits::OnRuntimeUpgrade;

pub(super) mod old {

	use super::*;

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum ChannelAction<AccountId> {
		Swap {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			broker_fees: Beneficiaries<AccountId>,
			refund_params: Option<ChannelRefundParameters>,
		},
		LiquidityProvision {
			lp_account: AccountId,
		},
		CcmTransfer {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			channel_metadata: CcmChannelMetadata,
			refund_params: Option<ChannelRefundParameters>,
		},
	}

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
		pub action: ChannelAction<T::AccountId>,
		/// The boost fee
		pub boost_fee: BasisPoints,
		/// Boost status, indicating whether there is pending boost on the channel
		pub boost_status: BoostStatus<TargetChainAmount<T, I>>,
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

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		DepositChannelLookup::<T, I>::translate(|_, details: old::DepositChannelDetails<T, I>| {
			Some(DepositChannelDetails {
				deposit_channel: details.deposit_channel,
				opened_at: details.opened_at,
				expires_at: details.expires_at,
				action: match details.action {
					old::ChannelAction::Swap {
						destination_asset,
						destination_address,
						broker_fees,
						refund_params,
					} => ChannelAction::Swap {
						destination_asset,
						destination_address,
						broker_fees,
						refund_params,
					},
					old::ChannelAction::LiquidityProvision { lp_account } =>
						ChannelAction::LiquidityProvision { lp_account },
					old::ChannelAction::CcmTransfer {
						destination_asset,
						destination_address,
						channel_metadata,
						refund_params,
					} => ChannelAction::CcmTransfer {
						destination_asset,
						destination_address,
						broker_fees: Default::default(),
						channel_metadata,
						refund_params,
					},
				},
				boost_fee: details.boost_fee,
				boost_status: details.boost_status,
			})
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok((old::DepositChannelLookup::<T, I>::iter().count() as u32).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let old_count = <u32>::decode(&mut &state[..]).expect("Failed to decode count");
		let new_count = DepositChannelLookup::<T, I>::iter().count() as u32;
		assert_eq!(old_count, new_count, "Migration failed: counts do not match");
		Ok(())
	}
}
