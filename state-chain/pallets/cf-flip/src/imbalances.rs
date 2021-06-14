//! This is based on the imbalances modules from the balances pallet.

// wrapping these imbalances in a private module is necessary to ensure absolute privacy
// of the inner member.

use crate::{self as Flip, Config};
use codec::{Decode, Encode};
use frame_support::traits::{Imbalance, TryDrop};
use sp_runtime::{
	traits::{Bounded, CheckedAdd, CheckedSub, Saturating, Zero},
	RuntimeDebug,
};
use sp_std::{mem, result};

#[derive(RuntimeDebug, PartialEq, Eq, Clone, Encode, Decode)]
pub enum ImbalanceSource<AccountId> {
	External,
	Account(AccountId),
	Emissions
}

/// Opaque, move-only struct with private fields that serves as a token denoting that funds have been added from
/// *somewhere*, and that we need to account for this by cancelling it against a corresponding [Deficit].
#[must_use = "This surplus needs to be reconciled - if not any remaining imblance will be reverted."]
#[derive(RuntimeDebug, PartialEq, Eq)]
pub struct Surplus<T: Config> {
	amount: T::Balance,
	pub(super) source: ImbalanceSource<T::AccountId>,
}

impl<T: Config> Surplus<T> {
	/// Create a new surplus.
	fn new(amount: T::Balance, source: ImbalanceSource<T::AccountId>) -> Self {
		Surplus { amount, source, }
	}

	/// Funds surplus from minting new funds. This surplus needs to be allocated somewhere or the mint will be
	/// [reverted](RevertImbalance).
	pub(super) fn from_mint(mut amount: T::Balance) -> Self {
		if amount.is_zero() {
			return Self::new(Zero::zero(), ImbalanceSource::Emissions);
		}
		Flip::TotalIssuance::<T>::mutate(|total| {
			*total = total.checked_add(&amount).unwrap_or_else(|| {
				amount = T::Balance::max_value() - *total;
				T::Balance::max_value()
			})
		});
		Self::new(amount, ImbalanceSource::Emissions)
	}

	/// Funds surplus from an account.
	///
	/// Usually means that funds have been debited from an account.
	pub(super) fn from_acct(account_id: &T::AccountId, amount: T::Balance) -> Self {
		Flip::Account::<T>::mutate(account_id, |account| {
			let deducted = account.stake.min(amount);
			account.stake = account.stake.saturating_sub(deducted);
			Self::new(deducted, ImbalanceSource::Account(account_id.clone()))
		})
	}

	/// Funds surplus from offchain.
	///
	/// Means we have received funds from offchain; there will now be a surplus that needs to be allocated somewhere.
	pub(super) fn from_offchain(amount: T::Balance) -> Self {
		Flip::OffchainFunds::<T>::mutate(|total| {
			let deducted = (*total).min(amount);
			*total = total.saturating_sub(deducted);
			Self::new(deducted, ImbalanceSource::External)
		})
	}
}

/// Opaque, move-only struct with private fields that serves as a token denoting that funds have been removed to
/// *somewhere*, and that we need to account for this by cancelling it against a corresponding [Surplus].
#[must_use = "This deficit needs to be reconciled - if not any remaining imbalance will be reverted."]
#[derive(RuntimeDebug, PartialEq, Eq)]
pub struct Deficit<T: Config> {
	amount: T::Balance,
	pub(super) source: ImbalanceSource<T::AccountId>,
}

impl<T: Config> Deficit<T> {
	/// Create a new deficit from a balance.
	fn new(amount: T::Balance, source: ImbalanceSource<T::AccountId>) -> Self {
		Deficit { amount, source }
	}

	/// Burn funds, creating a corresponding deficit. The deficit needs to be applied somewhere or the burn will be
	/// [reverted](RevertImbalance).
	pub(super) fn from_burn(mut amount: T::Balance) -> Self {
		if amount.is_zero() {
			return Self::new(Zero::zero(), ImbalanceSource::Emissions);
		}
		Flip::TotalIssuance::<T>::mutate(|issued| {
			*issued = issued.checked_sub(&amount).unwrap_or_else(|| {
				amount = *issued;
				Zero::zero()
			});
		});
		Self::new(amount, ImbalanceSource::Emissions)
	}

	/// Funds deficit from an account. 
	///
	/// Usually means that funds have been credited to an account.
	pub(super) fn from_acct(account_id: &T::AccountId, amount: T::Balance) -> Self {
		Flip::Account::<T>::mutate(account_id, |account| {
			match account.stake.checked_add(&amount) {
				Some(result) => {
					account.stake = result;
					Self::new(amount, ImbalanceSource::Account(account_id.clone()))
				}
				None => Self::new(Zero::zero(), ImbalanceSource::Account(account_id.clone()))
			}
		})
	}

	/// Funds deficit from offchain.
	///
	/// Means that funds have been sent offchain; we need to apply the resulting deficit somewhere.
	pub(super) fn from_offchain(amount: T::Balance) -> Self {
		let added = Flip::OffchainFunds::<T>::mutate(|total| {
			match total.checked_add(&amount) {
				Some(result) => {
					*total = result;
					amount
				},
				None => Zero::zero()
			}
		});
		Self::new(added, ImbalanceSource::External)
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
				// Some funds were bridged onto the chain but weren't be allocated to an account. For all intents and
				// purposes they are still offchain.
				// TODO: Allocate these to some 'error' account? Eg. for refunds.
				Flip::OffchainFunds::<T>::mutate(|total| *total = total.saturating_add(self.amount));
			}
			ImbalanceSource::Emissions => {
				// This means some Flip were minted without allocating them somewhere. We revert by burning
				// them again.
				Flip::TotalIssuance::<T>::mutate(|v| *v = v.saturating_sub(self.amount))
			}
			ImbalanceSource::Account(account_id) => {
				// This means we took funds from an account but didn't put them anywhere. Add the funds back to
				// the account again.
				Flip::Account::<T>::mutate(account_id, |acct| {
					acct.stake = acct.stake.saturating_add(self.amount)
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
				Flip::OffchainFunds::<T>::mutate(|total| *total = total.saturating_sub(self.amount));
			},
			ImbalanceSource::Emissions => {
				// This means some funds were burned without specifying the source. If this happens, we
				// add this back on to the total issuance again.
				Flip::TotalIssuance::<T>::mutate(|v| *v = v.saturating_add(self.amount))
			}
			ImbalanceSource::Account(account_id) => {
				// This means we added funds to an account without specifying a source. Deduct them again.
				Flip::Account::<T>::mutate(account_id, |acct| {
					acct.stake = acct.stake.saturating_sub(self.amount)
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
