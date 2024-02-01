use crate::*;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;

// Copied from state-chain/node/src/chain_spec/testnet.rs:
// These represent approximately 2 hours on testnet block times
pub const BITCOIN_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 / (10 * 60);
pub const ETHEREUM_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 / 14;
pub const POLKADOT_EXPIRY_BLOCKS: u32 = 2 * 60 * 60 / 6;

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

// These were removed in 0.9.4
mod old {

	use super::*;

	#[derive(
		CloneNoBound, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen,
	)]
	#[scale_info(skip_type_params(T, I))]
	pub struct DepositChannelDetails<T: Config<I>, I: 'static> {
		pub deposit_channel: DepositChannel<T::TargetChain>,
		/// The block number at which the deposit channel was opened, expressed as a block number
		/// on the external Chain.
		pub opened_at: <T::TargetChain as Chain>::ChainBlockNumber,
		// *State Chain block number*
		pub expires_at: BlockNumberFor<T>,
	}

	#[frame_support::storage_alias]
	pub type ChannelActions<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		TargetChainAccount<T, I>,
		ChannelAction<<T as frame_system::Config>::AccountId>,
		OptionQuery,
	>;

	#[frame_support::storage_alias]
	pub type DepositChannelLookup<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		TargetChainAccount<T, I>,
		DepositChannelDetails<T, I>,
		OptionQuery,
	>;
}

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let lifetime: TargetChainBlockNumber<T, I> = match T::TargetChain::NAME {
			"Bitcoin" => BITCOIN_EXPIRY_BLOCKS.into(),
			"Ethereum" => ETHEREUM_EXPIRY_BLOCKS.into(),
			"Polkadot" => POLKADOT_EXPIRY_BLOCKS.into(),
			_ => unreachable!("Unsupported chain"),
		};

		DepositChannelLifetime::<T, I>::put(lifetime);

		let channel_lifetime = DepositChannelLifetime::<T, I>::get();
		let current_external_block_height = T::ChainTracking::get_block_height();
		let expiry_block = current_external_block_height.saturating_add(channel_lifetime);
		let recycle_block = expiry_block.saturating_add(channel_lifetime);

		let old_channel_lookup = old::DepositChannelLookup::<T, I>::drain().collect::<Vec<_>>();

		for (address, old_channel) in old_channel_lookup {
			if let Some(action) = old::ChannelActions::<T, I>::take(&address) {
				DepositChannelLookup::<T, I>::insert(
					address.clone(),
					DepositChannelDetails {
						deposit_channel: old_channel.deposit_channel,
						opened_at: old_channel.opened_at,
						expires_at: expiry_block,
						action,
					},
				);
			}

			// We're just going to recycle them 2 hours from when we did the migration.
			DepositChannelRecycleBlocks::<T, I>::append((recycle_block, address));

			// Remove any we missed above.
			let _ = old::ChannelActions::<T, I>::drain().collect::<Vec<_>>();
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
		let number_of_channels_in_lookup_pre_migration = <u32>::decode(&mut &state[..]).unwrap();
		ensure!(
			DepositChannelLookup::<T, I>::iter_keys().count() as u32 ==
				number_of_channels_in_lookup_pre_migration,
			"DepositChannelLookup migration failed."
		);
		ensure!(
			DepositChannelRecycleBlocks::<T, I>::decode_len().unwrap_or_default() as u32 ==
				number_of_channels_in_lookup_pre_migration,
			"DepositChannelRecycleBlocks migration failed."
		);
		Ok(())
	}
}
