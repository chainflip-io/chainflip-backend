use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};

use crate::*;
mod old {
	use super::*;

	#[derive(CloneNoBound, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct OldDepositChannelDetails<T: Config<I>, I: 'static> {
		pub deposit_channel: DepositChannel<T::TargetChain>,
		pub opened_at: TargetChainBlockNumber<T, I>,
		pub expires_at: TargetChainBlockNumber<T, I>,
		pub action: ChannelAction<T::AccountId>,
		pub boost_fee: BasisPoints,
		pub boost_status: BoostStatus<TargetChainAmount<T, I>>,
	}

	#[frame_support::storage_alias]
	pub type DepositChannelLookup<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		TargetChainAccount<T, I>,
		OldDepositChannelDetails<T, I>,
		OptionQuery,
	>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		for (account, channel_details) in old::DepositChannelLookup::<T, I>::drain() {
			let dummy_account = T::AccountId::decode(&mut &[0u8; 32][..]).unwrap();
			let new_channel_details = DepositChannelDetails {
				owner: dummy_account,
				deposit_channel: channel_details.deposit_channel,
				opened_at: channel_details.opened_at,
				expires_at: channel_details.expires_at,
				action: channel_details.action,
				boost_fee: channel_details.boost_fee,
				boost_status: channel_details.boost_status,
			};
			DepositChannelLookup::<T, I>::insert(account, new_channel_details);
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let number_of_old_channels: u32 =
			old::DepositChannelLookup::<T, I>::iter().collect::<Vec<_>>().len() as u32;
		Ok(number_of_old_channels.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let number_of_old_channels = u32::decode(&mut &state[..]).unwrap();
		let number_of_new_channels =
			DepositChannelLookup::<T, I>::iter().collect::<Vec<_>>().len() as u32;
		assert_eq!(number_of_old_channels, number_of_new_channels);
		Ok(())
	}
}
