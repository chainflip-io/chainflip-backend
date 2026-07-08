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

#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{
	address::{AddressConverter, EncodedAddress},
	assets::any::AssetMap,
	AccountOrAddress, AnyChain, ForeignChain, ForeignChainAddress,
};
use cf_primitives::{AccountId, AccountRole, Asset, AssetAmount};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	impl_pallet_safe_mode, AccountRoleRegistry, AssetWithholding, BalanceApi, Chainflip,
	DeregistrationCheck, EgressApi, KeyProvider, LiabilityTracker, PoolApi, RefundAddressRegistry,
	ScheduledEgressDetails, WithdrawalAddressRestriction,
};
use cf_utilities::derive_common_traits;
use frame_support::{
	pallet_prelude::*,
	sp_runtime::traits::{Saturating, Zero},
	storage::transactional::with_storage_layer,
	traits::{ConstU64, DefensiveSaturating, OnKilledAccount, UnixTime},
};
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod migrations;

pub mod weights;
pub use weights::WeightInfo;

pub mod whitelist;
use whitelist::*;

pub const STORAGE_VERSION_U16: u16 = 2;
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(STORAGE_VERSION_U16);

#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum ExternalOwner {
	Vault,
	AggKey,
	Account(ForeignChainAddress),
}

impl From<ForeignChainAddress> for ExternalOwner {
	fn from(address: ForeignChainAddress) -> Self {
		ExternalOwner::Account(address)
	}
}

// The implementation for the **Ord** trait is required to use ExternalOwner as a key in a BTreeMap.
impl core::cmp::Ord for ExternalOwner {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		match (self, other) {
			(ExternalOwner::Vault, ExternalOwner::Vault) => core::cmp::Ordering::Equal,
			(ExternalOwner::Vault, _) => core::cmp::Ordering::Less,
			(_, ExternalOwner::Vault) => core::cmp::Ordering::Greater,
			(ExternalOwner::AggKey, ExternalOwner::AggKey) => core::cmp::Ordering::Equal,
			(ExternalOwner::AggKey, ExternalOwner::Account(_)) => core::cmp::Ordering::Less,
			(ExternalOwner::Account(a), ExternalOwner::Account(b)) => a.cmp(b),
			(ExternalOwner::Account(_), _) => core::cmp::Ordering::Greater,
		}
	}
}

impl core::cmp::PartialOrd for ExternalOwner {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

derive_common_traits! {
	#[derive(TypeInfo)]
	pub enum PalletConfigUpdate {
		RefundFeeMultiple { chain: ForeignChain, multiple: Option<u32> },
		/// Maximum whitelist timelock duration.
		MaxWhitelistTimelock { seconds: DurationSeconds },
		/// Maximum number of pending whitelist updates per account.
		MaxPendingWhitelistUpdates { count: u32 },
		/// Maximum number of active whitelist entries per account.
		MaxWhitelistEntries { count: u32 },
	}
}

impl_pallet_safe_mode!(PalletSafeMode; reconciliation_enabled);

#[frame_support::pallet]
pub mod pallet {
	use cf_chains::{dot::PolkadotCrypto, ForeignChain};
	use cf_primitives::EgressId;
	use frame_system::{
		ensure_signed,
		pallet_prelude::{BlockNumberFor, OriginFor},
	};

	use super::*;
	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Handles egress for all chains.
		type EgressHandler: EgressApi<AnyChain>;

		/// Polkadot environment.
		type PolkadotKeyProvider: KeyProvider<PolkadotCrypto>;

		type PoolApi: PoolApi<AccountId = Self::AccountId>;

		/// Converts between encoded (wire) and internal address representations.
		type AddressConverter: AddressConverter;

		/// Wall-clock time source for the whitelist timelock (seconds).
		type TimeSource: UnixTime;

		/// Safe mode configuration.
		type SafeMode: Get<PalletSafeMode>;

		/// Benchmark weights.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Refund amount is too low to cover the egress fees. In this case, the refund is skipped.
		RefundAmountTooLow,
		/// The user does not have enough funds.
		InsufficientBalance,
		/// The user has reached the maximum balance.
		BalanceOverflow,
		/// The Chain is deprecated.
		ChainDeprecated,
		/// The account still has free balance.
		FundsRemaining,
		/// The withdrawal destination is not on the account's withdrawal allowlist.
		DestinationNotAllowed,
		/// The provided address could not be decoded.
		InvalidEncodedAddress,
		/// The requested timelock exceeds the configured maximum.
		TimelockExceedsMaximum,
		/// The account has too many pending allowlist updates.
		TooManyPendingUpdates,
		/// No liquidity refund address is registered for the account on the relevant chain.
		NoLiquidityRefundAddressRegistered,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Refund scheduled for a validator.
		RefundScheduled {
			egress_id: EgressId,
			destination: ForeignChainAddress,
			amount: AssetAmount,
		},
		/// The refund was skipped because of the given reason.
		RefundSkipped {
			reason: DispatchError,
			chain: ForeignChain,
			address: ForeignChainAddress,
		},
		/// The Vault is running a deficit: we owe more than we have set aside for refunds.
		VaultDeficitDetected {
			chain: ForeignChain,
			amount_owed: AssetAmount,
			available: AssetAmount,
		},
		/// The account was debited.
		AccountDebited {
			account_id: T::AccountId,
			asset: Asset,
			amount_debited: AssetAmount,
			new_balance: AssetAmount,
		},
		/// The account was credited.
		AccountCredited {
			account_id: T::AccountId,
			asset: Asset,
			amount_credited: AssetAmount,
			new_balance: AssetAmount,
		},
		PalletConfigUpdated {
			update: PalletConfigUpdate,
		},
		/// An allowlist change was accepted.
		WithdrawalAllowlistUpdateScheduled {
			account_id: T::AccountId,
			change: WhitelistChange<T::AccountId, ForeignChainAddress>,
			apply_at: DurationSeconds,
		},
		/// An account's whitelist timelock was updated.
		WhitelistTimelockUpdated {
			account_id: T::AccountId,
			duration: DurationSeconds,
			effective_at: DurationSeconds,
		},
	}

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	/// Liabilities are funds that are owed to some external party.
	#[pallet::storage]
	pub type Liabilities<T: Config> =
		StorageMap<_, Twox64Concat, Asset, BTreeMap<ExternalOwner, AssetAmount>, ValueQuery>;

	/// Funds that have been set aside to refund external [Liabilities].
	#[pallet::storage]
	pub type WithheldAssets<T: Config> =
		StorageMap<_, Twox64Concat, Asset, AssetAmount, ValueQuery>;

	#[pallet::storage]
	/// Storage for user's free balances.
	pub type FreeBalances<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		Twox64Concat,
		Asset,
		AssetAmount,
		ValueQuery,
	>;

	#[pallet::storage]
	pub type RefundFeeMultiple<T> =
		StorageMap<_, Twox64Concat, ForeignChain, u32, ValueQuery, ConstU32<100>>;

	/// Per-account withdrawal whitelist: the *active* external/internal allowlists and the
	/// timelock. Timelocked changes live in [`PendingChanges`] until they are applied, so this
	/// always reflects current truth.
	///
	/// `None` = unrestricted (nothing configured); a stored whitelist = enforcement on. A
	/// default-valued whitelist is never stored (see [`Pallet::mutate_whitelist`]).
	#[pallet::storage]
	pub type WithdrawalWhitelists<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, WithdrawalWhitelist<T::AccountId>>;

	/// Timelocked changes awaiting activation, keyed by activation time (same-time changes keep
	/// submission order). A single value, so `on_idle` can tell whether anything is due with one
	/// read. Bounded per account: at most [`MaxPendingWhitelistUpdates`] whitelist changes, one
	/// timelock change, and one refund address change per chain can be in flight at a time.
	#[pallet::storage]
	pub type PendingChanges<T: Config> = StorageValue<
		_,
		BTreeMap<DurationSeconds, Vec<(T::AccountId, PendingChange<T::AccountId>)>>,
		ValueQuery,
	>;

	/// Maximum whitelist timelock duration (seconds). Governance-updatable via
	/// [`PalletConfigUpdate::MaxWhitelistTimelock`]. Defaults to 10 days.
	#[pallet::storage]
	pub type MaxWhitelistTimelock<T> =
		StorageValue<_, DurationSeconds, ValueQuery, ConstU64<{ 10 * 24 * 3600 }>>;

	/// Maximum number of pending allowlist updates per account. Governance-updatable via
	/// [`PalletConfigUpdate::MaxPendingWhitelistUpdates`]. Defaults to 16.
	#[pallet::storage]
	pub type MaxPendingWhitelistUpdates<T> = StorageValue<_, u32, ValueQuery, ConstU32<16>>;

	/// Maximum number of active allowlist entries per account (external addresses across all chains
	/// plus internal accounts).
	#[pallet::storage]
	pub type MaxWhitelistEntries<T> = StorageValue<_, u32, ValueQuery, ConstU32<100>>;

	/// The refund address registered by an account for each chain. A registered refund address is
	/// a trusted destination, so it is implicitly allowed by the withdrawal allowlist (see
	/// [`Pallet::ensure_withdrawal_allowed_to`]).
	#[pallet::storage]
	pub type RefundAddresses<T: Config> = StorageDoubleMap<
		_,
		Identity,
		T::AccountId,
		Twox64Concat,
		ForeignChain,
		ForeignChainAddress,
	>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::update_pallet_config())]
		pub fn update_pallet_config(
			origin: OriginFor<T>,
			update: PalletConfigUpdate,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			match update {
				PalletConfigUpdate::RefundFeeMultiple { chain, multiple } => {
					if let Some(value) = multiple {
						RefundFeeMultiple::<T>::insert(chain, value);
					} else {
						RefundFeeMultiple::<T>::remove(chain);
					}
				},
				PalletConfigUpdate::MaxWhitelistTimelock { seconds } => {
					MaxWhitelistTimelock::<T>::put(seconds);
				},
				PalletConfigUpdate::MaxPendingWhitelistUpdates { count } => {
					MaxPendingWhitelistUpdates::<T>::put(count);
				},
				PalletConfigUpdate::MaxWhitelistEntries { count } => {
					MaxWhitelistEntries::<T>::put(count);
				},
			}

			Self::deposit_event(Event::<T>::PalletConfigUpdated { update });

			Ok(())
		}

		/// Add or remove a destination from the caller's withdrawal allowlist. The change is
		/// scheduled and takes effect after the caller's timelock elapses (at the end of the
		/// current block when no timelock is set).
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::update_whitelist())]
		pub fn update_whitelist(
			origin: OriginFor<T>,
			change: WhitelistChange<T::AccountId, EncodedAddress>,
		) -> DispatchResult {
			let account_id = ensure_signed(origin)?;

			let decode = |destination: AccountOrAddress<T::AccountId, EncodedAddress>| {
				destination
					.try_into_decoded::<T::AddressConverter>()
					.map_err(|()| Error::<T>::InvalidEncodedAddress)
			};
			let change = match change {
				WhitelistChange::Allow(destination) => WhitelistChange::Allow(decode(destination)?),
				WhitelistChange::Remove(destination) =>
					WhitelistChange::Remove(decode(destination)?),
			};

			let timelock =
				WithdrawalWhitelists::<T>::get(&account_id).unwrap_or_default().timelock();
			let apply_at = Self::schedule_or_apply_change(
				&account_id,
				PendingChange::Whitelist(change.clone()),
				timelock,
			)?;

			Self::deposit_event(Event::<T>::WithdrawalAllowlistUpdateScheduled {
				account_id,
				change,
				apply_at,
			});

			Ok(())
		}

		/// Set the caller's whitelist timelock. Like any other change, the update is delayed by
		/// the current timelock, so a stolen key can't instantly remove the protection. Since a
		/// new timelock change replaces a pending one, the owner can still *cancel* a pending
		/// malicious change at any time by scheduling their own — the recovery lever against a
		/// stolen key.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::set_whitelist_timelock())]
		pub fn set_whitelist_timelock(
			origin: OriginFor<T>,
			duration: DurationSeconds,
		) -> DispatchResult {
			let account_id = ensure_signed(origin)?;
			ensure!(
				duration <= MaxWhitelistTimelock::<T>::get(),
				Error::<T>::TimelockExceedsMaximum
			);

			let current =
				WithdrawalWhitelists::<T>::get(&account_id).unwrap_or_default().timelock();
			let effective_at = Self::schedule_or_apply_change(
				&account_id,
				PendingChange::Timelock(duration),
				current,
			)?;

			Self::deposit_event(Event::<T>::WhitelistTimelockUpdated {
				account_id,
				duration,
				effective_at,
			});

			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Applies pending changes whose activation time has passed, within the block's leftover
		/// weight. Slight delays in activation are possible under sustained full blocks.
		fn on_idle(_block: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let mut used_weight = T::WeightInfo::on_idle_check();
			if remaining_weight.any_lt(used_weight) {
				return Weight::zero();
			}
			let now = Self::now_secs();
			let mut pending = PendingChanges::<T>::get();

			// Abort if no changes matured:
			if !pending.first_key_value().is_some_and(|(&time, _)| time <= now) {
				return used_weight;
			}

			// Apply matured changes only:
			let mut not_due = pending.split_off(&now.saturating_add(1));

			used_weight = used_weight.max(T::WeightInfo::on_idle_apply_change(0));
			let per_change_weight = T::WeightInfo::on_idle_apply_change(1)
				.saturating_sub(T::WeightInfo::on_idle_apply_change(0));

			// Not enough weight to apply even one change: leave the queue untouched.
			if remaining_weight.any_lt(used_weight.saturating_add(per_change_weight)) {
				return T::WeightInfo::on_idle_check();
			}

			// Apply changes while we are within the weight budget (the remainder
			// is carried over, so the next block resumes where this one stopped)
			'apply: while let Some((time, changes)) = pending.pop_first() {
				let mut changes = changes.into_iter();
				while let Some((account, change)) = changes.next() {
					if remaining_weight.any_lt(used_weight.saturating_add(per_change_weight)) {
						let carried_over = not_due.entry(time).or_default();
						carried_over.push((account, change));
						carried_over.extend(changes);
						break 'apply;
					}
					used_weight.saturating_accrue(per_change_weight);
					Self::apply_pending_change(&account, change);
				}
			}
			not_due.append(&mut pending);
			PendingChanges::<T>::set(not_due);
			used_weight
		}
	}
}

impl<T: Config> Pallet<T> {
	fn refund_via_egress(
		chain: ForeignChain,
		address: ForeignChainAddress,
		amount: AssetAmount,
	) -> Result<(), DispatchError> {
		match with_storage_layer(|| {
			T::EgressHandler::schedule_egress(chain.gas_asset(), amount, address.clone(), None)
				.map_err(Into::into)
				.and_then(
					|result @ ScheduledEgressDetails { egress_amount, fee_withheld, .. }| {
						if egress_amount <
							(RefundFeeMultiple::<T>::get(chain) as AssetAmount) * fee_withheld
						{
							Err(Error::<T>::RefundAmountTooLow.into())
						} else {
							Ok(result)
						}
					},
				)
		}) {
			Ok(ScheduledEgressDetails { egress_id, .. }) => {
				Self::deposit_event(Event::RefundScheduled {
					egress_id,
					destination: address,
					amount,
				});
				Ok(())
			},
			Err(err) => {
				Self::deposit_event(Event::RefundSkipped { reason: err, chain, address });
				Err(err)
			},
		}
	}

	// Reconciles the amount owed with the amount available for distribution.
	//
	// The owed and available amount are mutated in place.
	//
	// For Ethereum, Arbitrum and Bsc, we expect to pay out the validators via egress to their
	// accounts. For Polkadot, we expect to pay out to the current AggKey account.
	// For Bitcoin and Solana, the vault pays the fees directly so we don't need to egress
	// anything.
	//
	// Note that we refund to accounts atomically (we never partially refund an account), whereas
	// refunds to vaults or agg-keys can be made incrementally.
	fn reconcile(
		chain: ForeignChain,
		owner: &ExternalOwner,
		amount_owed: &mut AssetAmount,
		available: &mut AssetAmount,
	) -> Result<(), DispatchError> {
		let amount_reconciled = match chain {
			ForeignChain::Ethereum |
			ForeignChain::Arbitrum |
			ForeignChain::Tron |
			ForeignChain::Bsc => match owner {
				ExternalOwner::Account(address) =>
					if *amount_owed > *available {
						0
					} else {
						Self::refund_via_egress(chain, address.clone(), *amount_owed)?;
						*amount_owed
					},
				other => {
					log_or_panic!(
						"Expected ExternalOwner::Account for EVM chains, got {:?}.",
						other
					);
					0
				},
			},
			ForeignChain::Polkadot | ForeignChain::Assethub => match owner {
				ExternalOwner::AggKey => {
					if let Some(active_key) = T::PolkadotKeyProvider::active_epoch_key() {
						// Polkadot is deprecated.
						if chain == ForeignChain::Polkadot {
							Self::deposit_event(Event::RefundSkipped {
								reason: Error::<T>::ChainDeprecated.into(),
								chain,
								address: ForeignChainAddress::Dot(active_key.key),
							});
						}
						let refund_amount = core::cmp::min(*amount_owed, *available);
						Self::refund_via_egress(
							chain,
							ForeignChainAddress::Dot(active_key.key),
							refund_amount,
						)?;
						refund_amount
					} else {
						log_or_panic!("No active epoch key found for Polkadot.");
						0
					}
				},
				other => {
					log_or_panic!(
						"{:?} Liabilities are not supported for chain {:?}.",
						other,
						chain
					);
					0
				},
			},
			ForeignChain::Bitcoin | ForeignChain::Solana => match owner {
				ExternalOwner::Vault => core::cmp::min(*amount_owed, *available),
				other => {
					log_or_panic!(
						"{:?} Liabilities are not supported for chain {:?}.",
						other,
						chain
					);
					0
				},
			},
		};

		available.defensive_saturating_reduce(amount_reconciled);
		amount_owed.defensive_saturating_reduce(amount_reconciled);

		Ok(())
	}

	/// Triggers the reconciliation process for all chains.
	/// This function will refund the owed assets to the appropriate accounts following the chain
	/// specific requirements.
	pub fn trigger_reconciliation() {
		if !T::SafeMode::get().reconciliation_enabled {
			log::info!("Reconciliation is disabled. Skipping reconciliation.");
			return;
		}

		for chain in ForeignChain::iter() {
			WithheldAssets::<T>::mutate(chain.gas_asset(), |total_withheld| {
				let mut owed_assets =
					Liabilities::<T>::take(chain.gas_asset()).into_iter().collect::<Vec<_>>();
				owed_assets.sort_by_key(|(_, amount)| core::cmp::Reverse(*amount));

				for (destination, amount) in owed_assets.iter_mut() {
					debug_assert!(*amount > 0);
					let _ = Self::reconcile(chain, destination, amount, total_withheld);
					if *total_withheld == 0 {
						break;
					}
				}

				owed_assets.retain(|(_, amount)| *amount > 0);
				if !owed_assets.is_empty() {
					Self::deposit_event(Event::VaultDeficitDetected {
						chain,
						amount_owed: owed_assets
							.iter()
							.map(|(_, amount)| amount)
							.sum::<AssetAmount>(),
						available: *total_withheld,
					});
					Liabilities::<T>::insert(
						chain.gas_asset(),
						owed_assets.into_iter().collect::<BTreeMap<_, _>>(),
					);
				}
			});
		}
	}

	pub fn vault_imbalance(asset: Asset) -> VaultImbalance<AssetAmount> {
		let owed = Liabilities::<T>::get(asset).values().sum::<u128>();
		let withheld = WithheldAssets::<T>::get(asset);
		if owed > withheld {
			VaultImbalance::Deficit(owed - withheld)
		} else {
			VaultImbalance::Surplus(withheld - owed)
		}
	}

	fn now_secs() -> DurationSeconds {
		T::TimeSource::now().as_secs()
	}

	/// Mutates the account's whitelist, dropping the storage entry when it is left in its default
	/// state. (A set timelock alone keeps the entry alive — dropping it would silently disable
	/// the restriction.)
	fn mutate_whitelist<R>(
		who: &T::AccountId,
		f: impl FnOnce(&mut WithdrawalWhitelist<T::AccountId>) -> R,
	) -> R {
		WithdrawalWhitelists::<T>::mutate_exists(who, |maybe| {
			let mut whitelist = maybe.take().unwrap_or_default();
			let result = f(&mut whitelist);
			*maybe = (whitelist != Default::default()).then_some(whitelist);
			result
		})
	}

	/// Schedules `change` to take effect `delay` seconds from now, or applies it right away when
	/// `delay` is 0 (no timelock set). Returns the activation time.
	///
	/// Either way, an already-pending change that the new one supersedes is discarded first (see
	/// [`PendingChange::replaces`]): for single-value changes (timelock, per-chain refund
	/// address) the newest submission must win — a superseded change activating later would
	/// silently overwrite it. Eviction at submission time is also what lets the owner cancel a
	/// pending malicious change (it never matures). Whitelist changes stack instead, capped at
	/// [`MaxPendingWhitelistUpdates`] per account.
	fn schedule_or_apply_change(
		who: &T::AccountId,
		change: PendingChange<T::AccountId>,
		delay: DurationSeconds,
	) -> Result<DurationSeconds, Error<T>> {
		Self::discard_pending_matching(who, |existing| change.replaces(existing));
		if delay == 0 {
			Self::apply_pending_change(who, change);
			return Ok(Self::now_secs());
		}
		let apply_at = Self::now_secs().saturating_add(delay);
		PendingChanges::<T>::try_mutate(|pending| {
			if matches!(change, PendingChange::Whitelist(_)) {
				ensure!(
					pending
						.values()
						.flatten()
						.filter(|(account, change)| account == who &&
							matches!(change, PendingChange::Whitelist(_)))
						.count() < MaxPendingWhitelistUpdates::<T>::get() as usize,
					Error::<T>::TooManyPendingUpdates
				);
			}
			pending.entry(apply_at).or_default().push((who.clone(), change));
			Ok(apply_at)
		})
	}

	/// Discards all of the account's pending changes matching `filter`.
	fn discard_pending_matching(
		who: &T::AccountId,
		filter: impl Fn(&PendingChange<T::AccountId>) -> bool,
	) {
		PendingChanges::<T>::mutate(|pending| {
			pending.retain(|_, changes| {
				changes.retain(|(account, change)| account != who || !filter(change));
				!changes.is_empty()
			});
		});
	}

	fn apply_pending_change(who: &T::AccountId, change: PendingChange<T::AccountId>) {
		match change {
			PendingChange::Whitelist(whitelist_change) =>
				Self::mutate_whitelist(who, |whitelist| {
					whitelist.apply_change(&whitelist_change, MaxWhitelistEntries::<T>::get())
				}),
			PendingChange::Timelock(timelock) =>
				Self::mutate_whitelist(who, |whitelist| whitelist.set_timelock(timelock)),
			PendingChange::RefundAddress(address) =>
				RefundAddresses::<T>::insert(who, address.chain(), address),
		}
	}
}

impl<T: Config> WithdrawalAddressRestriction for Pallet<T> {
	type AccountId = T::AccountId;

	fn ensure_withdrawal_allowed_to(
		owner: &Self::AccountId,
		dest: AccountOrAddress<&Self::AccountId, &ForeignChainAddress>,
	) -> DispatchResult {
		// Extract the external address (if any) before `dest` is consumed by `is_allowed`, so a
		// registered refund address can be treated as implicitly allowed without an extra clone.
		let external = match &dest {
			AccountOrAddress::ExternalAddress(address) => Some(*address),
			AccountOrAddress::InternalAccount(_) => None,
		};
		// No stored whitelist = unrestricted.
		let allowed = WithdrawalWhitelists::<T>::get(owner)
			.is_none_or(|whitelist| whitelist.is_allowed(dest)) ||
			external.is_some_and(|address| {
				// The registered refund address is implicitly allowed; a pending (timelocked)
				// repoint is not, so the old address stays the only allowed one until the new one
				// takes effect.
				RefundAddresses::<T>::get(owner, address.chain()).as_ref() == Some(address)
			});
		ensure!(allowed, Error::<T>::DestinationNotAllowed);
		Ok(())
	}
}

impl<T: Config> RefundAddressRegistry for Pallet<T> {
	type AccountId = T::AccountId;

	fn register_liquidity_refund_address(who: &Self::AccountId, address: ForeignChainAddress) {
		// Repointing a refund address is delayed by the account's timelock (0 = immediate); the
		// current address stays registered until the new one takes effect.
		let timelock = WithdrawalWhitelists::<T>::get(who).unwrap_or_default().timelock();
		// Cannot fail: the pending cap only applies to whitelist changes.
		let _ =
			Self::schedule_or_apply_change(who, PendingChange::RefundAddress(address), timelock);
	}

	fn get_refund_address(
		who: &Self::AccountId,
		chain: ForeignChain,
	) -> Option<ForeignChainAddress> {
		RefundAddresses::<T>::get(who, chain)
	}

	fn clear_refund_addresses(who: &Self::AccountId) {
		let _ = RefundAddresses::<T>::clear_prefix(who, u32::MAX, None);
		// Also drop the account's pending refund address changes — they would otherwise
		// re-register an address after this cleanup.
		Self::discard_pending_matching(who, |change| {
			matches!(change, PendingChange::RefundAddress(_))
		});
	}

	/// Make sure refund address is either set or will be set after timelock. (If we didn't allow
	/// the latter, it would result in bad UX where upon adding a new chain the user would have to
	/// wait the timelock period before they can use it. This should be OK because max
	/// timelock is capped, so the address will become active soon.
	fn ensure_has_refund_address_for_asset(who: &Self::AccountId, asset: Asset) -> DispatchResult {
		let chain = ForeignChain::from(asset);
		ensure!(
			RefundAddresses::<T>::contains_key(who, chain) ||
				PendingChanges::<T>::get().values().flatten().any(|(account, change)| {
					account == who &&
						matches!(change, PendingChange::RefundAddress(address) if address.chain() == chain)
				}),
			Error::<T>::NoLiquidityRefundAddressRegistered
		);
		Ok(())
	}
}

#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	Clone,
	PartialEq,
	Eq,
	RuntimeDebug,
	Serialize,
	Deserialize,
)]
pub enum VaultImbalance<A> {
	/// There are more withheld assets than what is owed.
	Surplus(A),
	/// There are more assets owed than what is withheld.
	Deficit(A),
}

impl<A> VaultImbalance<A> {
	pub fn map<B>(&self, f: impl FnOnce(&A) -> B) -> VaultImbalance<B> {
		match self {
			VaultImbalance::Surplus(amount) => VaultImbalance::Surplus(f(amount)),
			VaultImbalance::Deficit(amount) => VaultImbalance::Deficit(f(amount)),
		}
	}
}

impl<T: Config> LiabilityTracker for Pallet<T> {
	fn record_liability(address: ForeignChainAddress, asset: Asset, amount: AssetAmount) {
		debug_assert_eq!(ForeignChain::from(asset), address.chain());
		Liabilities::<T>::mutate(asset, |fees| {
			fees.entry(match ForeignChain::from(asset) {
				ForeignChain::Ethereum |
				ForeignChain::Arbitrum |
				ForeignChain::Tron |
				ForeignChain::Bsc => address.into(),
				ForeignChain::Polkadot | ForeignChain::Assethub => ExternalOwner::AggKey,
				ForeignChain::Bitcoin | ForeignChain::Solana => ExternalOwner::Vault,
			})
			.and_modify(|fee| fee.saturating_accrue(amount))
			.or_insert(amount);
		});
	}
}

impl<T: Config> AssetWithholding for Pallet<T> {
	fn withhold_assets(asset: Asset, amount: AssetAmount) {
		WithheldAssets::<T>::mutate(asset, |fees| {
			fees.saturating_accrue(amount);
		});
	}
}

impl<T: Config> BalanceApi for Pallet<T>
where
	Vec<(AccountId, cf_primitives::Asset, u128)>:
		From<Vec<(<T as frame_system::Config>::AccountId, cf_primitives::Asset, u128)>>,
{
	type AccountId = T::AccountId;

	fn credit_account(account_id: &Self::AccountId, asset: Asset, amount: AssetAmount) {
		if amount == 0 {
			return;
		}

		let new_balance = FreeBalances::<T>::mutate(account_id, asset, |balance| {
			*balance = balance.saturating_add(amount);
			*balance
		});

		Self::deposit_event(Event::AccountCredited {
			account_id: account_id.clone(),
			asset,
			amount_credited: amount,
			new_balance,
		});
	}

	fn try_credit_account(
		account_id: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> DispatchResult {
		// Check if the result would overflow:
		FreeBalances::<T>::get(account_id, asset)
			.checked_add(amount)
			.ok_or(Error::<T>::BalanceOverflow)?;

		Self::credit_account(account_id, asset, amount);

		Ok(())
	}

	fn try_debit_account(
		account_id: &Self::AccountId,
		asset: Asset,
		amount: AssetAmount,
	) -> DispatchResult {
		if amount == 0 {
			return Ok(())
		}

		// Sweep LP earnings before debiting to ensure all available funds are accounted for
		if T::AccountRoleRegistry::has_account_role(account_id, AccountRole::LiquidityProvider) {
			T::PoolApi::sweep(account_id)?;
		}

		let new_balance = FreeBalances::<T>::try_mutate_exists(account_id, asset, |balance| {
			let new_balance = match balance.take() {
				None => Err(Error::<T>::InsufficientBalance),
				Some(balance) =>
					Ok(balance.checked_sub(amount).ok_or(Error::<T>::InsufficientBalance)?),
			}?;
			if new_balance > 0 {
				*balance = Some(new_balance);
			}
			Ok::<_, Error<T>>(new_balance)
		})?;

		Self::deposit_event(Event::AccountDebited {
			account_id: account_id.clone(),
			asset,
			amount_debited: amount,
			new_balance,
		});

		Ok(())
	}

	fn free_balances(who: &Self::AccountId) -> AssetMap<AssetAmount> {
		let _ = T::PoolApi::sweep(who);
		AssetMap::from_fn(|asset| FreeBalances::<T>::get(who, asset))
	}

	fn free_balances_dont_sweep(who: &Self::AccountId) -> AssetMap<AssetAmount> {
		AssetMap::from_fn(|asset| FreeBalances::<T>::get(who, asset))
	}

	fn get_balance(who: &Self::AccountId, asset: Asset) -> AssetAmount {
		FreeBalances::<T>::get(who, asset)
	}
}

pub struct FreeBalancesDeregistrationCheck<T: Config>(PhantomData<T>);

impl<T: Config> DeregistrationCheck for FreeBalancesDeregistrationCheck<T> {
	type AccountId = T::AccountId;
	type Error = Error<T>;

	fn check(account_id: &Self::AccountId) -> Result<(), Self::Error> {
		ensure!(
			FreeBalances::<T>::iter_prefix(account_id).all(|(_, amount)| amount.is_zero()),
			Error::<T>::FundsRemaining
		);

		Ok(())
	}
}

pub struct DeleteAccount<T: Config>(PhantomData<T>);

impl<T: Config> OnKilledAccount<T::AccountId> for DeleteAccount<T> {
	fn on_killed_account(who: &T::AccountId) {
		let _ = FreeBalances::<T>::clear_prefix(who, u32::MAX, None);
	}
}
