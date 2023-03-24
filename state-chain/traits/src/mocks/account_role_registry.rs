use crate::AccountRoleRegistry;
use cf_primitives::AccountRole;
use frame_system::{ensure_signed, Config};

use super::{MockPallet, MockPalletStorage};

pub struct MockAccountRoleRegistry;

impl MockPallet for MockAccountRoleRegistry {
	const PREFIX: &'static [u8] = b"MockAccountRoleRegistry";
}

impl<T: Config> AccountRoleRegistry<T> for MockAccountRoleRegistry {
	fn register_account_role(
		account_id: &<T as frame_system::Config>::AccountId,
		role: AccountRole,
	) -> sp_runtime::DispatchResult {
		if <Self as MockPalletStorage>::get_storage::<_, AccountRole>(b"Roles", account_id)
			.is_some()
		{
			return Err("Account already registered".into())
		}
		<Self as MockPalletStorage>::put_storage(b"Roles", account_id, role);
		Ok(())
	}

	fn ensure_account_role(
		origin: <T as frame_system::Config>::RuntimeOrigin,
		role: AccountRole,
	) -> Result<<T as frame_system::Config>::AccountId, frame_support::error::BadOrigin> {
		match ensure_signed(origin) {
			Ok(account_id) => {
				let account_role = <Self as MockPalletStorage>::get_storage::<_, AccountRole>(
					b"Roles",
					account_id.clone(),
				)
				.unwrap_or(AccountRole::None);
				if account_role == role {
					Ok(account_id)
				} else {
					Err(frame_support::error::BadOrigin)
				}
			},
			Err(_) => Err(frame_support::error::BadOrigin),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn register_account(_account_id: T::AccountId, _role: AccountRole) {}

	#[cfg(feature = "runtime-benchmarks")]
	fn get_account_role(_account_id: T::AccountId) -> AccountRole {
		AccountRole::None
	}
}
