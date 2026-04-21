use crate::{boost::BOOST_FEE, *};
use frame_support::traits::UncheckedOnRuntimeUpgrade;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {
	use super::*;

	// Old BoostedDeposits had a different value type (BTreeMap<BoostPoolTier,
	// BoostPoolContribution>). We only need to clear it, so we don't bother defining the old
	// value type.
	#[frame_support::storage_alias]
	pub type BoostedDeposits<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Twox64Concat,
		Asset,
		Twox64Concat,
		PrewitnessedDepositId,
		Vec<u8>,
		OptionQuery,
	>;
}

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		// 1. Clear old BoostedDeposits (encoding changed, should be empty).
		let count = old::BoostedDeposits::<T>::iter().count();
		if count > 0 {
			log::warn!("BoostedDeposits should be empty at upgrade time, found {} entries", count);
		}
		let _ = old::BoostedDeposits::<T>::clear(u32::MAX, None);

		// 2. Remove non-5bps boost pools, crediting LPs their funds.
		let legacy_pools_to_remove: Vec<_> = BoostPools::<T>::iter()
			.filter(|(_, tier, _)| *tier != BOOST_FEE)
			.map(|(asset, tier, pool)| (asset, tier, pool.core_pool_id))
			.collect();

		for (asset, tier, core_pool_id) in legacy_pools_to_remove {
			if let Some(core_pool) = CorePools::<T>::take(asset, core_pool_id) {
				for (booster_id, amount) in core_pool.get_amounts() {
					if amount > 0 {
						T::Balance::credit_account(&booster_id, asset, amount);

						log::info!(
							"Refunding booster {:?} with {} {:?}",
							&booster_id,
							amount,
							asset
						);

						Pallet::<T>::deposit_event(Event::StoppedBoosting {
							booster_id,
							boost_pool: BoostPoolId { asset, tier },
							unlocked_amount: amount,
							pending_boosts: Default::default(),
						});
					}
				}
			}

			BoostPools::<T>::remove(asset, tier);
			log::info!("Removed boost pool: asset={:?}, tier={}", asset, tier);
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		ensure!(
			old::BoostedDeposits::<T>::iter().count() == 0,
			"BoostedDeposits should be empty after migration"
		);
		ensure!(
			BoostPools::<T>::iter().all(|(_, tier, _)| tier == BOOST_FEE),
			"Only 5bps fee tier legacy pools should remain"
		);
		Ok(())
	}
}
