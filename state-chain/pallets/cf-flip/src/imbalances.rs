//! This is based on the imbalances modules from the balances pallet.

// wrapping these imbalances in a private module is necessary to ensure absolute privacy
// of the inner member.

use crate::{self as Flip, Config};
use frame_support::traits::{Imbalance, TryDrop};
use sp_runtime::{
	traits::{Saturating, Zero},
	RuntimeDebug,
};
use sp_std::{mem, result};

#[derive(RuntimeDebug, PartialEq, Eq, Clone)]
pub enum ImbalanceSource<AccountId> {
	External,
	Account(AccountId),
	Emissions
}

/// Opaque, move-only struct with private fields that serves as a token denoting that funds have been added from
/// *somewhere*, and that we need to account for this by cancelling it against a corresponding [Deficit].
#[must_use]
#[derive(RuntimeDebug, PartialEq, Eq)]
pub struct Surplus<T: Config> {
	amount: T::Balance,
	pub(super) source: ImbalanceSource<T::AccountId>,
}

impl<T: Config> Surplus<T> {
	/// Create a new positive imbalance.
	pub(super) fn new(amount: T::Balance, source: ImbalanceSource<T::AccountId>) -> Self {
		Surplus { amount, source, }
	}

	pub fn from_burn(amount: T::Balance) -> Self {
		Self::new(amount, ImbalanceSource::Emissions)
	}

	pub fn from_acct(amount: T::Balance, account_id: T::AccountId) -> Self {
		Self::new(amount, ImbalanceSource::Account(account_id))
	}

	pub fn from_offchain(amount: T::Balance) -> Self {
		Self::new(amount, ImbalanceSource::External)
	}
}

/// Opaque, move-only struct with private fields that serves as a token denoting that funds have been removed to
/// *somewhere*, and that we need to account for this by cancelling it against a correspnding [Surplus].
#[must_use]
#[derive(RuntimeDebug, PartialEq, Eq)]
pub struct Deficit<T: Config> {
	amount: T::Balance,
	pub(super) source: ImbalanceSource<T::AccountId>,
}

impl<T: Config> Deficit<T> {
	/// Create a new negative imbalance from a balance.
	pub(super) fn new(amount: T::Balance, source: ImbalanceSource<T::AccountId>) -> Self {
		Deficit { amount, source }
	}

	pub fn from_mint(amount: T::Balance) -> Self {
		Self::new(amount, ImbalanceSource::Emissions)
	}

	pub fn from_acct(amount: T::Balance, account_id: T::AccountId) -> Self {
		Self::new(amount, ImbalanceSource::Account(account_id))
	}

	pub fn from_offchain(amount: T::Balance) -> Self {
		Self::new(amount, ImbalanceSource::External)
	}
}

impl<T: Config> TryDrop for Surplus<T> {
	fn try_drop(self) -> result::Result<(), Self> {
		self.drop_zero()
	}
}

impl<T: Config> Imbalance<T::Balance> for Surplus<T> {
	type Opposite = Deficit<T>;

	fn zero() -> Self {
		Self {
			amount: Zero::zero(),
			source: ImbalanceSource::Emissions,
		}
	}
	fn drop_zero(self) -> result::Result<(), Self> {
		if self.amount.is_zero() {
			Ok(())
		} else {
			Err(self)
		}
	}
	fn split(self, amount: T::Balance) -> (Self, Self) {
		let first = self.amount.min(amount);
		let second = self.amount - first;
		let source = self.source.clone();

		mem::forget(self);
		(Self::new(first, source.clone()), Self::new(second, source))
	}
	/// Performs the merge only if sources match. Otherwise drops `other` and returns `self`.
	fn merge(mut self, other: Self) -> Self {
		self.subsume(other);
		self
	}
	/// Similarly to `merge` only succeeds if the sources match, otherwise drops `other`.
	fn subsume(&mut self, other: Self) {
		if self.source == other.source {
			self.amount = self.amount.saturating_add(other.amount);
			mem::forget(other);
		}
	}
	fn offset(self, other: Self::Opposite) -> result::Result<Self, Self::Opposite> {
		let (a, b) = (self.amount, other.amount);
		let (s_a, s_b) = (self.source.clone(), other.source.clone());
		mem::forget((self, other));

		if a >= b {
			Ok(Self::new(a - b, s_a))
		} else {
			Err(Deficit::new(b - a, s_b))
		}
	}
	fn peek(&self) -> T::Balance {
		self.amount
	}
}

impl<T: Config> TryDrop for Deficit<T> {
	fn try_drop(self) -> result::Result<(), Self> {
		self.drop_zero()
	}
}

impl<T: Config> Imbalance<T::Balance> for Deficit<T> {
	type Opposite = Surplus<T>;

	fn zero() -> Self {
		Self {
			amount: Zero::zero(),
			source: ImbalanceSource::Emissions,
		}
	}
	fn drop_zero(self) -> result::Result<(), Self> {
		if self.amount.is_zero() {
			Ok(())
		} else {
			Err(self)
		}
	}
	fn split(self, amount: T::Balance) -> (Self, Self) {
		let first = self.amount.min(amount);
		let second = self.amount - first;
		let source = self.source.clone();

		mem::forget(self);
		(Self::new(first, source.clone()), Self::new(second, source))
	}
	// Performs the merge only if sources match. Otherwise drops `other` and returns `self`.
	fn merge(mut self, other: Self) -> Self {
		self.subsume(other);
		self
	}
	// Similarly to `merge` only succeeds if the sources match, otherwise drops `other`.
	fn subsume(&mut self, other: Self) {
		if self.source == other.source {
			self.amount = self.amount.saturating_add(other.amount);
			mem::forget(other);
		}
	}
	fn offset(self, other: Self::Opposite) -> result::Result<Self, Self::Opposite> {
		let (a, b) = (self.amount, other.amount);
		let (s_a, s_b) = (self.source.clone(), other.source.clone());
		mem::forget((self, other));

		if a >= b {
			Ok(Self::new(a - b, s_a))
		} else {
			Err(Surplus::new(b - a, s_b))
		}
	}
	fn peek(&self) -> T::Balance {
		self.amount
	}
}

/// Reverts any remaining imbalance that hasn't been canceled out with an opposite imbalance.
pub trait RevertImbalance {
	fn revert(&mut self);
}

impl<T: Config> RevertImbalance for Surplus<T> {
	fn revert(&mut self) {
		match &self.source {
			ImbalanceSource::External => {
				// Some funds were bridged onto the chain but couldn't be allocated to an account. If this happens,
				// forget them since they had no on-chain source to begin with.
				// TODO: Allocate these to some 'error' account?
			}
			ImbalanceSource::Emissions => {
				// This means some funds were burned without specifying the source. If this happens, we
				// add this back on to the total issuance again.
				Flip::TotalIssuance::<T>::mutate(|v| *v = v.saturating_add(self.amount))
			}
			ImbalanceSource::Account(account_id) => {
				// This means we added funds to an account but didn't specify a source. Deduct the funds from
				// the account again.
				Flip::Account::<T>::mutate(account_id, |acct| {
					acct.stake = acct.stake.saturating_sub(self.amount)
				})
			}
		};
	}
}

impl<T: Config> RevertImbalance for Deficit<T> {
	fn revert(&mut self) {
		match &self.source {
			ImbalanceSource::External => {
				// This means we tried to move funds off-chain but didn't move them *from* anywhere
				// log::error!("Accounting error: Funds moved off-chain without accounting for it on-chain.");
			},
			ImbalanceSource::Emissions => {
				// This means some Flip were minted without allocating them somewhere. We revert by burning
				// them again.
				Flip::TotalIssuance::<T>::mutate(|v| *v = v.saturating_sub(self.amount))
			}
			ImbalanceSource::Account(account_id) => {
				// This means we deducted funds from an account and did nothing with them. Re-credit the funds to
				// the account.
				Flip::Account::<T>::mutate(account_id, |acct| {
					acct.stake = acct.stake.saturating_add(self.amount)
				})
			},
		};
	}
}

impl<T: Config> Drop for Surplus<T> {
	fn drop(&mut self) {
		<Self as RevertImbalance>::revert(self)
	}
}

impl<T: Config> Drop for Deficit<T> {
	fn drop(&mut self) {
		<Self as RevertImbalance>::revert(self)
	}
}
