use frame_support::traits::OnRuntimeUpgrade;

use crate::*;

/// This migration is now obsolete.
pub struct Migration<T>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		0
	}
}
