use crate::AccountRoleRegistry;
use cf_primitives::AccountRole;
use frame_system::{ensure_signed, Config};

use super::{MockPallet, MockPalletStorage};

pub struct MockAccountRoleRegistry;

impl MockPallet for MockAccountRoleRegistry {
	const PREFIX: &'static [u8] = b"MockAccountRoleRegistry";
}

const ACCOUNT_ROLES: &[u8] = b"AccountRoles";

impl<T: Config> AccountRoleRegistry<T> for MockAccountRoleRegistry {
	fn register_account_role(
		account_id: &<T as frame_system::Config>::AccountId,
		role: AccountRole,
	) -> sp_runtime::DispatchResult {
		if <Self as MockPalletStorage>::get_storage::<_, AccountRole>(ACCOUNT_ROLES, account_id)
			.is_some()
		{
			return Err("Account already registered".into())
		}
		<Self as MockPalletStorage>::put_storage(ACCOUNT_ROLES, account_id, role);
		Ok(())
	}

	fn deregister_account_role(
		account_id: &<T as Config>::AccountId,
		role: AccountRole,
	) -> sp_runtime::DispatchResult {
		match <Self as MockPalletStorage>::take_storage::<_, AccountRole>(ACCOUNT_ROLES, account_id)
		{
			Some(r) if r == role => Ok(()),
			Some(_) => Err("Account role mismatch".into()),
			_ => Err("Account not registered".into()),
		}
	}

	fn has_account_role(who: &<T as Config>::AccountId, role: AccountRole) -> bool {
		<Self as MockPalletStorage>::get_storage::<_, AccountRole>(ACCOUNT_ROLES, who)
			.unwrap_or(AccountRole::Unregistered) ==
			role
	}

	fn ensure_account_role(
		origin: <T as frame_system::Config>::RuntimeOrigin,
		role: AccountRole,
	) -> Result<<T as frame_system::Config>::AccountId, frame_support::error::BadOrigin> {
		match ensure_signed(origin) {
			Ok(account_id) => {
				let account_role = <Self as MockPalletStorage>::get_storage::<_, AccountRole>(
					ACCOUNT_ROLES,
					account_id.clone(),
				)
				.unwrap_or(AccountRole::Unregistered);
				if account_role == role {
					Ok(account_id)
				} else {
					Err(frame_support::error::BadOrigin)
				}
			},
			Err(_) => Err(frame_support::error::BadOrigin),
		}
	}
}
