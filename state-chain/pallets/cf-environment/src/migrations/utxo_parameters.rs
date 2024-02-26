use crate::*;
use frame_support::traits::OnRuntimeUpgrade;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		UtxoParameters::<T>::set(INITIAL_UTXO_PARAMETERS);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		assert_eq!(UtxoParameters::<T>::get(), INITIAL_UTXO_PARAMETERS);

		Ok(())
	}
}
