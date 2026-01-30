use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::Config;

use crate::*;
use cf_chains::evm::DeploymentStatus;
use cf_traits::ChainflipWithTargetChain;
use codec::{Decode, Encode};
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use frame_support::sp_runtime::DispatchError;
use scale_info::prelude::collections::VecDeque;
pub mod old {
	use super::*;

	/// The old DeploymentStatus enum where Deployed had no inner data.
	#[derive(Clone, PartialEq, Eq, Copy, Debug, Default, Encode, Decode)]
	pub enum OldDeploymentStatus {
		#[default]
		Undeployed,
		Pending,
		Deployed,
	}

	impl OldDeploymentStatus {
		pub fn into_new(self) -> DeploymentStatus {
			match self {
				Self::Undeployed => DeploymentStatus::Undeployed,
				Self::Pending => DeploymentStatus::Pending,
				// We don't know the block number, use 0 as a sentinel.
				Self::Deployed => DeploymentStatus::Deployed(0),
			}
		}
	}

	/// Specialised old DepositChannel for EVM chains where state = OldDeploymentStatus.
	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub struct OldEvmDepositChannel<C: Chain> {
		pub channel_id: ChannelId,
		pub address: C::ChainAccount,
		pub asset: C::ChainAsset,
		pub state: OldDeploymentStatus,
	}

	impl<C: Chain<DepositChannelState = DeploymentStatus>> OldEvmDepositChannel<C> {
		pub fn into_new(self) -> DepositChannel<C> {
			DepositChannel {
				channel_id: self.channel_id,
				address: self.address,
				asset: self.asset,
				state: self.state.into_new(),
			}
		}
	}

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub struct OldDepositChannelDetails<T: Config<I>, I: 'static>
	where
		T::TargetChain: Chain<DepositChannelState = DeploymentStatus>,
	{
		pub owner: T::AccountId,
		pub deposit_channel: OldEvmDepositChannel<T::TargetChain>,
		pub opened_at: TargetChainBlockNumber<T, I>,
		pub expires_at: TargetChainBlockNumber<T, I>,
		pub action: ChannelAction<T::AccountId, TargetChainAccount<T, I>>,
		pub boost_fee: BasisPoints,
		pub boost_status: BoostStatus<TargetChainAmount<T, I>, BlockNumberFor<T>>,
		pub is_marked_for_rejection: bool,
	}

	#[frame_support::storage_alias]
	pub type DepositChannelPool<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		ChannelId,
		OldEvmDepositChannel<<T as ChainflipWithTargetChain<I>>::TargetChain>,
	>;

	#[frame_support::storage_alias]
	pub type PreallocatedChannels<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		VecDeque<OldEvmDepositChannel<<T as ChainflipWithTargetChain<I>>::TargetChain>>,
		ValueQuery,
	>;

	#[frame_support::storage_alias]
	pub type DepositChannelLookup<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		TargetChainAccount<T, I>,
		OldDepositChannelDetails<T, I>,
		OptionQuery,
	>;
}

fn migrate_evm_channels<T: Config<I>, I: 'static>() -> Weight
where
	T::TargetChain: Chain<DepositChannelState = DeploymentStatus>,
{
	log::info!(
		"🍩 Running migration: DeploymentStatus Deployed variant now includes block number."
	);

	// Migrate DepositChannelPool
	let mut pool_count = 0u64;
	crate::DepositChannelPool::<T, I>::translate_values::<
		old::OldEvmDepositChannel<T::TargetChain>,
		_,
	>(|old_channel| {
		pool_count += 1;
		Some(old_channel.into_new())
	});
	log::info!("🍩 Migrated {} DepositChannelPool entries.", pool_count);

	// Migrate PreallocatedChannels
	let mut prealloc_count = 0u64;
	crate::PreallocatedChannels::<T, I>::translate_values::<
		VecDeque<old::OldEvmDepositChannel<T::TargetChain>>,
		_,
	>(|old_channels| {
		prealloc_count += 1;
		Some(old_channels.into_iter().map(|ch| ch.into_new()).collect())
	});
	log::info!("🍩 Migrated {} PreallocatedChannels entries.", prealloc_count);

	// Migrate DepositChannelLookup
	let mut lookup_count = 0u64;
	crate::DepositChannelLookup::<T, I>::translate_values::<old::OldDepositChannelDetails<T, I>, _>(
		|old| {
			lookup_count += 1;
			Some(DepositChannelDetails {
				owner: old.owner,
				deposit_channel: old.deposit_channel.into_new(),
				opened_at: old.opened_at,
				expires_at: old.expires_at,
				action: old.action,
				boost_fee: old.boost_fee,
				boost_status: old.boost_status,
				is_marked_for_rejection: old.is_marked_for_rejection,
			})
		},
	);
	log::info!("🍩 Migrated {} DepositChannelLookup entries.", lookup_count);
	log::info!("🍩 Migration complete.");

	Weight::zero()
}

#[cfg(feature = "try-runtime")]
fn pre_upgrade_evm<T: Config<I>, I: 'static>() -> Result<Vec<u8>, DispatchError>
where
	T::TargetChain: Chain<DepositChannelState = DeploymentStatus>,
{
	let pool_count = old::DepositChannelPool::<T, I>::iter_keys().count();
	let lookup_keys = old::DepositChannelLookup::<T, I>::iter_keys().count();
	let prealloc_count = old::PreallocatedChannels::<T, I>::iter_keys().count();

	log::info!(
		"🍩 Pre-upgrade: DepositChannelPool={}, DepositChannelLookup={}, PreallocatedChannels={}",
		pool_count,
		lookup_keys,
		prealloc_count,
	);

	Ok((pool_count as u32, lookup_keys as u32, prealloc_count as u32).encode())
}

#[cfg(feature = "try-runtime")]
fn post_upgrade_evm<T: Config<I>, I: 'static>(state: Vec<u8>) -> Result<(), DispatchError>
where
	T::TargetChain: Chain<DepositChannelState = DeploymentStatus>,
{
	let (pre_pool_count, pre_lookup_count, pre_prealloc_count) =
		<(u32, u32, u32)>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode pre-upgrade state"))?;

	let post_pool_count = crate::DepositChannelPool::<T, I>::iter_keys().count() as u32;
	assert_eq!(pre_pool_count, post_pool_count, "DepositChannelPool count mismatch");

	let post_lookup_count = crate::DepositChannelLookup::<T, I>::iter_keys().count() as u32;
	assert_eq!(pre_lookup_count, post_lookup_count, "DepositChannelLookup keys mismatch");

	let post_prealloc_count = crate::PreallocatedChannels::<T, I>::iter_keys().count() as u32;
	assert_eq!(pre_prealloc_count, post_prealloc_count, "PreallocatedChannels count mismatch");

	Ok(())
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for Migration<T, I>
where
	T::TargetChain: Chain<DepositChannelState = DeploymentStatus>,
{
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		match T::TargetChain::get() {
			ForeignChain::Ethereum | ForeignChain::Arbitrum => pre_upgrade_evm::<T, I>(),
			_ => {
				log::info!("No migration requited for this chain");
				Ok(vec![])
			},
		}
	}

	fn on_runtime_upgrade() -> Weight {
		match T::TargetChain::get() {
			ForeignChain::Ethereum | ForeignChain::Arbitrum => migrate_evm_channels::<T, I>(),
			_ => {
				log::info!("No migration requited for this chain");
				Weight::zero()
			},
		}
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		match T::TargetChain::get() {
			ForeignChain::Ethereum | ForeignChain::Arbitrum => post_upgrade_evm::<T, I>(state),
			_ => {
				log::info!("No migration requited for this chain");
				Ok(())
			},
		}
	}
}
