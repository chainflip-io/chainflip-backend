// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

//! Per-account withdrawal-whitelist state and its (storage-agnostic) state machine.
//!
//! [`WithdrawalWhitelist`] holds everything for one account — the active external/internal
//! allowlists, the timelock, and the queue of timelocked changes — and all logic is expressed as
//! pure methods (the environment, `now` and the caps, is passed in) so it can be unit-tested
//! without a runtime.

use cf_chains::{AccountOrAddress, ForeignChain, ForeignChainAddress};
use frame_support::{pallet_prelude::*, DefaultNoBound};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	vec::Vec,
};

/// A duration or a point in time, in seconds. The withdrawal timelock works in wall-clock time
/// (via the pallet's `TimeSource`), matching cf-funding's redemption timelock.
pub type DurationSeconds = u64;

/// A change to an account's withdrawal whitelist.
///
/// Generic over the address representation: the `update_whitelist` extrinsic input uses
/// `EncodedAddress`, which is decoded into the internal [`ForeignChainAddress`] form before being
/// applied and emitted in events.
#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum WhitelistChange<AccountId, Address> {
	Allow(AccountOrAddress<AccountId, Address>),
	Remove(AccountOrAddress<AccountId, Address>),
}

/// Per-account withdrawal timelock, in seconds.
///
/// Strengthening changes (longer timelock, incl. enabling) apply immediately; weakening changes
/// (shorter, incl. disabling) are themselves delayed by the current timelock and held in `pending`
/// as `(new_duration, effective_at)`.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	Clone,
	Copy,
	PartialEq,
	Eq,
	RuntimeDebug,
	Default,
)]
pub struct TimelockState {
	pub current: DurationSeconds,
	pub pending: Option<(DurationSeconds, DurationSeconds)>,
}

impl TimelockState {
	/// The timelock duration in force at `now`, accounting for a matured pending change.
	fn effective(&self, now: DurationSeconds) -> DurationSeconds {
		match self.pending {
			Some((new_duration, effective_at)) if now >= effective_at => new_duration,
			_ => self.current,
		}
	}

	/// Collapses a matured pending change into `current`; returns whether anything changed.
	fn collapse_if_matured(&mut self, now: DurationSeconds) -> bool {
		if let Some((new_duration, effective_at)) = self.pending {
			if now >= effective_at {
				self.current = new_duration;
				self.pending = None;
				return true;
			}
		}
		false
	}

	/// Like [`effective`](Self::effective) but also collapses a matured pending change so storage
	/// stays tidy.
	fn effective_and_collapse(&mut self, now: DurationSeconds) -> DurationSeconds {
		self.collapse_if_matured(now);
		self.current
	}
}

/// All of an account's withdrawal-whitelist state: the active external/internal allowlists, the
/// timelock, and the queue of timelocked changes awaiting activation.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	Clone,
	PartialEq,
	Eq,
	RuntimeDebug,
	DefaultNoBound,
)]
pub struct WithdrawalWhitelist<AccountId> {
	/// Active external destinations per chain. An empty/absent set for a chain = unrestricted
	/// there.
	external: BTreeMap<ForeignChain, BTreeSet<ForeignChainAddress>>,
	/// Active internal (account) destinations. Empty = unrestricted.
	internal: BTreeSet<AccountId>,
	/// The withdrawal timelock (`current == 0` = restriction off).
	timelock: TimelockState,
	/// Timelocked changes awaiting activation, keyed by activation time, in submission order.
	pending: BTreeMap<DurationSeconds, Vec<WhitelistChange<AccountId, ForeignChainAddress>>>,
}

impl<AccountId: Ord + Clone> WithdrawalWhitelist<AccountId> {
	/// Whether `dest` is currently an allowed destination.
	///
	/// The restriction is off (everything allowed) until a timelock is set — backward compatible.
	/// Once on, it is **account-wide and fail-safe**: every destination must be explicitly
	/// allowlisted, and anything unconfigured — a chain or the internal set with no entries — is
	/// blocked. This stops funds being moved out via an unconfigured route, e.g. swapping into an
	/// un-allowlisted chain and withdrawing there.
	pub(crate) fn is_allowed(
		&self,
		dest: AccountOrAddress<&AccountId, &ForeignChainAddress>,
		now: DurationSeconds,
	) -> bool {
		if self.timelock.effective(now) == 0 {
			return true;
		}
		match dest {
			AccountOrAddress::ExternalAddress(address) =>
				self.external.get(&address.chain()).map_or(false, |set| set.contains(address)),
			AccountOrAddress::InternalAccount(account) => self.internal.contains(account),
		}
	}

	/// Folds any pending updates whose activation time has passed into the active allowlists.
	/// Returns whether anything changed, so the caller can skip an unnecessary storage write.
	pub(crate) fn apply_due_updates(&mut self, now: DurationSeconds) -> bool {
		let mut changed = self.timelock.collapse_if_matured(now);
		if self.pending.first_key_value().map_or(false, |(&apply_at, _)| apply_at <= now) {
			// `pending` keeps the not-yet-due buckets; `due` takes the rest (apply_at <= now).
			let not_due = self.pending.split_off(&now.saturating_add(1));
			let due = core::mem::replace(&mut self.pending, not_due);
			// A BTreeMap yields keys in ascending order, so changes apply in activation order.
			for change in due.into_values().flatten() {
				self.apply_change(&change);
			}
			changed = true;
		}
		changed
	}

	/// Schedules a change to take effect after the current timelock, returning its activation time.
	/// Returns `None` if the pending queue is already at `max_pending`.
	pub(crate) fn schedule_change(
		&mut self,
		change: WhitelistChange<AccountId, ForeignChainAddress>,
		now: DurationSeconds,
		max_pending: u32,
	) -> Option<DurationSeconds> {
		if self.pending_count() >= max_pending {
			return None;
		}
		let apply_at = now.saturating_add(self.timelock.effective_and_collapse(now));
		self.pending.entry(apply_at).or_default().push(change);
		Some(apply_at)
	}

	/// Sets the timelock, returning when the new value takes effect. Strengthening
	/// (longer/enabling) is immediate; weakening (shorter/disabling) is delayed by the current
	/// timelock.
	pub(crate) fn set_timelock(
		&mut self,
		duration: DurationSeconds,
		now: DurationSeconds,
	) -> DurationSeconds {
		let current = self.timelock.effective_and_collapse(now);
		if duration >= current {
			self.timelock = TimelockState { current: duration, pending: None };
			now
		} else {
			let effective_at = now.saturating_add(current);
			self.timelock.pending = Some((duration, effective_at));
			effective_at
		}
	}

	/// Whether this whitelist holds no state, so the caller can drop the storage entry.
	pub(crate) fn is_empty(&self) -> bool {
		self.external.is_empty() &&
			self.internal.is_empty() &&
			self.pending.is_empty() &&
			self.timelock == TimelockState::default()
	}

	fn pending_count(&self) -> u32 {
		self.pending.values().map(|changes| changes.len() as u32).sum()
	}

	/// Applies a single change to the active allowlists, pruning a chain's set once it empties.
	fn apply_change(&mut self, change: &WhitelistChange<AccountId, ForeignChainAddress>) {
		let (destination, allow) = match change {
			WhitelistChange::Allow(destination) => (destination, true),
			WhitelistChange::Remove(destination) => (destination, false),
		};
		match destination {
			AccountOrAddress::ExternalAddress(address) => {
				let chain = address.chain();
				let became_empty = {
					let set = self.external.entry(chain).or_default();
					if allow {
						set.insert(address.clone());
					} else {
						set.remove(address);
					}
					set.is_empty()
				};
				if became_empty {
					self.external.remove(&chain);
				}
			},
			AccountOrAddress::InternalAccount(account) =>
				if allow {
					self.internal.insert(account.clone());
				} else {
					self.internal.remove(account);
				},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::H160;

	type AccountId = u64;
	type Whitelist = WithdrawalWhitelist<AccountId>;

	const DAY: DurationSeconds = 24 * 3600;
	const MAX_PENDING: u32 = 16;

	// Distinct external addresses; `eth`/`arb` live on different chains.
	fn eth(byte: u8) -> ForeignChainAddress {
		ForeignChainAddress::Eth(H160([byte; 20]))
	}
	fn arb(byte: u8) -> ForeignChainAddress {
		ForeignChainAddress::Arb(H160([byte; 20]))
	}

	fn allow(address: ForeignChainAddress) -> WhitelistChange<AccountId, ForeignChainAddress> {
		WhitelistChange::Allow(AccountOrAddress::ExternalAddress(address))
	}
	fn remove(address: ForeignChainAddress) -> WhitelistChange<AccountId, ForeignChainAddress> {
		WhitelistChange::Remove(AccountOrAddress::ExternalAddress(address))
	}
	fn allow_account(account: AccountId) -> WhitelistChange<AccountId, ForeignChainAddress> {
		WhitelistChange::Allow(AccountOrAddress::InternalAccount(account))
	}

	// Borrowed-destination constructors for `is_allowed`.
	fn to_address(
		address: &ForeignChainAddress,
	) -> AccountOrAddress<&AccountId, &ForeignChainAddress> {
		AccountOrAddress::ExternalAddress(address)
	}
	fn to_account(account: &AccountId) -> AccountOrAddress<&AccountId, &ForeignChainAddress> {
		AccountOrAddress::InternalAccount(account)
	}

	#[test]
	fn default_is_unrestricted() {
		let w = Whitelist::default();
		assert!(w.is_empty());
		assert!(w.is_allowed(to_address(&eth(1)), 0));
		assert!(w.is_allowed(to_account(&7), 0));
	}

	#[test]
	fn restriction_off_while_timelock_zero() {
		let mut w = Whitelist::default();
		// Configured, but the timelock is 0, so the allowlist is dormant: everything is allowed.
		assert_eq!(w.schedule_change(allow(eth(1)), 0, MAX_PENDING), Some(0));
		assert!(w.apply_due_updates(0));
		assert!(w.is_allowed(to_address(&eth(1)), 0));
		assert!(w.is_allowed(to_address(&eth(2)), 0));
		assert!(w.is_allowed(to_address(&arb(1)), 0));
	}

	#[test]
	fn enabling_timelock_blocks_unconfigured_destinations() {
		let mut w = Whitelist::default();
		// Configure while off, then enable — the recommended setup order.
		w.schedule_change(allow(eth(1)), 0, MAX_PENDING);
		assert!(w.apply_due_updates(0));
		assert!(w.is_allowed(to_address(&eth(2)), 0)); // off => still unrestricted
		assert_eq!(w.set_timelock(DAY, 0), 0); // enabling is immediate

		// On + fail-safe: only the configured destination is allowed.
		assert!(w.is_allowed(to_address(&eth(1)), 0));
		assert!(!w.is_allowed(to_address(&eth(2)), 0)); // same chain, not listed
		assert!(!w.is_allowed(to_address(&arb(1)), 0)); // unconfigured chain => blocked
		assert!(!w.is_allowed(to_account(&1), 0)); // internal set empty => blocked
	}

	#[test]
	fn removing_last_address_blocks_chain_when_on() {
		let mut w = Whitelist::default();
		w.schedule_change(allow(eth(1)), 0, MAX_PENDING);
		w.apply_due_updates(0);
		w.set_timelock(DAY, 0);
		assert!(w.is_allowed(to_address(&eth(1)), 0));

		// Removal is delayed by the timelock; once it folds, the chain has no entries...
		assert_eq!(w.schedule_change(remove(eth(1)), 0, MAX_PENDING), Some(DAY));
		assert!(w.apply_due_updates(DAY));
		// ...so it is blocked (fail-safe), not reopened — you can't empty-to-unrestrict.
		assert!(!w.is_allowed(to_address(&eth(1)), DAY));
		assert!(!w.is_allowed(to_address(&eth(2)), DAY));
	}

	#[test]
	fn changes_apply_in_activation_order_across_buckets() {
		let mut w = Whitelist::default();
		// Same destination, different activation times; the later one (remove) must win.
		w.schedule_change(allow(eth(1)), 0, MAX_PENDING); // apply_at 0
		w.schedule_change(remove(eth(1)), 5, MAX_PENDING); // apply_at 5
		assert!(w.apply_due_updates(10));
		assert!(w.is_empty());
	}

	#[test]
	fn later_change_in_same_bucket_wins() {
		let mut w = Whitelist::default();
		// Same activation time (same block + same timelock): submission order decides.
		w.schedule_change(allow(eth(1)), 0, MAX_PENDING);
		w.schedule_change(remove(eth(1)), 0, MAX_PENDING);
		assert!(w.apply_due_updates(0));
		assert!(w.is_empty());
	}

	#[test]
	fn set_timelock_strengthen_immediate_weaken_delayed() {
		let mut w = Whitelist::default();
		// Enable: 0 -> DAY applies immediately.
		assert_eq!(w.set_timelock(DAY, 0), 0);
		let scheduled = w.schedule_change(allow(eth(1)), 0, MAX_PENDING);
		assert_eq!(scheduled, Some(DAY));

		// Weaken: DAY -> 1h is delayed by the current timelock.
		let now = 100;
		assert_eq!(w.set_timelock(3600, now), now + DAY);
		// Until the weakening matures, scheduling still uses the old (longer) timelock.
		assert_eq!(w.schedule_change(allow(eth(2)), now, MAX_PENDING), Some(now + DAY));
		// Once it has matured, the shorter timelock is in force.
		let later = now + DAY;
		assert_eq!(w.schedule_change(allow(eth(3)), later, MAX_PENDING), Some(later + 3600));
	}

	#[test]
	fn schedule_change_respects_cap() {
		let mut w = Whitelist::default();
		for byte in 0..MAX_PENDING as u8 {
			assert!(w.schedule_change(allow(eth(byte)), 0, MAX_PENDING).is_some());
		}
		// Queue is full.
		assert!(w.schedule_change(allow(eth(99)), 0, MAX_PENDING).is_none());
	}

	#[test]
	fn internal_account_allowlist_when_on() {
		let mut w = Whitelist::default();
		w.schedule_change(allow_account(1), 0, MAX_PENDING);
		w.apply_due_updates(0);
		w.set_timelock(DAY, 0);

		assert!(w.is_allowed(to_account(&1), 0));
		assert!(!w.is_allowed(to_account(&2), 0));
		// External destinations are gated independently — an unconfigured chain is blocked.
		assert!(!w.is_allowed(to_address(&eth(1)), 0));
	}

	#[test]
	fn apply_due_updates_reports_change_and_keeps_not_due() {
		let mut w = Whitelist::default();
		w.set_timelock(DAY, 0);
		w.schedule_change(allow(eth(1)), 0, MAX_PENDING); // apply_at DAY
		w.schedule_change(allow(eth(2)), DAY, MAX_PENDING); // apply_at 2*DAY

		// Nothing due yet.
		assert!(!w.apply_due_updates(DAY - 1));
		// First matures; second is retained (and not yet active).
		assert!(w.apply_due_updates(DAY));
		assert!(w.is_allowed(to_address(&eth(1)), DAY));
		assert!(!w.is_allowed(to_address(&eth(2)), DAY));
		// Second matures later.
		assert!(w.apply_due_updates(2 * DAY));
		assert!(w.is_allowed(to_address(&eth(2)), 2 * DAY));
	}
}
