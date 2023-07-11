use crate::*;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// This is sufficient because the old and new types have identical encoding.
		storage::migration::move_storage_from_pallet(
			Pallet::<T, I>::storage_metadata().prefix.as_bytes(),
			b"ChainState",
			b"CurrentChainState",
		);
		Weight::from_ref_time(0)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
		Ok(CurrentChainState::<T, I>::exists().encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), &'static str> {
		if <bool>::decode(&mut &_state[..])
			.map_err(|_| "Failed to decode ChainTracking pre-upgrade state.")?
		{
			assert!(CurrentChainState::<T, I>::exists());
		}
		Ok(())
	}
}
