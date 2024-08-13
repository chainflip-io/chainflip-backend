use crate::{Config, EarnedBrokerFees};
use cf_primitives::Asset;
use frame_support::{traits::OnRuntimeUpgrade, Identity, Twox64Concat};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod old {
	use crate::{Config, Pallet};
	use cf_primitives::{Asset, AssetAmount};
	use frame_support::pallet_prelude::*;

	#[frame_support::storage_alias]
	pub type EarnedBrokerFees<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Blake2_128Concat,
		<T as frame_system::Config>::AccountId,
		Twox64Concat,
		Asset,
		AssetAmount,
		ValueQuery,
	>;
}

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let keys = old::EarnedBrokerFees::<T>::iter_keys().collect::<Vec<_>>();
		for (key1, key2) in keys {
			EarnedBrokerFees::<T>::migrate_keys::<
				Identity,
				Twox64Concat,
				<T as frame_system::Config>::AccountId,
				Asset,
			>(key1, key2);
		}

		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		use codec::Encode;
		use sp_std::collections::btree_set::BTreeSet;

		let old = old::EarnedBrokerFees::<T>::iter()
			.map(|(k1, k2, v)| ((k1, k2), v))
			.collect::<BTreeSet<_>>();
		Ok(old.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use sp_std::collections::btree_set::BTreeSet;

		use codec::Decode;
		use frame_support::ensure;

		let old = BTreeSet::<_>::decode(&mut &state[..])
			.map_err(|_| DispatchError::Other("Decode error"))?;
		let new = EarnedBrokerFees::<T>::iter()
			.map(|(k1, k2, v)| ((k1, k2), v))
			.collect::<BTreeSet<_>>();

		ensure!(
			old == new,
			DispatchError::Other("Migration failed, old and new broker fees do not match")
		);

		Ok(())
	}
}
