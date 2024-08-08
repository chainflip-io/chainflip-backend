use crate::Config;
use frame_support::traits::OnRuntimeUpgrade;

#[cfg(feature = "try-runtime")]
use frame_support::ensure;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub struct Migration<T: Config>(sp_std::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// Noop: the migration is applied from the pools pallet;
		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
		// Assumption is that the migration has already been performed in the pools pallet.
		ensure!(crate::FlipBuyInterval::<T>::exists(), "FlipBuyInterval doesn't exist");
		ensure!(crate::CollectedNetworkFee::<T>::exists(), "CollectedNetworkFee doesn't exist");

		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), frame_support::sp_runtime::DispatchError> {
		// Other checks are made in the pools migration.
		ensure!(crate::FlipBuyInterval::<T>::exists(), "FlipBuyInterval doesn't exist");
		ensure!(crate::CollectedNetworkFee::<T>::exists(), "CollectedNetworkFee doesn't exist");

		Ok(())
	}
}
