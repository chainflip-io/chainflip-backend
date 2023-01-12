use frame_system::{ensure_signed, Config};

use crate::AccountRoleRegistry;

impl<T: Config> AccountRoleRegistry<T> for () {
	fn register_account_role(
		_who: &<T as frame_system::Config>::AccountId,
		_role: cf_primitives::AccountRole,
	) -> sp_runtime::DispatchResult {
		Ok(())
	}

	fn ensure_account_role(
		origin: <T as frame_system::Config>::RuntimeOrigin,
		_role: cf_primitives::AccountRole,
	) -> Result<<T as frame_system::Config>::AccountId, frame_support::error::BadOrigin> {
		// always passes, regardless of role for the benchmarks
		ensure_signed(origin)
	}
}
