use super::Permissions;
use crate::mock::*;
use frame_support::assert_ok;
use cf_traits::PermissionError;

const ALICE: u64 = 100;
const BOB: u64 = 101;
const CHARLIE: u64 = 102;
const SCOPE_1 : u64 = 1;
const SCOPE_2 : u64 = 2;

#[test]
fn set_scope() {
	new_test_ext().execute_with(|| {
		assert_ok!(PermissionsManager::set_scope(ALICE, SCOPE_1));
		assert_ok!(PermissionsManager::set_scope(ALICE, SCOPE_2));
		assert_ok!(PermissionsManager::set_scope(BOB, SCOPE_2));
		assert_eq!(PermissionsManager::scope(BOB).unwrap(), SCOPE_2);
		assert_eq!(PermissionsManager::scope(ALICE).unwrap(), SCOPE_2);
		assert!(PermissionsManager::has_scope(ALICE, SCOPE_2).unwrap());
		assert_eq!(PermissionsManager::set_scope(BAD_ACTOR, SCOPE_1).unwrap_err(),
				   PermissionError::FailedToSetScope);
		assert_eq!(PermissionsManager::scope(CHARLIE).unwrap_err(),
				   PermissionError::AccountNotFound);
	});
}

#[test]
fn revoke_scope() {
	new_test_ext().execute_with(|| {
		assert_ok!(PermissionsManager::set_scope(ALICE, SCOPE_1));
		assert_eq!(PermissionsManager::scope(ALICE).unwrap(), SCOPE_1);
		assert_eq!(PermissionsManager::revoke(BOB).unwrap_err(),
				   PermissionError::AccountNotFound);
		assert_ok!(PermissionsManager::revoke(ALICE));
	});
}