use frame_system::{ensure_signed, Config};

use crate::AccountRoleRegistry;
use cf_primitives::AccountRole;
use sp_runtime::DispatchResult;

impl<T: Config> AccountRoleRegistry<T> for () {
	fn register_account_role(
		_who: &<T as frame_system::Config>::AccountId,
		_role: AccountRole,
	) -> DispatchResult {
		Ok(())
	}

	fn ensure_account_role(
		origin: <T as frame_system::Config>::RuntimeOrigin,
		_role: AccountRole,
	) -> Result<<T as frame_system::Config>::AccountId, frame_support::error::BadOrigin> {
		// always passes, regardless of role for the benchmarks
		ensure_signed(origin)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn register_account(_account_id: T::AccountId, _role: AccountRole) {}

	#[cfg(feature = "runtime-benchmarks")]
	fn get_account_role(_account_id: T::AccountId) -> AccountRole {
		AccountRole::None
	}
}
