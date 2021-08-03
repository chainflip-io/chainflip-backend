#![cfg_attr(not(feature = "std"), no_std)]

//! # Chainflip Alive Module
//!
//! A module to manage liveliness for the Chainflip State Chain
//!
//! - [`Config`]
//! - [`Call`]
//! - [`Module`]
//!
//! ## Overview
//! The module contains functionality to track behaviour of accounts and provides a good indication
//! of an account's liveliness.  The rules determining what is good or bad behaviour is outside the
//! scope of this pallet and this pallet is solely responsible in tracking and storing the
//! behavioural data. Actions, or behaviours, are stored and indexed by the account id of the
//! validator. The last behaviour recorded for a validator would be used as its last know 'live'
//! time and hence serve as a strong indicator of its liveliness in terms of an operational node.
//! In order to prevent spamming a whitelist of accounts is controlled in which before reporting
//! behaviour for an account the account has to be explicitly added using `add_account()` and
//! removed with `remove_account()`.
//!
//! ## Terminology
//! - **Liveness:** - the last block number we have had a report on an account for
//!

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use cf_traits::{Judgement, JudgementError, Reporter};
use frame_support::pallet_prelude::*;
pub use pallet::*;
use sp_std::prelude::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use codec::FullCodec;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The action type
		type Action: Member + FullCodec + Default;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	/// Storage of account against liveliness and actions
	#[pallet::storage]
	#[pallet::getter(fn actions)]
	pub(super) type Actions<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, (T::BlockNumber, Vec<T::Action>)>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {}
}

impl<T: Config> Pallet<T> {
	fn account_exists(account_id: &T::AccountId) -> bool {
		Actions::<T>::contains_key(account_id)
	}
}

impl<T: Config> Reporter for Pallet<T> {
	type AccountId = T::AccountId;
	type Action = T::Action;

	/// Add an account to our whitelist
	///
	/// An account is added with an empty report and its liveliness is recorded at this block number
	/// A `JudgementError::AccountExits` if this account is already whitelisted
	fn add_account(account_id: &Self::AccountId) -> Result<(), JudgementError> {
		if Self::account_exists(account_id) {
			return Err(JudgementError::AccountExists);
		}
		<Actions<T>>::insert(
			account_id,
			(T::BlockNumber::default(), Vec::<T::Action>::new()),
		);
		Ok(())
	}

	/// Remove an account from our whitelist
	///
	/// An account is removed and its liveliness is reset
	/// A `JudgementError::AccountExits` if this account is not already whitelisted
	fn remove_account(account_id: &Self::AccountId) -> Result<(), JudgementError> {
		if Self::account_exists(account_id) {
			<Actions<T>>::remove(account_id);
			return Ok(());
		}

		Err(JudgementError::AccountNotFound)
	}

	/// Report an action from an account.
	///
	/// We store the action and record the current block number as liveliness for this account
	fn report(account_id: &Self::AccountId, action: Self::Action) -> Result<(), JudgementError> {
		<Actions<T>>::try_mutate(account_id, |actions| match actions.as_mut() {
			Some((block_number, actions)) => {
				actions.push(action);
				*block_number = <frame_system::Pallet<T>>::block_number();
				Ok(())
			}
			None => Err(JudgementError::AccountNotFound),
		})
	}
}

impl<T: Config> Judgement<Pallet<T>, T::BlockNumber> for Pallet<T> {
	/// Return the liveliness of an account
	///
	/// Liveliness is defined as the last block number
	/// An error returns if the account is not whitelisted
	fn liveliness(account_id: &T::AccountId) -> Result<T::BlockNumber, JudgementError> {
		Actions::<T>::get(account_id)
			.map(|(block_number, _)| block_number)
			.ok_or(JudgementError::AccountNotFound)
	}

	/// Return a report on this account
	///
	/// The report consists of a vector of behaviours recorded
	/// An error returns if the account is not whitelisted
	fn report_for(
		account_id: &T::AccountId,
	) -> Result<Vec<<Pallet<T> as Reporter>::Action>, JudgementError> {
		Actions::<T>::get(account_id)
			.map(|(_, judgements)| judgements)
			.ok_or(JudgementError::AccountNotFound)
	}

	/// Clean out the report for this account
	///
	/// The report is cleared for this account
	/// An error returns is this account is not whitelisted
	fn clean_all(account_id: &T::AccountId) -> Result<(), JudgementError> {
		<Actions<T>>::mutate(account_id, |actions| match actions.as_mut() {
			Some((_, actions)) => {
				actions.clear();
				Ok(())
			},
			None => Err(JudgementError::AccountNotFound)
		})
	}
}
