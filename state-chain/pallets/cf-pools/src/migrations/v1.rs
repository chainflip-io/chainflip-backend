use crate::*;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {
	use super::*;
	use sp_std::collections::btree_map::BTreeMap;

	#[derive(Clone, Debug, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub struct Pool<T: Config> {
		pub enabled: bool,
		/// A cache of all the range orders that exist in the pool. This must be kept up to date
		/// with the underlying pool.
		pub range_orders_cache: BTreeMap<T::AccountId, BTreeMap<OrderId, Range<Tick>>>,
		/// A cache of all the limit orders that exist in the pool. This must be kept up to date
		/// with the underlying pool. These are grouped by the asset the limit order is selling
		pub limit_orders_cache: SideMap<BTreeMap<T::AccountId, BTreeMap<OrderId, Tick>>>,
		pub pool_state: cf_amm::v1::PoolState<(T::AccountId, OrderId)>,
	}
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		Pools::<T>::translate::<old::Pool<T>, _>(|_key, old_pool: old::Pool<T>| {
			Some(Pool::<T> {
				range_orders_cache: old_pool.range_orders_cache,
				limit_orders_cache: old_pool.limit_orders_cache,
				pool_state: old_pool.pool_state.into(),
			})
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}
