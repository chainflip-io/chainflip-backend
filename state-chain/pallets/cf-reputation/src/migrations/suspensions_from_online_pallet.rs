use frame_support::{traits::OnRuntimeUpgrade, weights::RuntimeDbWeight};

use crate::{Config, Reputations};

/// The storage is migrated at the runtime level.
pub struct Migration<T>(sp_std::marker::PhantomData<T>);

mod old {
	use crate::{Config, RuntimeReputationTracker};
	use frame_support::{generate_storage_alias, Blake2_128Concat};

	generate_storage_alias!(
		Reputation, Reputations<T: Config>
			=> Map<(Blake2_128Concat, T::ValidatorId), RuntimeReputationTracker<T>>
	);

	pub fn take_old_reputation<T: crate::Config>(
	) -> impl IntoIterator<Item = (T::ValidatorId, RuntimeReputationTracker<T>)> {
		Reputations::<T>::iter().drain()
	}
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let mut count = 0;
		for (id, rep) in old::take_old_reputation() {
			Reputations::<T>::insert(id, rep);
			count += 1;
		}

		RuntimeDbWeight::default().reads_writes(count, count)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		use crate::Suspensions;
		use frame_support::ensure;

		ensure!(Suspensions::<T>::iter().any(|_| true), {
			"🛑 Expected Suspensions to be non-empty."
		});
		ensure!(Reputations::<T>::iter().any(|_| true), {
			"🛑 Expected Reputations to be non-empty."
		});
		Ok(())
	}
}
