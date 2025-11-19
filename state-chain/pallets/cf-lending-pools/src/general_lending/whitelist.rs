use super::*;

#[derive(Encode, Decode, TypeInfo, PartialEq, Eq, Debug)]
pub enum WhitelistStatus<AccountId> {
	/// All accounts can use lending
	AllowAll,
	/// Only specified accounts can use lending
	AllowSome(BTreeSet<AccountId>),
}

impl<AccountId> Default for WhitelistStatus<AccountId> {
	fn default() -> Self {
		WhitelistStatus::AllowSome(Default::default())
	}
}

impl<AccountId: Ord> WhitelistStatus<AccountId> {
	pub fn apply_update(&mut self, update: WhitelistUpdate<AccountId>) -> DispatchResult {
		match update {
			WhitelistUpdate::SetAllowAll => {
				*self = WhitelistStatus::AllowAll;
			},
			WhitelistUpdate::SetAllowedAccounts(accounts) => {
				*self = WhitelistStatus::AllowSome(accounts);
			},
			WhitelistUpdate::AddAllowedAccounts(accounts) => match self {
				WhitelistStatus::AllowSome(ref mut existing_accounts) => {
					existing_accounts.extend(accounts);
				},
				_ => return Err(DispatchError::Other("bad parameter")),
			},
			WhitelistUpdate::RemoveAllowedAccounts(accounts) => match self {
				WhitelistStatus::AllowSome(ref mut existing_accounts) =>
					for account in accounts {
						existing_accounts.remove(&account);
					},
				_ => return Err(DispatchError::Other("bad parameter")),
			},
		}
		Ok(())
	}

	/// Check if the provided account is allowed.
	pub fn is_allowed(&self, account: &AccountId) -> bool {
		match self {
			WhitelistStatus::AllowAll => true,
			WhitelistStatus::AllowSome(allowed_accounts) => allowed_accounts.contains(account),
		}
	}
}

/// Update to the lending whitelist that can be submitted via config update.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum WhitelistUpdate<AccountId> {
	/// Set the whitelist to allow all accounts
	SetAllowAll,
	/// Set the whitelist to allow only specified accounts
	SetAllowedAccounts(BTreeSet<AccountId>),
	/// Add specified accounts to the whitelist
	AddAllowedAccounts(BTreeSet<AccountId>),
	/// Remove specified accounts from the whitelist
	RemoveAllowedAccounts(BTreeSet<AccountId>),
}

#[cfg(test)]
mod tests {

	use cf_utilities::assert_ok;

	use super::*;

	#[test]
	fn test_set_allow_all() {
		let mut whitelist = WhitelistStatus::<u64>::default();
		assert_eq!(whitelist, WhitelistStatus::AllowSome(Default::default()));

		assert_ok!(whitelist.apply_update(WhitelistUpdate::SetAllowAll));
		assert_eq!(whitelist, WhitelistStatus::AllowAll);
	}

	#[test]
	fn test_set_allowed_accounts() {
		let mut whitelist = WhitelistStatus::<u64>::default();

		let accounts = BTreeSet::from([1, 2, 3]);
		assert_ok!(whitelist.apply_update(WhitelistUpdate::SetAllowedAccounts(accounts.clone())));
		assert_eq!(whitelist, WhitelistStatus::AllowSome(accounts));
	}

	#[test]
	fn test_add_allowed_accounts() {
		let mut whitelist = WhitelistStatus::<u64>::default();

		let initial_accounts = BTreeSet::from([1, 2]);
		assert_ok!(
			whitelist.apply_update(WhitelistUpdate::AddAllowedAccounts(initial_accounts.clone()))
		);
		assert_eq!(whitelist, WhitelistStatus::AllowSome(initial_accounts));

		let new_accounts = BTreeSet::from([2, 3]);

		assert_ok!(whitelist.apply_update(WhitelistUpdate::AddAllowedAccounts(new_accounts)));

		assert_eq!(whitelist, WhitelistStatus::AllowSome(BTreeSet::from([1, 2, 3])));
	}

	#[test]
	fn test_remove_allowed_accounts() {
		let mut whitelist = WhitelistStatus::<u64>::AllowSome(BTreeSet::from([1, 2, 3, 4]));

		// Note: account 5 does not exist, but we won't panic:
		assert_ok!(
			whitelist.apply_update(WhitelistUpdate::RemoveAllowedAccounts(BTreeSet::from([2, 5])))
		);

		assert_eq!(whitelist, WhitelistStatus::AllowSome(BTreeSet::from([1, 3, 4])));
	}

	#[test]
	fn test_is_allowed() {
		{
			let whitelist = WhitelistStatus::<u64>::AllowSome(BTreeSet::from([1, 2, 3, 4]));
			assert!(whitelist.is_allowed(&1));
			assert!(!whitelist.is_allowed(&5));
		}

		{
			let whitelist = WhitelistStatus::<u64>::AllowAll;
			assert!(whitelist.is_allowed(&5));
		}
	}
}
