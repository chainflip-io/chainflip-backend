use crate::*;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {
	use super::*;
	use sp_std::collections::btree_map::BTreeMap;

	#[derive(Copy, Clone, Debug, Encode, Decode, TypeInfo, MaxEncodedLen, PartialEq, Eq)]
	pub struct CanonicalAssetPair {
		pub assets: AssetsMap<Asset>,
	}

	#[derive(Clone, Debug, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub struct Pool<T: Config> {
		pub enabled: bool,
		/// A cache of all the range orders that exist in the pool. This must be kept up to date
		/// with the underlying pool.
		pub range_orders_cache: BTreeMap<T::AccountId, BTreeMap<OrderId, Range<Tick>>>,
		/// A cache of all the limit orders that exist in the pool. This must be kept up to date
		/// with the underlying pool. These are grouped by the asset the limit order is selling
		pub limit_orders_cache: AssetsMap<BTreeMap<T::AccountId, BTreeMap<OrderId, Tick>>>,
		pub pool_state: cf_amm::v1::PoolState<(T::AccountId, OrderId)>,
	}

	#[frame_support::storage_alias]
	pub type Pools<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, CanonicalAssetPair, Pool<T>, OptionQuery>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		for (key, pool) in old::Pools::<T>::drain().filter_map(|(old_key, old_pool)| {
			Some((
				AssetPair::new(old_key.assets[Assets::Base], old_key.assets[Assets::Quote])?,
				Pool::<T> {
					range_orders_cache: old_pool.range_orders_cache,
					limit_orders_cache: old_pool.limit_orders_cache.into(),
					pool_state: old_pool.pool_state.into(),
				},
			))
		}) {
			Pools::<T>::insert(key, pool);
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let number_of_pools = old::Pools::<T>::iter_keys().count() as u32;
		Ok(number_of_pools.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let number_of_pools_pre_migration = <u32>::decode(&mut &state[..]).unwrap();
		ensure!(
			Pools::<T>::iter_keys().count() as u32 == number_of_pools_pre_migration,
			"Pools migration failed"
		);
		Ok(())
	}
}
