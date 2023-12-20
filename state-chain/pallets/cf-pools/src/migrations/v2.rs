use crate::*;
use sp_std::marker::PhantomData;

/// Resets the CollectedFees.
pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		log::info!("Deleting CollectedNetworkFee");
		CollectedNetworkFee::<T>::kill();
		Zero::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
		ensure!(CollectedNetworkFee::<T>::get() == 0, "CollectedNetworkFee should be zero");
		Ok(())
	}
}
