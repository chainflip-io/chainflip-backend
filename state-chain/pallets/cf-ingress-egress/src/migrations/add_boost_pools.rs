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
		pub action: ChannelAction<T::AccountId>,
		/// The boost fee
		pub boost_fee: BasisPoints,
	}

	#[frame_support::storage_alias]
	pub type DepositChannelLookup<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		TargetChainAccount<T, I>,
		old::DepositChannelDetails<T, I>,
		OptionQuery,
	>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		// Add (default) boost status to existing deposit channels:
		DepositChannelLookup::<T, I>::translate(
			|_address, old_channel: old::DepositChannelDetails<T, I>| {
				Some(DepositChannelDetails::<T, I> {
					deposit_channel: old_channel.deposit_channel,
					opened_at: old_channel.opened_at,
					expires_at: old_channel.expires_at,
					action: old_channel.action,
					boost_fee: old_channel.boost_fee,
					boost_status: BoostStatus::NotBoosted,
				})
			},
		);

		// Create boost pools:
		use strum::IntoEnumIterator;
		for asset in TargetChainAsset::<T, I>::iter() {
			for pool_tier in BoostPoolTier::iter() {
				BoostPools::<T, I>::set(
					asset,
					pool_tier,
					Some(BoostPool::new(pool_tier as BasisPoints)),
				);
			}
		}

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
