use frame_support::traits::OnRuntimeUpgrade;

use crate::Config;

/// The storage is migrated at the runtime level, nothing required here.
pub struct Migration<T: Config>(sp_std::marker::PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		use crate::LastHeartbeat;
		use frame_support::ensure;

		ensure!(LastHeartbeat::<T>::iter().any(|_| true), {
			"🛑 Expected LastHeartbeat to be non-empty."
		});
		Ok(())
	}
}
