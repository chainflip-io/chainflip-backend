use frame_support::traits::OnRuntimeUpgrade;

use crate::Config;

/// The storage is migrated at the runtime level.
pub struct Migration<T>(sp_std::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		use crate::Suspensions;
		use frame_support::ensure;

		ensure!(Suspensions::<T>::iter().any(|_| true), {
			"🛑 Expected Suspensions to be non-empty."
		});
		Ok(())
	}
}
