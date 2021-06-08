#![cfg_attr(not(feature = "std"), no_std)]

//! Flip Token Pallet
//!
//! Loosely based on Parity's Balances pallet.
//!
//! Provides some low-level helpers for creating balance updates that maintain the accounting of funds.
//!
//! Exposes higher-level operations via the [cf_traits::StakeTransfer] and [cf_traits::Emissions] traits.
//!
//! ## Imbalances
//!
//! Imbalances are not very intuitive but the idea is this: if you want to manipulate the balance of FLIP in the
//! system, there always needs to be an equal and opposite
//!
//! A [Deficit] means that there is an excess of funds *in the accounts* that needs to be reconciled. This
//! requires a corresponding [Surplus]. A [Surplus] means there is an excess of funds *outside of
//! the accounts* that requires a corresponding [Deficit]. If the imbalances are not canceled against each
//! other, the [imbalances::RevertImbalance] implementation ensures that any excess funds are reverted to their source.
//!
//! ### Example
//! A [burn](Pallet::burn) creates a [Deficit], since the total issuance has been reduced without
//! changing the amounts held in the accounts. The accounts hold *more*  funds than there should be, so the imbalance is
//! *positive*. This can be counteracted by [debiting](Pallet::debit) an account. The net effect is as if the account's
//! tokens were burned.
//!
//! ```
//! // let (account_id, amount) = (something);
//! let burn_imbalance = Pallet::<T>::burn(&account_id, amount);
//! let debit_imbalance = Pallet::<T>::debit(&account_id, amount);
//! burn_imbalance.offset(debit_imbalance);
//!
//! // Alternatively:
//! Pallet::<T>::burn(account_id, amount).offset(Pallet::<T>::debit(&account_id, amount));
//!
//! // Or even:
//! Pallet::<T>::settle(
//!    &account_id,
//!    Pallet::<T>::burn(&account_id, amount).into()
//! )
//! ```
//!
//! If the [Deficit] created by the burn goes out of scope, the change is reverted, effectively minting the
//! tokens and adding them back to the total issuance.

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

mod imbalances;

use frame_support::{
	ensure,
	traits::{Get, Imbalance, SignedImbalance},
};
use imbalances::{Surplus, Deficit};

use codec::{Decode, Encode};
use sp_runtime::{DispatchError, RuntimeDebug, traits::{
		AtLeast32BitUnsigned, MaybeSerializeDeserialize,
		Saturating, Zero,
	}};
use sp_std::{fmt::Debug, prelude::*};

pub use pallet::*;

pub use crate::imbalances::ImbalanceSource;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The balance of an account.
		type Balance: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ Debug;

		/// The minimum amount required to keep an account open.
		#[pallet::constant]
		type ExistentialDeposit: Get<Self::Balance>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Funds belonging to on-chain accounts.
	#[pallet::storage]
	#[pallet::getter(fn account)]
	pub type Account<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, FlipAccount<T::Balance>, ValueQuery>;

	/// The total number of tokens issued.
	#[pallet::storage]
	#[pallet::getter(fn total_issuance)]
	pub type TotalIssuance<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

	/// The total number of tokens currently on-chain.
	#[pallet::storage]
	#[pallet::getter(fn onchain_funds)]
	pub type OnchainFunds<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

	/// The number of tokens currently off-chain.
	#[pallet::storage]
	#[pallet::getter(fn offchain_funds)]
	pub type OffchainFunds<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

	#[pallet::event]
	#[pallet::metadata(T::AccountId = "AccountId")]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Some imbalance could not be settled and the remainder will be reverted. [reverted_to, amount]
		RemainingImbalance(ImbalanceSource<T::AccountId>, T::Balance),

		/// An imbalance has been settled. [source, dest, amount_settled, amount_reverted]
		BalanceSettled(
			ImbalanceSource<T::AccountId>,
			ImbalanceSource<T::AccountId>,
			T::Balance,
			T::Balance,
		),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Not enough liquid funds.
		InsufficientLiquidity,

		/// Not enough funds.
		InsufficientFunds,

		/// Some operations can only be performed on existing accounts.
		UnknownAccount,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		// No external calls for this pallet.
	}
}

/// All balance information for a Flip account.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Default, RuntimeDebug)]
pub struct FlipAccount<Amount> {
	/// Amount that has been staked and is considered as a bid in the validator auction. Includes any bonded
	/// and vesting funds. Excludes any funds in the process of being claimed.
	stake: Amount,

	/// Amount that is bonded due to validator status and cannot be withdrawn.
	validator_bond: Amount,

	/// Amount of tokens that originated from a vesting contract - these are subject to special treatment when
	/// withdrawn.
	vesting: Amount,
}

impl<Balance: Saturating + Copy + Ord> FlipAccount<Balance> {
	/// The total balance excludes any funds that are in a pending claim request.
	fn total(&self) -> Balance {
		self.stake
	}

	/// Includes vesting funds, excludes the bond.
	fn liquid(&self) -> Balance {
		self.stake.saturating_sub(self.validator_bond)
	}

	/// Funds that have no vesting or bonding restrictions.
	fn free_to_claim(&self) -> Balance {
		self.stake
			.saturating_sub(self.validator_bond.max(self.vesting))
	}
}

type FlipImbalance<T> = SignedImbalance<<T as Config>::Balance, Surplus<T>>;

impl<T: Config> From<Surplus<T>> for FlipImbalance<T> {
	fn from(surplus: Surplus<T>) -> Self {
		SignedImbalance::Positive(surplus)
	}
}

impl<T: Config> From<Deficit<T>> for FlipImbalance<T> {
	fn from(deficit: Deficit<T>) -> Self {
		SignedImbalance::Negative(deficit)
	}
}

impl<T: Config> Pallet<T> {
	/// Total funds stored in an account.
	pub fn total_balance_of(account_id: &T::AccountId) -> T::Balance {
		Account::<T>::get(account_id).total()
	}

	/// Sets the validator bond for an account.
	pub fn set_validator_bond(account_id: &T::AccountId, amount: T::Balance) {
		Account::<T>::mutate_exists(account_id, |maybe_account| {
			match maybe_account.as_mut() {
				Some(account) => account.validator_bond = amount,
				None => {},
			}
		})
	}

	/// Slashable funds for an account.
	pub fn slashable_funds(account_id: &T::AccountId) -> T::Balance {
		Account::<T>::get(account_id).total().saturating_sub(T::ExistentialDeposit::get())
	}

	/// Debits an account's staked balance. Ignores restricted funds, so can be used for slashing.
	///
	/// Debiting creates a surplus since we now have some funds that need to be allocated somewhere.
	fn debit(account_id: &T::AccountId, amount: T::Balance) -> Surplus<T> {
		Surplus::from_acct(account_id, amount)
	}

	/// Credits an account with some staked funds. If the amount provided would result in overflow, does nothing.
	/// 
	/// Crediting an account creates a deficit since we need to take the credited funds from somewhere. In a sense we
	/// have spent money we don't have.
	fn credit(account_id: &T::AccountId, amount: T::Balance) -> Deficit<T> {
		Deficit::from_acct(account_id, amount)
	}

	/// Tries to settle an imbalance against an account. Returns `Ok(())` if the whole amount was settled, otherwise
	/// an `Err` containing any remaining imbalance.
	fn try_settle(
		account_id: &T::AccountId,
		imbalance: FlipImbalance<T>,
	) -> Result<(), FlipImbalance<T>> {
		match imbalance {
			SignedImbalance::Positive(surplus) => {
				let amount = surplus.peek();
				surplus.offset(Self::credit(account_id, amount))
					.map(SignedImbalance::Positive)
					.unwrap_or_else(SignedImbalance::Negative)
			}
			SignedImbalance::Negative(deficit) => {
				let amount = deficit.peek();
				deficit.offset(Self::debit(account_id, amount))
					.map(SignedImbalance::Negative)
					.unwrap_or_else(SignedImbalance::Positive)
			}
		}
		.drop_zero()
	}

	/// Settles an imbalance against an account. Any excess is reverted to source according to the rules defined in
	/// [imbalances::RevertImbalance].
	pub fn settle(account_id: &T::AccountId, imbalance: FlipImbalance<T>) {
		let settlement_source = ImbalanceSource::Account(account_id.clone());
		let (from, to, amount) = match &imbalance {
			SignedImbalance::Positive(surplus) => (surplus.source.clone(), settlement_source, surplus.peek()),
			SignedImbalance::Negative(deficit) => (settlement_source, deficit.source.clone(), deficit.peek()),
		};

		let (settled, reverted) = Self::try_settle(account_id, imbalance)
			// In the case of success, nothing to revert.
			.map(|_| (amount, Zero::zero()))
			// In case of failure, calculate the remainder.
			.unwrap_or_else(|remaining| {
				// Note `remaining` will be dropped and automatically reverted at the end of this block.
				let (source, remainder) = match remaining {
					SignedImbalance::Positive(surplus) => (surplus.source.clone(), surplus.peek()),
					SignedImbalance::Negative(deficit) => (deficit.source.clone(), deficit.peek()),
				};
				Self::deposit_event(Event::<T>::RemainingImbalance(source, remainder));
				(amount.saturating_sub(remainder), remainder)
			});

		Self::deposit_event(Event::<T>::BalanceSettled(from, to, settled, reverted))
	}

	/// Decreases total issuance and returns a corresponding imbalance that must be reconciled.
	fn burn(amount: T::Balance) -> Deficit<T> {
		Deficit::from_burn(amount)
	}

	/// Increases total issuance and returns a corresponding imbalance that must be reconciled.
	fn mint(amount: T::Balance) -> Surplus<T> {
		Surplus::from_mint(amount)
	}

	/// Create some funds that have been added to the chain from outside.
	fn bridge_in(amount: T::Balance) -> Surplus<T> {
		Surplus::from_offchain(amount)
	}

	/// Send some funds off-chain.
	fn bridge_out(amount: T::Balance) -> Deficit<T> {
		Deficit::from_offchain(amount)
	}
}

impl<T: Config> cf_traits::Emissions for Pallet<T> {
	type AccountId = T::AccountId;
	type Balance = T::Balance;

	fn burn_from(account_id: &Self::AccountId, amount: Self::Balance) {
		Self::settle(account_id, Self::burn(amount).into());
	}

	fn try_burn_from(
		account_id: &Self::AccountId,
		amount: Self::Balance,
	) -> Result<(), DispatchError> {
		ensure!(
			amount <= Self::slashable_funds(account_id),
			DispatchError::from(Error::<T>::InsufficientFunds)
		);
		Self::burn_from(account_id, amount);
		Ok(())
	}

	fn mint_to(account_id: &Self::AccountId, amount: Self::Balance) {
		Self::settle(account_id, Self::mint(amount).into());
	}

	fn vaporise(amount: Self::Balance) {
		let _ = Self::burn(amount).offset(Self::bridge_in(amount));
	}

	fn total_issuance() -> Self::Balance {
		Self::total_issuance()
	}
}

impl<T: Config> cf_traits::StakeTransfer for Pallet<T> {
	type AccountId = T::AccountId;
	type Balance = T::Balance;

	fn stakeable_balance(account_id: &T::AccountId) -> Self::Balance {
		Account::<T>::get(account_id).total()
	}

	fn credit_stake(account_id: &Self::AccountId, amount: Self::Balance) -> Self::Balance {
		let incoming = Self::bridge_in(amount);
		Self::settle(account_id, SignedImbalance::Positive(incoming));
		Self::total_balance_of(account_id)
	}

	fn try_claim(
		account_id: &Self::AccountId,
		amount: Self::Balance,
	) -> Result<(), DispatchError> {
		ensure!(
			amount <= Account::<T>::get(account_id).free_to_claim(),
			DispatchError::from(Error::<T>::InsufficientLiquidity)
		);

		// TODO: add explicit claims source.
		Self::settle(account_id, Self::bridge_out(amount).into());
		Ok(())
	}

	fn try_claim_vesting(
		account_id: &Self::AccountId,
		amount: Self::Balance,
	) -> Result<(), DispatchError> {
		ensure!(
			amount <= Account::<T>::get(account_id).liquid(),
			DispatchError::from(Error::<T>::InsufficientLiquidity)
		);

		Self::settle(account_id, Self::bridge_out(amount).into());
		Ok(())
	}

	fn settle_claim(_amount: Self::Balance) {
		// Nothing to do.
	}

	fn revert_claim(account_id: &Self::AccountId, amount: Self::Balance) {
		Self::settle(account_id, Self::bridge_in(amount).into());
		// claim reverts automatically when dropped
	}
}
