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

//! Per-account withdrawal-whitelist state.
//!
//! [`WithdrawalWhitelist`] holds an account's *active* state only — the external/internal
//! allowlists and the timelock. Timelocked changes live in the pallet's
//! pending queue ([`crate::PendingChanges`]) and are applied eagerly by `on_idle`, so this struct
//! always reflects current truth and all reads are pure.

use cf_chains::{AccountOrAddress, ForeignChain, ForeignChainAddress};
use frame_support::{pallet_prelude::*, DefaultNoBound};
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet};

/// A duration or a point in time, in seconds. The whitelist timelock works in wall-clock time
/// (via the pallet's `TimeSource`), matching cf-funding's redemption timelock.
pub type Seconds = u64;

/// A change to an account's withdrawal whitelist.
///
/// Generic over the address representation: the `update_whitelist` extrinsic input uses
/// `EncodedAddress`, which is decoded into the internal [`ForeignChainAddress`] form before being
/// scheduled and emitted in events.
#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum WhitelistChange<AccountId, Address> {
	Allow(AccountOrAddress<AccountId, Address>),
	Remove(AccountOrAddress<AccountId, Address>),
}

/// Why a whitelist change could not be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApplyChangeError {
	/// The active whitelist is already at its maximum number of entries.
	WhitelistFull,
}

/// A timelocked change awaiting activation in the pallet's pending queue.
#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum PendingChange<AccountId> {
	/// An allowlist change.
	Whitelist(WhitelistChange<AccountId, ForeignChainAddress>),
	/// A whitelist timelock update.
	Timelock(Seconds),
	/// A refund address update (the chain is implied by the address).
	RefundAddress(ForeignChainAddress),
}

impl<AccountId> PendingChange<AccountId> {
	/// Whether scheduling `self` replaces an already-pending `other` rather than stacking: at
	/// most one timelock change and one refund address change per chain can be in flight.
	/// Replacement can only ever push activation later (fresh `now + timelock`), never forward.
	pub(crate) fn replaces(&self, other: &Self) -> bool {
		match (self, other) {
			(PendingChange::Timelock(_), PendingChange::Timelock(_)) => true,
			(PendingChange::RefundAddress(new), PendingChange::RefundAddress(old)) =>
				new.chain() == old.chain(),
			_ => false,
		}
	}
}

/// An account's active withdrawal-whitelist state: the external/internal allowlists and the
/// whitelist timelock.
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
	/// Active external destinations per chain. An empty/absent set for a chain = nothing allowed
	/// there (while the restriction is on).
	external: BTreeMap<ForeignChain, BTreeSet<ForeignChainAddress>>,
	/// Active internal (account) destinations.
	internal: BTreeSet<AccountId>,
	/// The delay applied to whitelist changes (`0` = changes apply immediately). Setting a
	/// timelock also enforces the allowlist even while it has no entries.
	timelock: Seconds,
}

impl<AccountId: Ord + Clone> WithdrawalWhitelist<AccountId> {
	/// Whether `dest` is an allowed destination.
	///
	/// Enforcement is **account-wide and fail-safe**: every destination must be explicitly
	/// allowlisted, and anything unconfigured — a chain or the internal set with no entries — is
	/// blocked. This stops funds being moved out via an unconfigured route, e.g. swapping into an
	/// un-allowlisted chain and withdrawing there.
	///
	/// The *unrestricted* state (nothing configured => everything allowed) is represented by the
	/// absence of the stored whitelist, handled at the pallet level; a default-valued whitelist
	/// is never stored.
	pub(crate) fn is_allowed(
		&self,
		dest: AccountOrAddress<&AccountId, &ForeignChainAddress>,
	) -> bool {
		match dest {
			AccountOrAddress::ExternalAddress(address) =>
				self.external.get(&address.chain()).is_some_and(|set| set.contains(address)),
			AccountOrAddress::InternalAccount(account) => self.internal.contains(account),
		}
	}

	pub(crate) fn timelock(&self) -> Seconds {
		self.timelock
	}

	pub(crate) fn set_timelock(&mut self, timelock: Seconds) {
		self.timelock = timelock;
	}

	/// Applies a single change to the active allowlists, pruning a chain's set once it empties.
	///
	/// An `Allow` that would grow the active set beyond `max_entries` is rejected — this bounds
	/// the stored whitelist, which is decoded on every withdrawal. `Remove` always applies, so an
	/// account can always shrink its allowlist.
	pub(crate) fn apply_change(
		&mut self,
		change: &WhitelistChange<AccountId, ForeignChainAddress>,
		max_entries: u32,
	) -> Result<(), ApplyChangeError> {
		let (destination, allow) = match change {
			WhitelistChange::Allow(destination) => (destination, true),
			WhitelistChange::Remove(destination) => (destination, false),
		};
		if allow && self.active_entry_count() >= max_entries {
			return Err(ApplyChangeError::WhitelistFull);
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
		Ok(())
	}

	/// Number of active allowlist entries: external addresses across all chains plus internal
	/// accounts.
	fn active_entry_count(&self) -> u32 {
		self.external
			.values()
			.fold(0u32, |acc, set| acc.saturating_add(set.len() as u32))
			.saturating_add(self.internal.len() as u32)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::H160;

	type AccountId = u64;
	type Whitelist = WithdrawalWhitelist<AccountId>;

	const DAY: Seconds = 24 * 3600;
	const MAX_ENTRIES: u32 = 100;

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
	fn enforcement_is_fail_safe_and_account_wide() {
		let mut w = Whitelist::default();
		w.apply_change(&allow(eth(1)), MAX_ENTRIES).unwrap();
		assert!(w.is_allowed(to_address(&eth(1))));
		assert!(!w.is_allowed(to_address(&eth(2)))); // same chain, not listed
		assert!(!w.is_allowed(to_address(&arb(1)))); // unconfigured chain => blocked
		assert!(!w.is_allowed(to_account(&1))); // internal set empty => blocked
	}

	#[test]
	fn removing_last_address_keeps_blocking() {
		let mut w = Whitelist::default();
		w.apply_change(&allow(eth(1)), MAX_ENTRIES).unwrap();
		w.apply_change(&remove(eth(1)), MAX_ENTRIES).unwrap();
		// A stored whitelist with no entries blocks everything (fail-safe); whether an emptied
		// whitelist is *dropped* — returning the account to the unrestricted state — is the
		// pallet's decision (it keeps it while a timelock is set).
		assert!(!w.is_allowed(to_address(&eth(1))));
		assert!(!w.is_allowed(to_address(&eth(2))));
	}

	#[test]
	fn internal_account_allowlist_enforcement() {
		let mut w = Whitelist::default();
		w.apply_change(&allow_account(1), MAX_ENTRIES).unwrap();

		assert!(w.is_allowed(to_account(&1)));
		assert!(!w.is_allowed(to_account(&2)));
		// External destinations are gated by the same account-wide fail-safe — an unconfigured
		// chain is blocked.
		assert!(!w.is_allowed(to_address(&eth(1))));
	}

	#[test]
	fn allow_is_dropped_at_entry_cap() {
		let mut w = Whitelist::default();
		let max_entries = 2;
		w.apply_change(&allow(eth(1)), max_entries).unwrap();
		w.apply_change(&allow_account(7), max_entries).unwrap();
		// At the cap: a further Allow is rejected...
		assert_eq!(
			w.apply_change(&allow(arb(1)), max_entries),
			Err(ApplyChangeError::WhitelistFull)
		);
		assert!(!w.is_allowed(to_address(&arb(1))));
		// ...but a Remove always applies, freeing a slot.
		w.apply_change(&remove(eth(1)), max_entries).unwrap();
		w.apply_change(&allow(arb(1)), max_entries).unwrap();
		assert!(w.is_allowed(to_address(&arb(1))));
	}

	#[test]
	fn pending_change_replacement_rules() {
		let timelock = PendingChange::<AccountId>::Timelock(DAY);
		let refund_eth = PendingChange::<AccountId>::RefundAddress(eth(1));
		let refund_eth2 = PendingChange::<AccountId>::RefundAddress(eth(2));
		let refund_arb = PendingChange::<AccountId>::RefundAddress(arb(1));
		let whitelist = PendingChange::Whitelist(allow(eth(1)));

		assert!(PendingChange::<AccountId>::Timelock(0).replaces(&timelock));
		assert!(refund_eth2.replaces(&refund_eth)); // same chain
		assert!(!refund_arb.replaces(&refund_eth)); // different chain
		assert!(!whitelist.replaces(&whitelist.clone())); // whitelist changes stack
		assert!(!timelock.replaces(&refund_eth));
	}
}
