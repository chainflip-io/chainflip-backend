#![cfg_attr(not(feature = "std"), no_std)]

//! Flip Token Pallet
//!
//! Loosely based on Parity's Balances pallet.
//!
//! Provides some low-level helpers for creating balance updates that maintain the accounting of funds.
//!
//! Exposes higher-level operations via the [cf_traits::StakeTransfer] and [cf_traits::Issuance] traits.
//!
//! ## Imbalances
//!
//! Imbalances are not very intuitive but the idea is this: if you want to manipulate the balance of FLIP in the
//! system, there always need to be two equal and opposite [Imbalance]s. Any excess is reverted according to the
//! implementation of [imbalances::RevertImbalance] when the imbalance is dropped.
//!
//! A [Deficit] means that there is an excess of funds *in the accounts* that needs to be reconciled. Either we have
//! credited some funds to an account, or we have debited funds from some external source without putting them anywhere.
//! Think of it like this: if we credit an account, we need to pay for it somehow. Either by debiting from another, or
//! by minting some tokens, or by bridging them from outside (aka. staking).
//!
//! A [Surplus] is (unsurprisingly) the opposite: it means there is an excess of funds *outside of the accounts*. Maybe
//! an account has been debited some amount, or we have minted some tokens. These to be allocated somewhere.
//!
//! ### Example
//!
//! A [burn](Pallet::burn) creates a [Deficit]: the total issuance has been reduced so we need a [Surplus] from
//! somewhere that we can offset against this. Usually, we want to debit an account to burn (slash) funds. We may also
//! want to burn funds that are held in trading pools, for example. In this case we might withdraw from a pool to create
//! a surplus to offset the burn (not implemented yet).
//!
//! If the [Deficit] created by the burn goes out of scope without being offset, the change is reverted, effectively
//! minting the tokens and adding them back to the total issuance.

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

mod imbalances;
mod on_charge_transaction;

pub use imbalances::{Deficit, Surplus};
pub use on_charge_transaction::FlipTransactionPayment;

use frame_support::{
	ensure,
	traits::{Get, Imbalance, OnKilledAccount, SignedImbalance},
};

use codec::{Decode, Encode};
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, MaybeSerializeDeserialize, Saturating, Zero},
	DispatchError, RuntimeDebug,
};
use sp_std::{fmt::Debug, marker::PhantomData, prelude::*};

pub use pallet::*;

pub use crate::imbalances::ImbalanceSource;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// A 4-byte identifier for different reserves.
	pub type ReserveId = [u8; 4];

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

	/// Funds belonging to on-chain reserves.
	#[pallet::storage]
	#[pallet::getter(fn reserve)]
	pub type Reserve<T: Config> = StorageMap<_, Blake2_128Concat, ReserveId, T::Balance, ValueQuery>;

	/// The total number of tokens issued.
	#[pallet::storage]
	#[pallet::getter(fn total_issuance)]
	pub type TotalIssuance<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

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

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub total_issuance: T::Balance,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				total_issuance: Zero::zero(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			TotalIssuance::<T>::set(self.total_issuance);
			OffchainFunds::<T>::set(self.total_issuance);
		}
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
}

impl<Balance: Saturating + Copy + Ord> FlipAccount<Balance> {
	/// The total balance excludes any funds that are in a pending claim request.
	fn total(&self) -> Balance {
		self.stake
	}

	/// Excludes the bond.
	fn liquid(&self) -> Balance {
		self.stake.saturating_sub(self.validator_bond)
	}
}

/// Convenient alias for [SignedImbalance].
pub type FlipImbalance<T> = SignedImbalance<<T as Config>::Balance, Surplus<T>>;

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
	/// The total number of tokens currently on-chain.
	pub fn onchain_funds() -> T::Balance {
		TotalIssuance::<T>::get() - OffchainFunds::<T>::get()
	}

	/// Total funds stored in an account.
	pub fn total_balance_of(account_id: &T::AccountId) -> T::Balance {
		Account::<T>::get(account_id).total()
	}

	/// Amount of funds allocated to a [Reserve].
	pub fn reserved_balance(reserve_id: ReserveId) -> T::Balance {
		Reserve::<T>::get(reserve_id)
	}

	/// Sets the validator bond for an account.
	pub fn set_validator_bond(account_id: &T::AccountId, amount: T::Balance) {
		Account::<T>::mutate_exists(account_id, |maybe_account| match maybe_account.as_mut() {
			Some(account) => account.validator_bond = amount,
			None => {}
		})
	}

	/// Slashable funds for an account.
	pub fn slashable_funds(account_id: &T::AccountId) -> T::Balance {
		Account::<T>::get(account_id)
			.total()
			.saturating_sub(T::ExistentialDeposit::get())
	}

	/// Debits an account's staked balance. 
	///
	/// *Warning:* Creates the flip account if it doesn't exist already, but *doesn't* ensure that the `System`-level 
	/// account exists so should only be used with accounts that are known to exist.
	///
	/// Use `try_debit` instead when the existence of the account is unsure.
	///
	/// Debiting creates a surplus since we now have some funds that need to be allocated somewhere.
	pub fn debit(account_id: &T::AccountId, amount: T::Balance) -> Surplus<T> {
		Surplus::from_acct(account_id, amount)
	}

	/// Debits an account's staked balance, if sufficient funds are available, otherwise returns `None`. Unlike [debit],
	/// does not create the account if it doesn't exist.
	pub fn try_debit(account_id: &T::AccountId, amount: T::Balance) -> Option<Surplus<T>> {
		Surplus::try_from_acct(account_id, amount)
	}

	/// Credits an account with some staked funds. If the amount provided would result in overflow, does nothing.
	///
	/// Crediting an account creates a deficit since we need to take the credited funds from somewhere. In a sense we
	/// have spent money we don't have.
	pub fn credit(account_id: &T::AccountId, amount: T::Balance) -> Deficit<T> {
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
				surplus
					.offset(Self::credit(account_id, amount))
					.map(SignedImbalance::Positive)
					.unwrap_or_else(SignedImbalance::Negative)
			}
			SignedImbalance::Negative(deficit) => {
				let amount = deficit.peek();
				deficit
					.offset(Self::debit(account_id, amount))
					.map(SignedImbalance::Negative)
					.unwrap_or_else(SignedImbalance::Positive)
			}
		}
		.drop_zero()
	}

	/// Settles an imbalance against an account. Any excess is reverted to source according to the rules defined in
	/// [imbalances::RevertImbalance].
	pub fn settle(account_id: &T::AccountId, imbalance: FlipImbalance<T>) {
		let settlement_source = ImbalanceSource::from_acct(account_id.clone());
		let (from, to, amount) = match &imbalance {
			SignedImbalance::Positive(surplus) => {
				(surplus.source.clone(), settlement_source, surplus.peek())
			}
			SignedImbalance::Negative(deficit) => {
				(settlement_source, deficit.source.clone(), deficit.peek())
			}
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

	/// Withdraws *up to* `amount` from a reserve.
	///
	/// *Warning:* if the reserve does not exist, it will be created as a side effect. 
	pub fn withdraw_reserves(reserve_id: ReserveId, amount: T::Balance) -> Surplus<T> {
		Surplus::from_reserve(reserve_id, amount)
	}

	/// Tries to withdraw funds from a reserve. Fails if the reserve doesn't exist or has insufficient funds.
	pub fn try_withdraw_reserves(reserve_id: ReserveId, amount: T::Balance) -> Option<Surplus<T>> {
		Surplus::try_from_reserve(reserve_id, amount)
	}

	/// Deposit `amount` into the reserve identified by a `reserve_id`. Creates the reserve it it doesn't exist already.
	pub fn deposit_reserves(reserve_id: ReserveId, amount: T::Balance) -> Deficit<T> {
		Deficit::from_reserve(reserve_id, amount)
	}
}
pub struct FlipIssuance<T>(PhantomData<T>);

impl<T: Config> cf_traits::Issuance for FlipIssuance<T> {
	type AccountId = T::AccountId;
	type Balance = T::Balance;
	type Surplus = Surplus<T>;

	fn mint(amount: Self::Balance) -> Surplus<T> {
		Pallet::<T>::mint(amount)
	}

	fn burn(amount: Self::Balance) -> Deficit<T> {
		Pallet::<T>::burn(amount)
	}

	fn total_issuance() -> Self::Balance {
		Pallet::<T>::total_issuance()
	}
}

impl<T: Config> cf_traits::StakeTransfer for Pallet<T> {
	type AccountId = T::AccountId;
	type Balance = T::Balance;

	fn stakeable_balance(account_id: &T::AccountId) -> Self::Balance {
		Account::<T>::get(account_id).total()
	}

	fn claimable_balance(account_id: &T::AccountId) -> Self::Balance {
		Account::<T>::get(account_id).liquid()
	}

	fn credit_stake(account_id: &Self::AccountId, amount: Self::Balance) -> Self::Balance {
		let incoming = Self::bridge_in(amount);
		Self::settle(account_id, SignedImbalance::Positive(incoming));
		Self::total_balance_of(account_id)
	}

	fn try_claim(account_id: &Self::AccountId, amount: Self::Balance) -> Result<(), DispatchError> {
		ensure!(
			amount <= Self::claimable_balance(account_id),
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

/// Implementation of `OnKilledAccount` ensures that we reconcile any flip dust remaining in the account by burning it.
impl<T: Config> OnKilledAccount<T::AccountId> for Pallet<T> {
	fn on_killed_account(account_id: &T::AccountId) {
		let dust = Self::total_balance_of(account_id);
		Self::settle(account_id, Self::burn(dust).into());
		Account::<T>::remove(account_id);
	}
}
