#![cfg_attr(not(feature = "std"), no_std)]

//! # Chainflip Alive Module
//!
//! A module to manage liveness for the Chainflip State Chain
//!
//! - [`Config`]
//! - [`Call`]
//! - [`Module`]
//!
//! ## Overview
//!
//! ## Terminology
//! - **Liveness:**
//!

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;
use sp_std::prelude::*;
use frame_support::pallet_prelude::*;
use cf_traits::{Reporter, Action, Judgement, JudgementError};
use sp_core::crypto::Ss58AddressFormat::JupiterAccount;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;
	use codec::FullCodec;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// The action type
		type Action: Action + Member + FullCodec + Default;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	/// Storage of account against actions
	#[pallet::storage]
	#[pallet::getter(fn actions)]
	pub(super) type Actions<T: Config> = StorageMap<_, Identity, T::AccountId, Vec<T::Action>>;

	/// Storage of account last known liveliness
	#[pallet::storage]
	#[pallet::getter(fn last_know_liveliness)]
	pub(super) type LastKnownLiveliness<T: Config> = StorageMap<_, Identity, T::AccountId, T::BlockNumber>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {}

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}
}

impl<T: Config> Reporter for Pallet<T> {
	type AccountId = T::AccountId;
	type Action = T::Action;

	/// Report an action from an account.  We store the action to storage and mark this as the last
	/// block number we have seen activity from this account
	fn report(account_id: &Self::AccountId, action: Self::Action) -> Result<(), JudgementError> {
		<Actions<T>>::try_mutate(account_id, |actions| {
			match actions.as_mut() {
				Some(actions) => {
					actions.push(action);

					<LastKnownLiveliness<T>>::try_mutate(account_id, |last| {
						match last.as_mut() {
							Some(last) => {
								*last = <frame_system::Pallet<T>>::block_number();
								Ok(())
							},
							None => {
								Err(JudgementError::AccountNotFound)
							}
						}
					})
				},
				None => {
					Err(JudgementError::AccountNotFound)
				}
			}
		})
	}
}

impl<T: Config> Judgement<Pallet<T>, T::BlockNumber> for Pallet<T> {
	fn liveliness(account_id: &T::AccountId) -> Result<T::BlockNumber, JudgementError> {
		Self::liveliness(account_id).or(JudgementError::AccountNotFound)
	}

	fn report_for(account_id: &T::AccountId) -> Result<Vec<<Pallet<T> as Reporter>::Action>, JudgementError> {
		Self::actions(account_id).ok_or(JudgementError::AccountNotFound)
	}

	fn clean_all(account_id: &T::AccountId) -> Result<(), JudgementError> {
		// <LastKnownLiveliness<T>>::try_
		todo!()
	}
}
