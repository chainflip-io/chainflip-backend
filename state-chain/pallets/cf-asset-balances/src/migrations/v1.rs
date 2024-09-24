use crate::*;
use frame_support::traits::OnRuntimeUpgrade;

mod old {
	use cf_primitives::{Asset, AssetAmount};
	use frame_support::{pallet_prelude::ValueQuery, Identity, Twox64Concat};

	#[frame_support::storage_alias]
	pub type EarnedBrokerFees<T: Config> = StorageDoubleMap<
		Swapping,
		Identity,
		<T as frame_system::Config>::AccountId,
		Twox64Concat,
		Asset,
		AssetAmount,
		ValueQuery,
	>;
}

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		for (account_id, asset, earned_fees) in old::EarnedBrokerFees::<T>::drain() {
			log::info!(
				"💸 Migrating earned broker fees for account {:?} and asset {:?} to free balances",
				account_id,
				asset
			);
			FreeBalances::<T>::mutate(account_id, asset, |balance| {
				*balance = balance.saturating_add(earned_fees);
			});
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let earned_fees = old::EarnedBrokerFees::<T>::iter().collect::<Vec<_>>();
		assert!(!earned_fees.is_empty());
		Ok(earned_fees.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let earned_fees = Vec::<(<T as frame_system::Config>::AccountId, Asset, u128)>::decode(
			&mut state.as_slice(),
		)
		.unwrap();

		assert!(old::EarnedBrokerFees::<T>::iter().count() == 0);

		for (account_id, asset, earned_fees) in earned_fees {
			assert!(FreeBalances::<T>::get(account_id, asset) >= earned_fees);
		}

		Ok(())
	}
}
