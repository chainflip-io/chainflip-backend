//! This is based on the imbalances modules from the balances pallet.

// wrapping these imbalances in a private module is necessary to ensure absolute privacy
// of the inner member.

use crate::{self as Flip, Config, ReserveId};
use codec::{Decode, Encode};
use frame_support::{
	sp_runtime::{
		traits::{CheckedAdd, CheckedSub, Saturating, Zero},
		RuntimeDebug,
	},
	traits::{Imbalance, SameOrOther, TryDrop},
};
use scale_info::TypeInfo;
use sp_std::{cmp, mem, result};

/// Internal sources of funds.
#[derive(RuntimeDebug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo)]
pub enum InternalSource<AccountId> {
	/// A user account.
	Account(AccountId),
	/// Reserved funds. Could be a pot of rewards, a treasury balance, etc.
	Reserve(ReserveId),
	/// Pending redemptions for different accounts.
	PendingRedemption(AccountId),
}

/// The origin of an imbalance.
#[derive(RuntimeDebug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo)]
pub enum ImbalanceSource<AccountId> {
	/// External, aka. off-chain.
	External,
	/// Internal, aka. on-chain.
	Internal(InternalSource<AccountId>),
	/// Emissions, aka. a mint or burn.
	Emissions,
}

impl<AccountId> ImbalanceSource<AccountId> {
	pub fn acct(id: AccountId) -> Self {
		Self::Internal(InternalSource::Account(id))
	}

	pub fn reserve(id: ReserveId) -> Self {
		Self::Internal(InternalSource::Reserve(id))
	}

	pub fn pending_redemptions(id: AccountId) -> Self {
		Self::Internal(InternalSource::PendingRedemption(id))
	}
}

/// Opaque, move-only struct with private fields that serves as a token denoting that funds have
/// been added from *somewhere*, and that we need to account for this by cancelling it against a
/// corresponding [Deficit].
#[must_use = "This surplus needs to be reconciled - if not any remaining imblance will be reverted."]
#[derive(RuntimeDebug, PartialEq, Eq)]
pub struct Surplus<T: Config> {
	amount: T::Balance,
	pub(super) source: ImbalanceSource<T::AccountId>,
}

impl<T: Config> Surplus<T> {
	/// Create a new surplus.
	fn new(amount: T::Balance, source: ImbalanceSource<T::AccountId>) -> Self {
		Surplus { amount, source }
	}

	/// Funds surplus from minting new funds. This surplus needs to be allocated somewhere or the
	/// mint will be [reverted](RevertImbalance).
	pub(super) fn from_mint(amount: T::Balance) -> Self {
		Self::new(
			if amount.is_zero() {
				Zero::zero()
			} else {
				Flip::TotalIssuance::<T>::mutate(|total| match total.checked_add(&amount) {
					Some(new_total) => {
						*total = new_total;
						amount
					},
					None => Zero::zero(),
				})
			},
			ImbalanceSource::Emissions,
		)
	}

	/// Tries to withdraw funds from an account. Fails if the account doesn't exist or has
	/// insufficient funds. Also ensures that we only touch funds from the bonded balance if
	/// `check_liquidity` is `false`.
	pub(super) fn try_from_acct(
		account_id: &T::AccountId,
		amount: T::Balance,
		check_liquidity: bool,
	) -> Option<Self> {
		Flip::Account::<T>::try_mutate_exists(account_id, |maybe_account| {
			if let Some(account) = maybe_account.as_mut() {
				if check_liquidity && account.liquid() < amount {
					return Err(())
				}
				if account.balance < amount {
					return Err(())
				}
				account.balance = account.balance.saturating_sub(amount);
				Ok(Self::new(amount, ImbalanceSource::acct(account_id.clone())))
			} else {
				Err(())
			}
		})
		.ok()
	}

	/// Withdraw funds from an account. Deducts *up to* the requested amount, depending on available
	/// funds.
	///
	/// *Warning:* if the account entry does not exist, it will be created as a side effect. Do not
	/// expose this via  an extrinsic.
	pub(super) fn from_acct(account_id: &T::AccountId, amount: T::Balance) -> Self {
		Flip::Account::<T>::mutate(account_id, |account| {
			let deducted = account.balance.min(amount);
			account.balance = account.balance.saturating_sub(deducted);
			Self::new(deducted, ImbalanceSource::acct(account_id.clone()))
		})
	}

	/// Tries to withdraw funds from a reserve. Fails if the reserve doesn't exist or has
	/// insufficient funds.
	pub(super) fn try_from_reserve(reserve_id: ReserveId, amount: T::Balance) -> Option<Self> {
		Flip::Reserve::<T>::try_mutate(reserve_id, |balance| {
			if (*balance) < amount {
				Err(())
			} else {
				(*balance) = (*balance).saturating_sub(amount);
				Ok(Self::new(amount, ImbalanceSource::reserve(reserve_id)))
			}
		})
		.ok()
	}

	/// Tries to withdraw funds from a Pending Redemptions reserve for the corresponding Account ID.
	/// Fails if the pending redemption doesn't exist for that ID
	pub(super) fn try_from_pending_redemptions_reserve(account_id: &T::AccountId) -> Option<Self> {
		Flip::PendingRedemptionsReserve::<T>::take(account_id).map(|amount| {
			Self::new(amount, ImbalanceSource::pending_redemptions(account_id.clone()))
		})
	}

	/// Withdraw funds from a reserve. Deducts *up to* the requested amount, depending on available
	/// funds.
	///
	/// *Warning:* if the reserve does not exist, it will be created as a side effect. Do not expose
	/// this via  an extrinsic.
	pub(super) fn from_reserve(reserve_id: ReserveId, amount: T::Balance) -> Self {
		Flip::Reserve::<T>::mutate(reserve_id, |balance| {
			let deducted = (*balance).min(amount);
			*balance = (*balance).saturating_sub(deducted);
			Self::new(deducted, ImbalanceSource::reserve(reserve_id))
		})
	}

	/// Funds surplus from offchain.
	///
	/// Means we have received funds from offchain; there will now be a surplus that needs to be
	/// allocated somewhere.
	pub(super) fn from_offchain(amount: T::Balance) -> Self {
		Flip::OffchainFunds::<T>::mutate(|total| {
			let deducted = (*total).min(amount);
			*total = total.saturating_sub(deducted);
			Self::new(deducted, ImbalanceSource::External)
		})
	}
}

/// Opaque, move-only struct with private fields that serves as a token denoting that funds have
/// been removed to *somewhere*, and that we need to account for this by cancelling it against a
/// corresponding [Surplus].
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

	/// Burn funds, creating a corresponding deficit. The deficit needs to be applied somewhere or
	/// the burn will be [reverted](RevertImbalance).
	pub(super) fn from_burn(mut amount: T::Balance) -> Self {
		if amount.is_zero() {
			return Self::new(Zero::zero(), ImbalanceSource::Emissions)
		}
		Flip::TotalIssuance::<T>::mutate(|issued| {
			*issued = issued.checked_sub(&amount).unwrap_or_else(|| {
				amount = *issued;
				Zero::zero()
			});
		});
		Self::new(amount, ImbalanceSource::Emissions)
	}

	/// Credit funds to an account.
	///
	/// In case of overflow, the returned imbalance is zero (meaning nothing will be credited).
	///
	/// *Warning:* if the accout does not exist, it will be created as a side effect. Do not expose
	/// this via  an extrinsic.
	pub(super) fn from_acct(account_id: &T::AccountId, amount: T::Balance) -> Self {
		Flip::Account::<T>::mutate(account_id, |account| {
			let added = match account.balance.checked_add(&amount) {
				Some(result) => {
					account.balance = result;
					amount
				},
				None => Zero::zero(),
			};
			Self::new(added, ImbalanceSource::acct(account_id.clone()))
		})
	}

	/// Credit funds to a reserve.
	///
	/// In case of overflow, the returned imbalance is zero (meaning nothing will be credited).
	///
	/// *Warning:* if the reserve does not exist, it will be created as a side effect. Do not expose
	/// this via  an extrinsic.
	pub(super) fn from_reserve(reserve_id: ReserveId, amount: T::Balance) -> Self {
		Flip::Reserve::<T>::mutate(reserve_id, |balance| {
			let added = match balance.checked_add(&amount) {
				Some(result) => {
					(*balance) = result;
					amount
				},
				None => Zero::zero(),
			};
			Self::new(added, ImbalanceSource::reserve(reserve_id))
		})
	}

	/// Creates a pending redemptions reserve account for the given account ID.
	pub(super) fn from_pending_redemptions_reserve(
		account_id: &T::AccountId,
		amount: T::Balance,
	) -> Self {
		Flip::PendingRedemptionsReserve::<T>::insert(account_id, amount);
		Self::new(amount, ImbalanceSource::pending_redemptions(account_id.clone()))
	}

	/// Funds deficit from offchain.
	///
	/// Means that funds have been sent offchain; we need to apply the resulting deficit somewhere.
	pub(super) fn from_offchain(amount: T::Balance) -> Self {
		let added = Flip::OffchainFunds::<T>::mutate(|total| match total.checked_add(&amount) {
			Some(result) => {
				*total = result;
				amount
			},
			None => Zero::zero(),
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
		Self { amount: Zero::zero(), source: ImbalanceSource::Emissions }
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
	fn offset(self, other: Self::Opposite) -> SameOrOther<Self, Self::Opposite> {
		let (a, b) = (self.amount, other.amount);
		let (s_a, s_b) = (self.source.clone(), other.source.clone());
		mem::forget((self, other));

		match a.cmp(&b) {
			cmp::Ordering::Less => SameOrOther::Other(Deficit::new(b - a, s_b)),
			cmp::Ordering::Greater => SameOrOther::Same(Self::new(a - b, s_a)),
			cmp::Ordering::Equal => SameOrOther::None,
		}
	}
	fn peek(&self) -> T::Balance {
		self.amount
	}
	fn extract(&mut self, amount: T::Balance) -> Self {
		let extracted = self.amount.min(amount);
		self.amount -= extracted;
		Self::new(extracted, self.source.clone())
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
		Self { amount: Zero::zero(), source: ImbalanceSource::Emissions }
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
	fn offset(self, other: Self::Opposite) -> SameOrOther<Self, Self::Opposite> {
		let (a, b) = (self.amount, other.amount);
		let (s_a, s_b) = (self.source.clone(), other.source.clone());
		mem::forget((self, other));

		if a >= b {
			SameOrOther::Same(Self::new(a - b, s_a))
		} else {
			SameOrOther::Other(Surplus::new(b - a, s_b))
		}
	}
	fn peek(&self) -> T::Balance {
		self.amount
	}
	fn extract(&mut self, amount: T::Balance) -> Self {
		let extracted = self.amount.min(amount);
		self.amount -= extracted;
		Self::new(extracted, self.source.clone())
	}
}

impl<T: Config> Default for Surplus<T> {
	fn default() -> Self {
		Self::zero()
	}
}

impl<T: Config> Default for Deficit<T> {
	fn default() -> Self {
		Self::zero()
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
				// Some funds were bridged onto the chain but weren't be allocated to an account.
				// For all intents and purposes they are still offchain.
				// TODO: Allocate these to some 'error' account? Eg. for refunds.
				Flip::OffchainFunds::<T>::mutate(|total| {
					*total = total.saturating_add(self.amount)
				});
			},
			ImbalanceSource::Emissions => {
				// This means some Flip were minted without allocating them somewhere. We revert by
				// burning them again.
				Flip::TotalIssuance::<T>::mutate(|v| *v = v.saturating_sub(self.amount))
			},
			ImbalanceSource::Internal(internal) => {
				match internal {
					InternalSource::Account(account_id) => {
						// This means we took funds from an account but didn't put them anywhere.
						// Add the funds back to the account again.
						Flip::Account::<T>::mutate(account_id, |acct| {
							acct.balance = acct.balance.saturating_add(self.amount)
						})
					},
					InternalSource::Reserve(reserve_id) => {
						// This means we took funds from a reserve but didn't put them anywhere. Add
						// the funds back to the reserve again.
						Flip::Reserve::<T>::mutate(reserve_id, |rsrv| {
							*rsrv = rsrv.saturating_add(self.amount)
						})
					},
					InternalSource::PendingRedemption(account_id) => {
						// This means we took funds from a pending redemption but didn't put them
						// anywhere. Add the funds back to the account again.
						if self.amount != 0_u128.into() {
							Flip::PendingRedemptionsReserve::<T>::insert(account_id, self.amount);
						}
					},
				}
			},
		};
	}
}

impl<T: Config> RevertImbalance for Deficit<T> {
	fn revert(&mut self) {
		match &self.source {
			ImbalanceSource::External => {
				// This means we tried to move funds off-chain but didn't move them *from* anywhere
				Flip::OffchainFunds::<T>::mutate(|total| {
					*total = total.saturating_sub(self.amount)
				});
			},
			ImbalanceSource::Emissions => {
				// This means some funds were burned without specifying the source. If this happens,
				// we add this back on to the total issuance again.
				Flip::TotalIssuance::<T>::mutate(|v| *v = v.saturating_add(self.amount))
			},
			ImbalanceSource::Internal(internal) => {
				match internal {
					InternalSource::Account(account_id) => {
						// This means we added funds to an account without specifying a source.
						// Deduct them again.
						Flip::Account::<T>::mutate(account_id, |acct| {
							acct.balance = acct.balance.saturating_sub(self.amount)
						})
					},
					InternalSource::Reserve(reserve_id) => {
						// This means we added funds to a reserve without specifying a source.
						// Deduct them again.
						Flip::Reserve::<T>::mutate(reserve_id, |rsrv| {
							*rsrv = rsrv.saturating_sub(self.amount)
						})
					},
					InternalSource::PendingRedemption(account_id) => {
						// This means we added funds to a pending redemption without specifying a
						// source. Deduct them again.
						if self.amount != 0_u128.into() {
							Flip::PendingRedemptionsReserve::<T>::remove(account_id);
						}
					},
				}
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
