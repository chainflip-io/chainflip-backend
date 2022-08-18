use sp_std::marker::PhantomData;

use codec::{Decode, Encode};
use frame_support::{traits::StoredMap, RuntimeDebug};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum ValidatorAccountState {
	CurrentAuthority,
	/// Historical implies backup too
	HistoricalAuthority,
	Backup,
}

impl ValidatorAccountState {
	pub fn is_authority(&self) -> bool {
		matches!(self, ValidatorAccountState::CurrentAuthority)
	}

	pub fn is_backup(&self) -> bool {
		matches!(self, ValidatorAccountState::HistoricalAuthority | ValidatorAccountState::Backup)
	}
}

// TODO: Just use the AccountState
#[derive(PartialEq, Eq, Clone, Copy, Encode, Decode, TypeInfo, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ChainflipAccountData {
	pub state: ValidatorAccountState,
}

impl Default for ChainflipAccountData {
	fn default() -> Self {
		ChainflipAccountData { state: ValidatorAccountState::Backup }
	}
}

pub trait ChainflipAccount {
	type AccountId;

	/// Get the account data for the given account id.
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

impl<T: frame_system::Config<AccountData = ChainflipAccountData>> ChainflipAccount
	for ChainflipAccountStore<T>
{
	type AccountId = T::AccountId;

	fn get(account_id: &Self::AccountId) -> ChainflipAccountData {
		frame_system::Pallet::<T>::get(account_id)
	}

	/// Set the last epoch number and set the account state to Validator
	fn set_current_authority(account_id: &Self::AccountId) {
		log::debug!("Setting current authority {:?}", account_id);
		frame_system::Pallet::<T>::mutate(account_id, |account_data| {
			account_data.state = ValidatorAccountState::CurrentAuthority;
		})
		.unwrap_or_else(|e| log::error!("Mutating account state failed {:?}", e));
	}

	fn set_historical_authority(account_id: &Self::AccountId) {
		frame_system::Pallet::<T>::mutate(account_id, |account_data| {
			account_data.state = ValidatorAccountState::HistoricalAuthority;
		})
		.unwrap_or_else(|e| log::error!("Mutating account state failed {:?}", e));
	}

	fn from_historical_to_backup(account_id: &Self::AccountId) {
		frame_system::Pallet::<T>::mutate(account_id, |account_data| match account_data.state {
			ValidatorAccountState::HistoricalAuthority => {
				account_data.state = ValidatorAccountState::Backup;
			},
			_ => {
				const ERROR_MESSAGE: &str = "Attempted to transition to backup from historical, on a non-historical authority";
				log::error!("{}", ERROR_MESSAGE);
				#[cfg(test)]
				panic!("{}", ERROR_MESSAGE);
			},
		})
		.unwrap_or_else(|e| log::error!("Mutating account state failed {:?}", e));
	}
}
