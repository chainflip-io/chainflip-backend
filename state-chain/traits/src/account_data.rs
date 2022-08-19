use sp_runtime::DispatchError;
use sp_std::marker::PhantomData;

use codec::{Decode, Encode};
use frame_support::{traits::StoredMap, RuntimeDebug};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum AccountType {
	Undefined,
	Validator { state: ValidatorAccountState, is_active_bidder: bool },
	LiquidityProvider,
	Relayer,
}

impl Default for AccountType {
	fn default() -> Self {
		AccountType::Undefined
	}
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum ValidatorAccountState {
	CurrentAuthority,
	/// Historical implies backup too
	HistoricalAuthority,
	Backup,
}

impl Default for ValidatorAccountState {
	fn default() -> Self {
		ValidatorAccountState::Backup
	}
}

impl AccountType {
	pub fn is_authority(&self) -> bool {
		matches!(self, Self::Validator { state: ValidatorAccountState::CurrentAuthority, .. })
	}

	pub fn is_backup(&self) -> bool {
		matches!(
			self,
			Self::Validator { state: ValidatorAccountState::HistoricalAuthority, .. } |
				Self::Validator { state: ValidatorAccountState::Backup, .. }
		)
	}
}

#[derive(Default, PartialEq, Eq, Clone, Copy, Encode, Decode, TypeInfo, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ChainflipAccountData {
	pub account_type: AccountType,
}

pub trait ValidatorAccount {
	type AccountId;

	/// Get the account data for the given account id.
	///
	/// Note: if the account does not exist, returns the [Default].
	fn get(account_id: &Self::AccountId) -> ChainflipAccountData;
	/// Set the node to be a current authority
	fn set_current_authority(account_id: &Self::AccountId);
	/// Sets the authority state to historical
	fn set_historical_authority(account_id: &Self::AccountId);
	/// Sets the current authority to the historical authority, should be called
	/// once the authority has no more active epochs
	fn from_historical_to_backup(account_id: &Self::AccountId);
}

pub struct ChainflipAccountStore<T>(PhantomData<T>);

impl<T: frame_system::Config<AccountData = ChainflipAccountData>> ChainflipAccountStore<T> {
	pub fn try_mutate_account_data<
		R,
		E: Into<DispatchError>,
		F: FnOnce(&mut ChainflipAccountData) -> Result<R, E>,
	>(
		account_id: &T::AccountId,
		f: F,
	) -> Result<R, DispatchError> {
		frame_system::Pallet::<T>::try_mutate_exists(account_id, |maybe_account_data| {
			maybe_account_data
				.as_mut()
				.map_or(Err(DispatchError::CannotLookup), |account_data| {
					f(account_data).map_err(Into::into)
				})
		})
	}

	pub fn mutate_validator_state(
		account_id: &T::AccountId,
		f: impl Fn(&mut ValidatorAccountState),
	) {
		frame_system::Pallet::<T>::mutate(account_id, |account_data| {
			assert!(matches!(account_data.account_type, AccountType::Validator { .. }));
			match account_data.account_type {
				AccountType::Validator { ref mut state, .. } => f(state),
				_ => unreachable!(),
			}
		})
		.unwrap_or_else(|e| log::error!("Mutating account state failed {:?}", e));
	}
}

impl<T: frame_system::Config<AccountData = ChainflipAccountData>> ValidatorAccount
	for ChainflipAccountStore<T>
{
	type AccountId = T::AccountId;

	fn get(account_id: &Self::AccountId) -> ChainflipAccountData {
		frame_system::Pallet::<T>::get(account_id)
	}

	/// Set the account state to Validator.
	///
	/// **Only call this on Validator accounts.**
	fn set_current_authority(account_id: &Self::AccountId) {
		log::debug!("Setting current authority {:?}", account_id);
		Self::mutate_validator_state(account_id, |state| {
			*state = ValidatorAccountState::CurrentAuthority;
		});
	}

	/// Set the account state to HistoricalAuthority.
	///
	/// **Only call this on Validator accounts.**
	fn set_historical_authority(account_id: &Self::AccountId) {
		Self::mutate_validator_state(account_id, |state| {
			*state = ValidatorAccountState::HistoricalAuthority;
		});
	}

	/// Set the account state to Backup.
	///
	/// **Only call this on Validator accounts.**
	fn from_historical_to_backup(account_id: &Self::AccountId) {
		Self::mutate_validator_state(account_id, |state| match state {
			ValidatorAccountState::HistoricalAuthority => {
				*state = ValidatorAccountState::Backup;
			},
			_ => {
				const ERROR_MESSAGE: &str = "Attempted to transition to backup from historical, on a non-historical authority";
				log::error!("{}", ERROR_MESSAGE);
				#[cfg(test)]
				panic!("{}", ERROR_MESSAGE);
			},
		});
	}
}
