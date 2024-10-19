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
		DepositChannelLookup::<T, I>::translate(
			|_account, channel_details: old::OldDepositChannelDetails<T, I>| {
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
				Some(new_channel_details)
			},
		);
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

#[cfg(test)]
mod migration_tests {
	use super::*;
	use crate::mock_eth::*;

	#[test]
	fn test_migration() {
		use cf_chains::{evm::DeploymentStatus, ForeignChainAddress::Eth};
		new_test_ext().execute_with(|| {
			let channel_id = 1u64;
			let address = sp_core::H160([1u8; 20]);
			let asset = cf_chains::assets::eth::Asset::Eth;
			let deployment_state = DeploymentStatus::Deployed;
			let lp_account = 5u64;
			let refund_address = Eth([1u8; 20].into());
			let opened_at = 1u64;
			let expires_at = 2u64;
			let action = ChannelAction::LiquidityProvision { lp_account, refund_address };
			let boost_fee = 1;
			let boost_status = BoostStatus::NotBoosted;

			old::DepositChannelLookup::<Test, _>::insert(
				address,
				old::OldDepositChannelDetails {
					deposit_channel: DepositChannel {
						asset,
						channel_id,
						address,
						state: deployment_state,
					},
					opened_at,
					expires_at,
					action: action.clone(),
					boost_fee,
					boost_status,
				},
			);
			assert_eq!(old::DepositChannelLookup::<Test, _>::iter().count(), 1);

			#[cfg(feature = "try-runtime")]
			let state = super::Migration::<Test, _>::pre_upgrade().unwrap();
			super::Migration::<Test, _>::on_runtime_upgrade();

			#[cfg(feature = "try-runtime")]
			super::Migration::<Test, _>::post_upgrade(state).unwrap();

			assert_eq!(DepositChannelLookup::<Test, _>::iter().count(), 1);

			let migrated_deposit_channel = DepositChannelLookup::<Test, _>::get(address)
				.expect("to have a channel in storage");

			assert_eq!(migrated_deposit_channel.owner, 0);
			assert_eq!(old::DepositChannelLookup::<Test, _>::iter().count(), 0);

			assert_eq!(migrated_deposit_channel.deposit_channel.asset, asset);
			assert_eq!(migrated_deposit_channel.deposit_channel.channel_id, channel_id);
			assert_eq!(migrated_deposit_channel.deposit_channel.address, address);
			assert_eq!(migrated_deposit_channel.deposit_channel.state, deployment_state);
			assert_eq!(migrated_deposit_channel.opened_at, opened_at);
			assert_eq!(migrated_deposit_channel.expires_at, expires_at);
			assert_eq!(migrated_deposit_channel.action, action);
		});
	}
}
