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

/// A value with an optional timelocked change: `pending` — a `(new_value, apply_at)` pair — becomes
/// `current` once `now` reaches its activation time. Only the state and its resolution against
/// `now` live here; the *policy* for when a change is immediate vs delayed belongs to the caller.
/// Used for both the withdrawal [`TimelockState`] and [`RefundAddress`].
#[derive(
	Encode, Decode, DecodeWithMemTracking, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug, Default,
)]
pub struct TimelockedValue<T> {
	current: T,
	pending: Option<(T, DurationSeconds)>,
}

impl<T: Clone> TimelockedValue<T> {
	/// The value in force at `now`, accounting for a matured pending change.
	fn effective(&self, now: DurationSeconds) -> &T {
		match &self.pending {
			Some((value, apply_at)) if now >= *apply_at => value,
			_ => &self.current,
		}
	}

	/// Collapses a matured pending change into `current`; returns whether anything changed.
	fn collapse_if_matured(&mut self, now: DurationSeconds) -> bool {
		if let Some((value, apply_at)) = &self.pending {
			if now >= *apply_at {
				self.current = value.clone();
				self.pending = None;
				return true;
			}
		}
		false
	}

	/// Like [`effective`](Self::effective) but also collapses a matured pending change so storage
	/// stays tidy.
	fn effective_and_collapse(&mut self, now: DurationSeconds) -> &T {
		self.collapse_if_matured(now);
		&self.current
	}

	/// Applies `value` immediately, discarding any pending change.
	fn set_now(&mut self, value: T) {
		self.current = value;
		self.pending = None;
	}

	/// Schedules `value` to take effect `delay` seconds from `now` (folding any already-matured
	/// change first). Returns the activation time.
	fn schedule(
		&mut self,
		value: T,
		now: DurationSeconds,
		delay: DurationSeconds,
	) -> DurationSeconds {
		self.collapse_if_matured(now);
		let apply_at = now.saturating_add(delay);
		self.pending = Some((value, apply_at));
		apply_at
	}
}

/// Per-account withdrawal timelock, in seconds (`current == 0` = restriction off). Strengthening
/// (longer/enabling) applies immediately; weakening (shorter/disabling) is delayed by the current
/// timelock — the policy lives in [`WithdrawalWhitelist::set_timelock`].
pub type TimelockState = TimelockedValue<DurationSeconds>;

/// An account's refund address for one chain, with a possibly-pending timelocked change.
///
/// The refund address is implicitly allowed as a withdrawal destination, so — like whitelist
/// entries — repointing it is delayed by the account's timelock while the restriction is on: the
/// current address stays active until the pending one matures.
#[derive(
	Encode, Decode, DecodeWithMemTracking, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug, Default,
)]
pub struct RefundAddress(TimelockedValue<Option<ForeignChainAddress>>);

impl RefundAddress {
	/// An immediately-active refund address.
	pub fn immediate(address: ForeignChainAddress) -> Self {
		Self(TimelockedValue { current: Some(address), pending: None })
	}

	/// Registers `new`. Immediate when `timelock == 0`; otherwise the current address stays active
	/// and `new` is scheduled for `now + timelock`. A pending change can be freely replaced (which
	/// reschedules it for a fresh `now + timelock`).
	pub(crate) fn register(
		&mut self,
		new: ForeignChainAddress,
		now: DurationSeconds,
		timelock: DurationSeconds,
	) {
		if timelock == 0 {
			self.0.set_now(Some(new));
		} else {
			self.0.schedule(Some(new), now, timelock);
		}
	}

	/// Whether a refund address is registered — either effective now or scheduled (pending but not
	/// yet matured). Gates chain interaction: a pending first registration counts, so an LP isn't
	/// blocked from a chain during the timelock window after registering there.
	pub(crate) fn is_registered(&self) -> bool {
		self.0.current.is_some() || self.0.pending.is_some()
	}

	/// The effective (active) refund address at `now` (accounting for a matured pending change), if
	/// any.
	pub(crate) fn effective(&self, now: DurationSeconds) -> Option<&ForeignChainAddress> {
		self.0.effective(now).as_ref()
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
		if *self.timelock.effective(now) == 0 {
			return true;
		}
		match dest {
			AccountOrAddress::ExternalAddress(address) =>
				self.external.get(&address.chain()).is_some_and(|set| set.contains(address)),
			AccountOrAddress::InternalAccount(account) => self.internal.contains(account),
		}
	}

	/// Folds any pending updates whose activation time has passed into the active allowlists.
	/// Returns whether anything changed, so the caller can skip an unnecessary storage write.
	///
	/// The active allowlist is capped at `max_entries`: an `Allow` that would exceed it is applied
	/// as a no-op. This is enforced here rather than at scheduling time to keep things simple (this
	/// prevents abuse, while real users are unlikely to ever hit this).
	pub(crate) fn apply_due_updates(&mut self, now: DurationSeconds, max_entries: u32) -> bool {
		let mut changed = self.timelock.collapse_if_matured(now);
		if self.pending.first_key_value().is_some_and(|(&apply_at, _)| apply_at <= now) {
			// `pending` keeps the not-yet-due buckets; `due` takes the rest (apply_at <= now).
			let not_due = self.pending.split_off(&now.saturating_add(1));
			let due = core::mem::replace(&mut self.pending, not_due);
			// A BTreeMap yields keys in ascending order, so changes apply in activation order.
			for change in due.into_values().flatten() {
				self.apply_change(&change, max_entries);
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
		let apply_at = now.saturating_add(*self.timelock.effective_and_collapse(now));
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
		let current = *self.timelock.effective_and_collapse(now);
		if duration >= current {
			self.timelock.set_now(duration);
			now
		} else {
			// Weakening/disabling is delayed by the current timelock.
			self.timelock.schedule(duration, now, current)
		}
	}

	/// The account's effective withdrawal timelock at `now` (0 = restriction off).
	pub(crate) fn effective_timelock(&self, now: DurationSeconds) -> DurationSeconds {
		*self.timelock.effective(now)
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

	/// Number of active allowlist entries: external addresses across all chains plus internal
	/// accounts.
	fn active_entry_count(&self) -> u32 {
		self.external
			.values()
			.fold(0u32, |acc, set| acc.saturating_add(set.len() as u32))
			.saturating_add(self.internal.len() as u32)
	}

	/// Applies a single change to the active allowlists, pruning a chain's set once it empties.
	///
	/// An `Allow` that would grow the active set beyond `max_entries` is a no-op — this bounds the
	/// stored whitelist. `Remove` always applies. (Re-allowing an address that is already active is
	/// unaffected, since it doesn't grow the set.)
	fn apply_change(
		&mut self,
		change: &WhitelistChange<AccountId, ForeignChainAddress>,
		max_entries: u32,
	) {
		let (destination, allow) = match change {
			WhitelistChange::Allow(destination) => (destination, true),
			WhitelistChange::Remove(destination) => (destination, false),
		};
		if allow && self.active_entry_count() >= max_entries {
			return;
		}
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
	const MAX_ENTRIES: u32 = 100;

	// Schedules a change with the default pending cap, asserting it is accepted; returns the
	// activation time.
	fn schedule(
		w: &mut Whitelist,
		change: WhitelistChange<AccountId, ForeignChainAddress>,
		now: DurationSeconds,
	) -> DurationSeconds {
		w.schedule_change(change, now, MAX_PENDING).unwrap()
	}

	// Applies due updates with the default entry cap.
	fn apply(w: &mut Whitelist, now: DurationSeconds) -> bool {
		w.apply_due_updates(now, MAX_ENTRIES)
	}

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
		assert_eq!(schedule(&mut w, allow(eth(1)), 0), 0);
		assert!(apply(&mut w, 0));
		assert!(w.is_allowed(to_address(&eth(1)), 0));
		assert!(w.is_allowed(to_address(&eth(2)), 0));
		assert!(w.is_allowed(to_address(&arb(1)), 0));
	}

	#[test]
	fn enabling_timelock_blocks_unconfigured_destinations() {
		let mut w = Whitelist::default();
		// Configure while off, then enable — the recommended setup order.
		schedule(&mut w, allow(eth(1)), 0);
		assert!(apply(&mut w, 0));
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
		schedule(&mut w, allow(eth(1)), 0);
		apply(&mut w, 0);
		w.set_timelock(DAY, 0);
		assert!(w.is_allowed(to_address(&eth(1)), 0));

		// Removal is delayed by the timelock; once it folds, the chain has no entries...
		assert_eq!(schedule(&mut w, remove(eth(1)), 0), DAY);
		assert!(apply(&mut w, DAY));
		// ...so it is blocked (fail-safe), not reopened — you can't empty-to-unrestrict.
		assert!(!w.is_allowed(to_address(&eth(1)), DAY));
		assert!(!w.is_allowed(to_address(&eth(2)), DAY));
	}

	#[test]
	fn changes_apply_in_activation_order_across_buckets() {
		let mut w = Whitelist::default();
		// Same destination, different activation times; the later one (remove) must win.
		schedule(&mut w, allow(eth(1)), 0); // apply_at 0
		schedule(&mut w, remove(eth(1)), 5); // apply_at 5
		assert!(apply(&mut w, 10));
		assert!(w.is_empty());
	}

	#[test]
	fn later_change_in_same_bucket_wins() {
		let mut w = Whitelist::default();
		// Same activation time (same block + same timelock): submission order decides.
		schedule(&mut w, allow(eth(1)), 0);
		schedule(&mut w, remove(eth(1)), 0);
		assert!(apply(&mut w, 0));
		assert!(w.is_empty());
	}

	#[test]
	fn set_timelock_strengthen_immediate_weaken_delayed() {
		let mut w = Whitelist::default();
		// Enable: 0 -> DAY applies immediately.
		assert_eq!(w.set_timelock(DAY, 0), 0);
		assert_eq!(schedule(&mut w, allow(eth(1)), 0), DAY);

		// Weaken: DAY -> 1h is delayed by the current timelock.
		let now = 100;
		assert_eq!(w.set_timelock(3600, now), now + DAY);
		// Until the weakening matures, scheduling still uses the old (longer) timelock.
		assert_eq!(schedule(&mut w, allow(eth(2)), now), now + DAY);
		// Once it has matured, the shorter timelock is in force.
		let later = now + DAY;
		assert_eq!(schedule(&mut w, allow(eth(3)), later), later + 3600);
	}

	#[test]
	fn schedule_change_respects_pending_cap() {
		let mut w = Whitelist::default();
		for byte in 0..MAX_PENDING as u8 {
			assert!(w.schedule_change(allow(eth(byte)), 0, MAX_PENDING).is_some());
		}
		// Queue is full.
		assert_eq!(w.schedule_change(allow(eth(99)), 0, MAX_PENDING), None);
	}

	#[test]
	fn apply_caps_active_entries() {
		let mut w = Whitelist::default();
		let max_entries = 3;
		// Schedule more allows than the cap allows (pending queue is large enough to hold them).
		for byte in 0..5 {
			w.schedule_change(allow(eth(byte)), 0, MAX_PENDING);
		}
		// Applying them stops adding once the cap is reached; the excess are no-ops.
		w.apply_due_updates(0, max_entries);
		assert_eq!(w.active_entry_count(), max_entries);
	}

	#[test]
	fn apply_cap_lets_matured_removes_free_slots() {
		let mut w = Whitelist::default();
		let max_entries = 2;
		// Two allows fill the cap; a third (later) allow would be a no-op — unless a remove frees a
		// slot first. Schedule remove(eth(0)) before allow(eth(2)) so it applies earlier.
		w.schedule_change(allow(eth(0)), 0, MAX_PENDING); // apply_at 0
		w.schedule_change(allow(eth(1)), 0, MAX_PENDING); // apply_at 0
		w.schedule_change(remove(eth(0)), 5, MAX_PENDING); // apply_at 5, frees a slot
		w.schedule_change(allow(eth(2)), 5, MAX_PENDING); // apply_at 5, takes the freed slot
		w.apply_due_updates(10, max_entries);
		assert_eq!(w.active_entry_count(), max_entries);
		assert!(w.is_allowed(to_address(&eth(2)), 0)); // timelock is 0 here, so allowed regardless
												 // eth(0) was removed, eth(1) and eth(2) remain.
		w.set_timelock(DAY, 20);
		assert!(w.is_allowed(to_address(&eth(1)), 20));
		assert!(w.is_allowed(to_address(&eth(2)), 20));
		assert!(!w.is_allowed(to_address(&eth(0)), 20));
	}

	#[test]
	fn internal_account_allowlist_when_on() {
		let mut w = Whitelist::default();
		schedule(&mut w, allow_account(1), 0);
		apply(&mut w, 0);
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
		schedule(&mut w, allow(eth(1)), 0); // apply_at DAY
		schedule(&mut w, allow(eth(2)), DAY); // apply_at 2*DAY

		// Nothing due yet.
		assert!(!apply(&mut w, DAY - 1));
		// First matures; second is retained (and not yet active).
		assert!(apply(&mut w, DAY));
		assert!(w.is_allowed(to_address(&eth(1)), DAY));
		assert!(!w.is_allowed(to_address(&eth(2)), DAY));
		// Second matures later.
		assert!(apply(&mut w, 2 * DAY));
		assert!(w.is_allowed(to_address(&eth(2)), 2 * DAY));
	}

	#[test]
	fn refund_immediate_when_timelock_off() {
		let mut r = RefundAddress::default();
		r.register(eth(1), 0, 0);
		assert_eq!(r.effective(0), Some(&eth(1)));
		// A repoint while off is also immediate.
		r.register(eth(2), 100, 0);
		assert_eq!(r.effective(100), Some(&eth(2)));
	}

	#[test]
	fn refund_repoint_is_timelocked_and_old_stays_active() {
		let mut r = RefundAddress::default();
		// active: eth(1)
		r.register(eth(1), 0, 0);
		// Repoint under restriction: eth(1) stays active until eth(2) matures.
		r.register(eth(2), 0, DAY);
		assert_eq!(r.effective(0), Some(&eth(1)));
		assert_eq!(r.effective(DAY - 1), Some(&eth(1)));
		assert_eq!(r.effective(DAY), Some(&eth(2)));
	}

	#[test]
	fn refund_first_registration_has_no_active_until_matured() {
		let mut r = RefundAddress::default();
		// First-ever registration under restriction: nothing active until it matures.
		r.register(eth(1), 0, DAY);
		assert_eq!(r.effective(0), None);
		assert_eq!(r.effective(DAY - 1), None);
		assert_eq!(r.effective(DAY), Some(&eth(1)));
	}

	#[test]
	fn refund_pending_change_can_be_replaced() {
		let mut r = RefundAddress::default();
		r.register(eth(1), 0, 0); // active: eth(1)
		r.register(eth(2), 0, DAY); // pending: eth(2) @ DAY
							  // A pending change can be freely replaced; this reschedules it for a fresh `now +
							  // timelock`, and the active eth(1) stays put until then.
		r.register(eth(3), 100, DAY);
		assert_eq!(r.effective(DAY), Some(&eth(1)));
		assert_eq!(r.effective(100 + DAY), Some(&eth(3)));
	}

	#[test]
	fn refund_is_registered_covers_effective_and_pending() {
		let mut r = RefundAddress::default();
		assert!(!r.is_registered());
		// A pending first registration counts as registered even before it matures.
		r.register(eth(1), 0, DAY);
		assert!(r.is_registered());
		// Still registered once effective.
		let mut r = RefundAddress::default();
		r.register(eth(1), 0, 0);
		assert!(r.is_registered());
	}
}
