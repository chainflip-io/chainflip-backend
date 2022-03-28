use frame_support::traits::OnRuntimeUpgrade;

use crate::*;

/// Adds ReputationPenalty as a storage item.
pub struct Migration<T>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// Migration from the runtime parameter value of version V0
		ReputationPointPenalty::<T>::put(ReputationPenalty { points: 1, blocks: 10u32.into() });
		0
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		assert_eq!(
			ReputationPointPenalty::<T>::get(),
			ReputationPenalty { points: 1, blocks: (10 as u32).into() }
		);
		log::info!(
			target: "runtime::cf_reputation",
			"migration: Reputation storage version v1 POST migration checks successful!"
		);
		Ok(())
	}
}
