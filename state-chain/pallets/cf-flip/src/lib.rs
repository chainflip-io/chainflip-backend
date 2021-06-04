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
//! A [PositiveImbalance] means that there is an excess of funds *in the accounts* that need to be accounted for. This
//! requires a corresponding [NegativeImbalance]. A [NegativeImbalance] means there is an excess of funds *outside of
//! the accounts* that requires a corresponding [PositiveImbalance]. If the imbalances are not canceled against each
//! other, the [imbalances::RevertImbalance] implementation ensures that any excess funds are reverted to their source.
//!
//! ### Example
//! A [burn](Pallet::burn) creates a [PositiveImbalance], since the total issuance has been reduced without
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
//! If the [PositiveImbalance] created by the burn goes out of scope, the change is reverted, effectively minting the
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
use imbalances::{NegativeImbalance, PositiveImbalance};

use codec::{Codec, Decode, Encode};
use sp_runtime::{DispatchError, RuntimeDebug, traits::{
		AtLeast32BitUnsigned, Bounded, CheckedAdd, CheckedSub, MaybeSerializeDeserialize,
		Saturating, Zero,
	}};
use sp_std::{fmt::Debug, prelude::*};

pub use pallet::*;

use crate::imbalances::ImbalanceSource;

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
			+ Codec
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ Debug;

		/// The minimum amount required to keep an account open.
		#[pallet::constant]
		type ExistentialDeposit: Get<Self::Balance>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn account)]
	pub type Account<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, FlipAccount<T::Balance>, ValueQuery>;

	/// The total amount of Flip tokens.
	#[pallet::storage]
	#[pallet::getter(fn total_issuance)]
	pub type TotalIssuance<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

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

type FlipImbalance<T> = SignedImbalance<<T as Config>::Balance, PositiveImbalance<T>>;

impl<T: Config> From<PositiveImbalance<T>> for FlipImbalance<T> {
	fn from(p: PositiveImbalance<T>) -> Self {
		SignedImbalance::Positive(p)
	}
}

impl<T: Config> From<NegativeImbalance<T>> for FlipImbalance<T> {
	fn from(n: NegativeImbalance<T>) -> Self {
		SignedImbalance::Negative(n)
	}
}

impl<T: Config> Pallet<T> {
	/// Debits an account's staked balance. Ignores restricted funds, so can be used for slashing.
	fn debit(account_id: &T::AccountId, amount: T::Balance) -> NegativeImbalance<T> {
		Account::<T>::mutate(account_id, |account| {
			let deducted = account.stake.min(amount);
			account.stake = account.stake.saturating_sub(deducted);
			NegativeImbalance::from_acct(deducted, account_id.clone())
		})
	}

	/// Credits an account with some staked funds. If the amount provided would result in overflow, does nothing.
	fn credit(account_id: &T::AccountId, amount: T::Balance) -> PositiveImbalance<T> {
		Account::<T>::mutate(account_id, |account| {
			match account.stake.checked_add(&amount) {
				Some(result) => {
					account.stake = result;
					PositiveImbalance::from_acct(amount, account_id.clone())
				}
				None => PositiveImbalance::zero(),
			}
		})
	}

	/// Tries to settle an imbalance against an account. Returns `Ok(())` if the whole amount was settled, otherwise
	/// an `Err` containing any remaining imbalance.
	fn try_settle(
		account_id: &T::AccountId,
		imbalance: FlipImbalance<T>,
	) -> Result<(), FlipImbalance<T>> {
		match imbalance {
			SignedImbalance::Positive(p) => {
				let amount = p.peek();
				p.offset(Self::debit(account_id, amount))
					.map(SignedImbalance::Positive)
					.unwrap_or_else(SignedImbalance::Negative)
			}
			SignedImbalance::Negative(n) => {
				let amount = n.peek();
				n.offset(Self::credit(account_id, amount))
					.map(SignedImbalance::Negative)
					.unwrap_or_else(SignedImbalance::Positive)
			}
		}
		.drop_zero()
	}

	/// Settles an imbalance against an account. Any excess is reverted to source according to the rules defined in
	/// [imbalances::RevertImbalance].
	pub fn settle(
		account_id: &T::AccountId,
		imbalance: SignedImbalance<T::Balance, PositiveImbalance<T>>,
	) {
		let settlement_source = ImbalanceSource::Account(account_id.clone());
		let (from, to, amount) = match &imbalance {
			SignedImbalance::Positive(p) => (settlement_source, p.source.clone(), p.peek()),
			SignedImbalance::Negative(n) => (n.source.clone(), settlement_source, n.peek()),
		};

		let (settled, reverted) = Self::try_settle(account_id, imbalance)
			.map(|_| (amount, Zero::zero()))
			.unwrap_or_else(|remaining| {
				let (source, remainder) = match remaining {
					SignedImbalance::Positive(p) => (p.source.clone(), p.peek()),
					SignedImbalance::Negative(n) => (n.source.clone(), n.peek()),
				};
				Self::deposit_event(Event::<T>::RemainingImbalance(source, remainder));
				(amount.saturating_sub(remainder), remainder)
			});

		Self::deposit_event(Event::<T>::BalanceSettled(from, to, settled, reverted))
	}

	/// Decreases total issuance and returns a corresponding imbalance that must be reconciled.
	fn burn(mut amount: T::Balance) -> PositiveImbalance<T> {
		if amount.is_zero() {
			return PositiveImbalance::zero();
		}
		TotalIssuance::<T>::mutate(|issued| {
			*issued = issued.checked_sub(&amount).unwrap_or_else(|| {
				amount = *issued;
				Zero::zero()
			});
		});
		PositiveImbalance::from_burn(amount)
	}

	/// Increases total issuance and returns a corresponding imbalance that must be reconciled.
	fn mint(mut amount: T::Balance) -> NegativeImbalance<T> {
		if amount.is_zero() {
			return NegativeImbalance::zero();
		}
		TotalIssuance::<T>::mutate(|issued| {
			*issued = issued.checked_add(&amount).unwrap_or_else(|| {
				amount = T::Balance::max_value() - *issued;
				T::Balance::max_value()
			})
		});
		NegativeImbalance::from_mint(amount)
	}

	/// Create some funds that have been added to the chain from outside.
	fn bridge_in(amount: T::Balance) -> NegativeImbalance<T> {
		NegativeImbalance::from_offchain(amount)
	}

	/// Send some funds off-chain.
	fn bridge_out(amount: T::Balance) -> PositiveImbalance<T> {
		PositiveImbalance::from_offchain(amount)
	}

	/// 
	pub fn slashable_funds(account_id: &T::AccountId) -> T::Balance {
		Account::<T>::get(account_id).total().saturating_sub(T::ExistentialDeposit::get())
	}
}

impl<T: Config> cf_traits::Emissions for Pallet<T> {
	type AccountId = T::AccountId;
	type Balance = T::Balance;

	fn burn_from(account_id: &Self::AccountId, amount: Self::Balance) {
		let _ = Self::settle(account_id, Self::burn(amount).into());
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
		let _ = Self::settle(account_id, Self::mint(amount).into());
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

	fn credit_stake(
		account_id: &Self::AccountId,
		amount: Self::Balance,
	) -> Result<(), DispatchError> {
		// Make sure account exists.
		ensure!(
			Account::<T>::contains_key(account_id),
			DispatchError::from(Error::<T>::UnknownAccount)
		);

		let incoming = Self::bridge_in(amount);
		Self::settle(account_id, SignedImbalance::Negative(incoming));
		Ok(())
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
