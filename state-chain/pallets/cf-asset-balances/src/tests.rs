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

use cf_chains::{Ethereum, ForeignChain, ForeignChainAddress};
use cf_primitives::{AccountId, AssetAmount};
use cf_traits::{
	mocks::egress_handler::MockEgressParameter, AssetWithholding, LiabilityTracker, SetSafeMode,
};

use crate::{Event, FreeBalances};
use cf_chains::AnyChain;
use cf_test_utilities::assert_has_event;
use cf_traits::{mocks::egress_handler::MockEgressHandler, BalanceApi, SafeMode};
use frame_support::{assert_noop, assert_ok, traits::OnKilledAccount};

use crate::{
	mock::*, ExternalOwner, Liabilities, Pallet, PalletConfigUpdate, RefundFeeMultiple,
	WithheldAssets,
};

fn payed_gas(chain: ForeignChain, amount: AssetAmount, account: ForeignChainAddress) {
	Pallet::<Test>::record_liability(account, chain.gas_asset(), amount);
	Pallet::<Test>::withhold_assets(chain.gas_asset(), amount);
}

#[track_caller]
fn assert_egress(
	number_of_egresses: usize,
	maybe_additional_conditions: Option<fn(egresses: Vec<MockEgressParameter<AnyChain>>)>,
) {
	let egresses = MockEgressHandler::<AnyChain>::get_scheduled_egresses();
	assert_eq!(egresses.len(), number_of_egresses);
	if let Some(additional_conditions) = maybe_additional_conditions {
		additional_conditions(egresses);
	}
}

#[test]
fn skip_refunding_if_safe_mode_is_enabled() {
	new_test_ext().execute_with(|| {
		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_1.clone());

		let recorded_fees_eth = Liabilities::<Test>::get(ForeignChain::Ethereum.gas_asset());

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1.into()), Some(&100));
		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset()), 100);

		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			refunding: crate::PalletSafeMode::code_red(),
		});

		Pallet::<Test>::trigger_reconciliation();

		assert_egress(0, None);

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1.into()), Some(&100));
		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset()), 100);
	});
}

#[test]
pub fn keep_fees_in_storage_if_egress_fails() {
	new_test_ext().execute_with(|| {
		MockEgressHandler::<AnyChain>::return_failure(true);

		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_1.clone());

		let recorded_fees_eth = Liabilities::<Test>::get(ForeignChain::Ethereum.gas_asset());

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1.into()), Some(&100));
		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset()), 100);

		Pallet::<Test>::trigger_reconciliation();

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1.into()), Some(&100));
		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset()), 100);
	});
}

#[test]
pub fn not_enough_withheld_fees() {
	new_test_ext().execute_with(|| {
		const BTC_OWED: u128 = 100;
		const BTC_AVAILABLE: u128 = 99;
		const ETH_OWED: u128 = 200;
		const ETH_AVAILABLE: u128 = 199;
		payed_gas(ForeignChain::Bitcoin, BTC_OWED, BTC_ADDR_1.clone());
		WithheldAssets::<Test>::insert(ForeignChain::Bitcoin.gas_asset(), BTC_AVAILABLE);

		payed_gas(ForeignChain::Ethereum, ETH_OWED, ETH_ADDR_1.clone());
		WithheldAssets::<Test>::insert(ForeignChain::Ethereum.gas_asset(), ETH_AVAILABLE);

		Pallet::<Test>::trigger_reconciliation();

		System::assert_has_event(RuntimeEvent::AssetBalances(Event::VaultDeficitDetected {
			chain: ForeignChain::Bitcoin,
			amount_owed: BTC_OWED - BTC_AVAILABLE,
			available: 0,
		}));
		System::assert_has_event(RuntimeEvent::AssetBalances(Event::VaultDeficitDetected {
			chain: ForeignChain::Ethereum,
			amount_owed: ETH_OWED,
			available: ETH_AVAILABLE,
		}));

		// For Bitcoin, reconciliate as much as possible.
		assert_eq!(
			Liabilities::<Test>::get(ForeignChain::Bitcoin.gas_asset())[&ExternalOwner::Vault],
			BTC_OWED - BTC_AVAILABLE
		);
		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Bitcoin.gas_asset()), 0);

		// For Ethereum, either refund the entirety or do nothing.
		let recorded_fees_eth = Liabilities::<Test>::get(ForeignChain::Ethereum.gas_asset());
		assert_eq!(recorded_fees_eth[&ExternalOwner::Account(ETH_ADDR_1)], ETH_OWED);
		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset()), ETH_AVAILABLE);
	});
}

#[test]
pub fn do_not_refund_if_amount_is_too_low() {
	new_test_ext().execute_with(|| {
		const REFUND_AMOUNT: u128 = 10;
		payed_gas(ForeignChain::Ethereum, REFUND_AMOUNT, ETH_ADDR_1.clone());

		MockEgressHandler::<Ethereum>::set_fee(REFUND_AMOUNT * 2);
		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset()), REFUND_AMOUNT);

		Pallet::<Test>::trigger_reconciliation();

		assert_has_event::<Test>(
			Event::RefundSkipped {
				reason: crate::Error::<Test>::RefundAmountTooLow.into(),
				chain: ForeignChain::Ethereum,
				address: ETH_ADDR_1,
			}
			.into(),
		);

		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset()), REFUND_AMOUNT);
		assert_eq!(
			Liabilities::<Test>::get(ForeignChain::Ethereum.gas_asset())
				.get(&ExternalOwner::Account(ETH_ADDR_1)),
			Some(&REFUND_AMOUNT)
		);

		assert_egress(0, None);
	});
}

#[test]
fn governance_can_configure_refund_fee_multiple() {
	new_test_ext().execute_with(|| {
		// Defaults to 100 for any chain.
		assert_eq!(RefundFeeMultiple::<Test>::get(ForeignChain::Ethereum), 100);

		// Only governance can update the config.
		assert_noop!(
			Pallet::<Test>::update_pallet_config(
				RuntimeOrigin::signed(AccountId::from([0; 32])),
				PalletConfigUpdate::RefundFeeMultiple {
					chain: ForeignChain::Ethereum,
					multiple: Some(5),
				},
			),
			sp_runtime::traits::BadOrigin
		);

		// Governance sets a new value, which is stored and announced via an event.
		assert_ok!(Pallet::<Test>::update_pallet_config(
			RuntimeOrigin::root(),
			PalletConfigUpdate::RefundFeeMultiple {
				chain: ForeignChain::Ethereum,
				multiple: Some(5),
			},
		));
		assert_eq!(RefundFeeMultiple::<Test>::get(ForeignChain::Ethereum), 5);
		// Other chains keep the default.
		assert_eq!(RefundFeeMultiple::<Test>::get(ForeignChain::Arbitrum), 100);
		assert_has_event::<Test>(
			Event::PalletConfigUpdated {
				update: PalletConfigUpdate::RefundFeeMultiple {
					chain: ForeignChain::Ethereum,
					multiple: Some(5),
				},
			}
			.into(),
		);

		// Clearing the value (None) resets it to the default.
		assert_ok!(Pallet::<Test>::update_pallet_config(
			RuntimeOrigin::root(),
			PalletConfigUpdate::RefundFeeMultiple { chain: ForeignChain::Ethereum, multiple: None },
		));
		assert_eq!(RefundFeeMultiple::<Test>::get(ForeignChain::Ethereum), 100);
	});
}

#[test]
fn configured_refund_fee_multiple_is_respected() {
	new_test_ext().execute_with(|| {
		const REFUND_AMOUNT: u128 = 1_000;
		const EGRESS_FEE: u128 = 20;

		payed_gas(ForeignChain::Ethereum, REFUND_AMOUNT, ETH_ADDR_1.clone());
		MockEgressHandler::<Ethereum>::set_fee(EGRESS_FEE);

		// With the default multiple (100), the refundable amount (980) is below the
		// 100 * 20 = 2000 threshold, so the refund is skipped.
		Pallet::<Test>::trigger_reconciliation();
		assert_has_event::<Test>(
			Event::RefundSkipped {
				reason: crate::Error::<Test>::RefundAmountTooLow.into(),
				chain: ForeignChain::Ethereum,
				address: ETH_ADDR_1,
			}
			.into(),
		);
		assert_egress(0, None);

		// Lower the multiple so the threshold (1 * 20 = 20) is below the refundable amount.
		assert_ok!(Pallet::<Test>::update_pallet_config(
			RuntimeOrigin::root(),
			PalletConfigUpdate::RefundFeeMultiple {
				chain: ForeignChain::Ethereum,
				multiple: Some(1),
			},
		));

		// Now the refund goes through.
		Pallet::<Test>::trigger_reconciliation();
		assert_egress(
			1,
			Some(|egresses: Vec<MockEgressParameter<AnyChain>>| {
				for egress in egresses {
					assert_eq!(egress.amount(), REFUND_AMOUNT - EGRESS_FEE);
				}
			}),
		);
		assert!(Liabilities::<Test>::get(ForeignChain::Ethereum.gas_asset()).is_empty());
	});
}

#[track_caller]
fn test_refund_validators_with_checks(
	chain: ForeignChain,
	addresses: Vec<ForeignChainAddress>,
	liabilities: Vec<ExternalOwner>,
	egress_check: impl Fn(),
) {
	new_test_ext().execute_with(|| {
		const REFUND_AMOUNT: u128 = 1_000u128;
		let asset = chain.gas_asset();

		let total_gas_paid = REFUND_AMOUNT * addresses.len() as u128;
		addresses.into_iter().for_each(|addr| payed_gas(chain, REFUND_AMOUNT, addr));

		assert_eq!(WithheldAssets::<Test>::get(asset), total_gas_paid);

		liabilities.into_iter().for_each(|liability| {
			assert_eq!(Liabilities::<Test>::get(asset).get(&liability), Some(&REFUND_AMOUNT))
		});

		Pallet::<Test>::trigger_reconciliation();

		// Check all fees owed have been paid out.
		assert!(Liabilities::<Test>::get(asset).is_empty());
		assert_eq!(WithheldAssets::<Test>::get(asset), 0);

		egress_check();
	});
}

#[test]
pub fn test_refund_validators() {
	const REFUND_AMOUNT: u128 = 1_000u128;
	test_refund_validators_with_checks(
		ForeignChain::Bitcoin,
		vec![BTC_ADDR_1],
		vec![ExternalOwner::Vault],
		|| assert_egress(0, None),
	);
	test_refund_validators_with_checks(
		ForeignChain::Solana,
		vec![SOL_ADDR],
		vec![ExternalOwner::Vault],
		|| assert_egress(0, None),
	);

	test_refund_validators_with_checks(
		ForeignChain::Polkadot,
		vec![DOT_ADDR_1],
		vec![ExternalOwner::AggKey],
		|| {
			assert_egress(
				1,
				Some(|egresses: Vec<MockEgressParameter<AnyChain>>| {
					for egress in egresses {
						assert_eq!(egress.amount(), REFUND_AMOUNT);
					}
				}),
			)
		},
	);

	test_refund_validators_with_checks(
		ForeignChain::Ethereum,
		vec![ETH_ADDR_1, ETH_ADDR_2],
		vec![ExternalOwner::Account(ETH_ADDR_1), ExternalOwner::Account(ETH_ADDR_2)],
		|| {
			assert_egress(
				2,
				Some(|egresses: Vec<MockEgressParameter<AnyChain>>| {
					for egress in egresses {
						assert_eq!(egress.amount(), REFUND_AMOUNT);
					}
				}),
			)
		},
	);

	test_refund_validators_with_checks(
		ForeignChain::Arbitrum,
		vec![ARB_ADDR_1],
		vec![ExternalOwner::Account(ARB_ADDR_1)],
		|| {
			assert_egress(
				1,
				Some(|egresses: Vec<MockEgressParameter<AnyChain>>| {
					for egress in egresses {
						assert_eq!(egress.amount(), REFUND_AMOUNT);
					}
				}),
			)
		},
	);
}

#[test]
fn can_reconciliate_multiple_chains_at_once() {
	new_test_ext().execute_with(|| {
		const REFUND_AMOUNT: u128 = 1_000u128;

		let test_accounts = vec![
			(ForeignChain::Ethereum, ETH_ADDR_1),
			(ForeignChain::Arbitrum, ARB_ADDR_1),
			(ForeignChain::Polkadot, DOT_ADDR_1),
			(ForeignChain::Bitcoin, BTC_ADDR_1),
			(ForeignChain::Solana, SOL_ADDR),
		];

		test_accounts.iter().for_each(|(chain, acc)| {
			payed_gas(*chain, REFUND_AMOUNT, acc.clone());
			assert_eq!(WithheldAssets::<Test>::get(chain.gas_asset()), REFUND_AMOUNT);
		});

		Pallet::<Test>::trigger_reconciliation();

		assert_egress(
			// Only Ethereum, Arbitrum and Polkadot requires Egress
			3,
			Some(|egresses: Vec<MockEgressParameter<AnyChain>>| {
				for egress in egresses {
					assert_eq!(egress.amount(), REFUND_AMOUNT);
				}
			}),
		);

		test_accounts.into_iter().for_each(|(chain, _)| {
			let asset = chain.gas_asset();
			assert!(Liabilities::<Test>::get(asset).is_empty());
			assert_eq!(WithheldAssets::<Test>::get(asset), 0);
		});
	});
}

pub mod balance_api {
	use crate::{DeleteAccount, FreeBalancesDeregistrationCheck};
	use cf_traits::DeregistrationCheck;

	use super::*;

	use cf_primitives::chains::assets::eth;

	#[test]
	pub fn credit_and_debit() {
		new_test_ext().execute_with(|| {
			let alice = AccountId::from([1; 32]);
			const AMOUNT: u128 = 100;
			const DELTA: u128 = 10;
			Pallet::<Test>::credit_account(&alice, ForeignChain::Ethereum.gas_asset(), AMOUNT);
			assert_has_event::<Test>(
				Event::AccountCredited {
					account_id: alice.clone(),
					asset: ForeignChain::Ethereum.gas_asset(),
					amount_credited: AMOUNT,
					new_balance: AMOUNT,
				}
				.into(),
			);
			assert_eq!(
				FreeBalances::<Test>::get(&alice, ForeignChain::Ethereum.gas_asset()),
				AMOUNT
			);
			assert_noop!(
				Pallet::<Test>::try_debit_account(
					&alice,
					ForeignChain::Ethereum.gas_asset(),
					AMOUNT + DELTA
				),
				crate::Error::<Test>::InsufficientBalance
			);
			assert_ok!(Pallet::<Test>::try_debit_account(
				&AccountId::from([1; 32]),
				ForeignChain::Ethereum.gas_asset(),
				AMOUNT - DELTA
			));
			assert_eq!(
				FreeBalances::<Test>::get(
					AccountId::from([1; 32]),
					ForeignChain::Ethereum.gas_asset()
				),
				DELTA
			);
			assert_has_event::<Test>(
				Event::AccountDebited {
					account_id: alice.clone(),
					asset: ForeignChain::Ethereum.gas_asset(),
					amount_debited: AMOUNT - DELTA,
					new_balance: DELTA,
				}
				.into(),
			);
		});
	}

	#[test]
	pub fn kill_account() {
		new_test_ext().execute_with(|| {
			FreeBalances::<Test>::insert(
				AccountId::from([1; 32]),
				ForeignChain::Ethereum.gas_asset(),
				100,
			);
			DeleteAccount::<Test>::on_killed_account(&AccountId::from([1; 32]));
			assert!(
				FreeBalances::<Test>::get(
					AccountId::from([1; 32]),
					ForeignChain::Ethereum.gas_asset()
				) == 0
			);
		});
	}

	#[test]
	pub fn free_balances() {
		new_test_ext().execute_with(|| {
			FreeBalances::<Test>::insert(
				AccountId::from([1; 32]),
				ForeignChain::Ethereum.gas_asset(),
				100,
			);
			assert_eq!(
				Pallet::<Test>::free_balances(&AccountId::from([1; 32])).eth,
				eth::AssetMap { eth: 100, flip: 0, usdc: 0, usdt: 0, wbtc: 0 }
			);
		});
	}

	#[test]
	fn deregistration_check_fails_when_funds_remain() {
		new_test_ext().execute_with(|| {
			let account = AccountId::from([9; 32]);
			FreeBalances::<Test>::insert(&account, ForeignChain::Ethereum.gas_asset(), 100);

			assert!(matches!(
				FreeBalancesDeregistrationCheck::<Test>::check(&account),
				Err(crate::Error::<Test>::FundsRemaining)
			));
		});
	}

	#[test]
	fn deregistration_check_passes_without_funds() {
		new_test_ext().execute_with(|| {
			let account = AccountId::from([10; 32]);
			assert_ok!(FreeBalancesDeregistrationCheck::<Test>::check(&account));
		});
	}
}

mod withdrawal_whitelist {
	use super::*;
	use crate::{Error, MaxPendingWhitelistUpdates, MaxWithdrawalTimelock, WhitelistChange};
	use cf_chains::{
		address::{AddressConverter, EncodedAddress},
		AccountOrAddress,
	};
	use cf_traits::{
		mocks::{address_converter::MockAddressConverter, time_source},
		RefundAddressRegistry, WithdrawalAddressRestriction,
	};
	use core::time::Duration;
	use frame_support::{
		assert_err,
		pallet_prelude::{DispatchResult, Weight},
		traits::Hooks,
	};

	fn account(seed: u8) -> AccountId {
		AccountId::from([seed; 32])
	}

	/// Runs `on_idle` so that due pending changes are applied.
	fn apply_pending() {
		let _ = Pallet::<Test>::on_idle(System::block_number(), Weight::MAX);
	}

	fn advance_clock(seconds: u64) {
		time_source::Mock::advance_clock(Duration::from_secs(seconds));
	}

	fn encoded(address: ForeignChainAddress) -> EncodedAddress {
		MockAddressConverter::to_encoded_address(address)
	}

	fn allow(who: &AccountId, address: ForeignChainAddress) {
		assert_ok!(Pallet::<Test>::update_whitelist(
			RuntimeOrigin::signed(who.clone()),
			WhitelistChange::Allow(AccountOrAddress::ExternalAddress(encoded(address))),
		));
	}

	fn allow_account(who: &AccountId, account: &AccountId) {
		assert_ok!(Pallet::<Test>::update_whitelist(
			RuntimeOrigin::signed(who.clone()),
			WhitelistChange::Allow(AccountOrAddress::InternalAccount(account.clone())),
		));
	}

	fn remove(who: &AccountId, address: ForeignChainAddress) {
		assert_ok!(Pallet::<Test>::update_whitelist(
			RuntimeOrigin::signed(who.clone()),
			WhitelistChange::Remove(AccountOrAddress::ExternalAddress(encoded(address))),
		));
	}

	fn set_timelock(who: &AccountId, seconds: u64) {
		assert_ok!(Pallet::<Test>::set_withdrawal_timelock(
			RuntimeOrigin::signed(who.clone()),
			seconds
		));
	}

	fn ensure_allowed_external(who: &AccountId, address: &ForeignChainAddress) -> DispatchResult {
		Pallet::<Test>::ensure_withdrawal_allowed_to(
			who,
			AccountOrAddress::ExternalAddress(address),
		)
	}

	fn ensure_allowed_internal(who: &AccountId, account: &AccountId) -> DispatchResult {
		Pallet::<Test>::ensure_withdrawal_allowed_to(
			who,
			AccountOrAddress::InternalAccount(account),
		)
	}

	#[test]
	fn allowlist_enforced_without_timelock_until_emptied() {
		new_test_ext().execute_with(|| {
			let who = account(1);
			// Nothing configured => unrestricted.
			assert_ok!(ensure_allowed_external(&who, &ETH_ADDR_2));

			// Adding an entry (applied immediately, since no timelock is set) turns enforcement
			// on — mistake protection without any timelock.
			allow(&who, ETH_ADDR_1);
			assert_ok!(ensure_allowed_external(&who, &ETH_ADDR_1));
			assert_err!(
				ensure_allowed_external(&who, &ETH_ADDR_2),
				Error::<Test>::DestinationNotAllowed
			);

			// Removing the last entry leaves nothing configured => unrestricted again.
			remove(&who, ETH_ADDR_1);
			assert_ok!(ensure_allowed_external(&who, &ETH_ADDR_2));
		});
	}

	#[test]
	fn update_whitelist_stores_and_emits_scheduled_event() {
		new_test_ext().execute_with(|| {
			let who = account(1);
			// The timelock is off, so the change is scheduled for the current time.
			allow(&who, ETH_ADDR_1);
			System::assert_has_event(RuntimeEvent::AssetBalances(
				Event::WithdrawalAllowlistUpdateScheduled {
					account_id: who,
					change: WhitelistChange::Allow(AccountOrAddress::ExternalAddress(ETH_ADDR_1)),
					apply_at: 0,
				},
			));
		});
	}

	#[test]
	fn update_whitelist_rejects_undecodable_address() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				Pallet::<Test>::update_whitelist(
					RuntimeOrigin::signed(account(1)),
					WhitelistChange::Allow(AccountOrAddress::ExternalAddress(EncodedAddress::Btc(
						vec![]
					))),
				),
				Error::<Test>::InvalidEncodedAddress
			);
		});
	}

	#[test]
	fn update_whitelist_enforces_pending_cap() {
		new_test_ext().execute_with(|| {
			let who = account(1);
			assert_ok!(Pallet::<Test>::update_pallet_config(
				RuntimeOrigin::root(),
				PalletConfigUpdate::MaxPendingWhitelistUpdates { count: 2 },
			));
			// The cap applies to *pending* changes, so the restriction must be on for changes to
			// queue up.
			set_timelock(&who, 1000);
			allow(&who, ETH_ADDR_1);
			allow(&who, ETH_ADDR_2);
			assert_noop!(
				Pallet::<Test>::update_whitelist(
					RuntimeOrigin::signed(who),
					WhitelistChange::Allow(AccountOrAddress::ExternalAddress(encoded(ARB_ADDR_1))),
				),
				Error::<Test>::TooManyPendingUpdates
			);
		});
	}

	#[test]
	fn set_withdrawal_timelock_enforces_maximum_and_emits() {
		new_test_ext().execute_with(|| {
			let who = account(1);
			let max = MaxWithdrawalTimelock::<Test>::get();
			assert_noop!(
				Pallet::<Test>::set_withdrawal_timelock(
					RuntimeOrigin::signed(who.clone()),
					max + 1
				),
				Error::<Test>::TimelockExceedsMaximum
			);
			// Enabling (0 -> 1000) takes effect immediately.
			set_timelock(&who, 1000);
			System::assert_has_event(RuntimeEvent::AssetBalances(
				Event::WithdrawalTimelockUpdated {
					account_id: who,
					duration: 1000,
					effective_at: 0,
				},
			));
		});
	}

	#[test]
	fn governance_can_update_limits() {
		new_test_ext().execute_with(|| {
			assert_ok!(Pallet::<Test>::update_pallet_config(
				RuntimeOrigin::root(),
				PalletConfigUpdate::MaxWithdrawalTimelock { seconds: 123 },
			));
			assert_eq!(MaxWithdrawalTimelock::<Test>::get(), 123);

			assert_ok!(Pallet::<Test>::update_pallet_config(
				RuntimeOrigin::root(),
				PalletConfigUpdate::MaxPendingWhitelistUpdates { count: 5 },
			));
			assert_eq!(MaxPendingWhitelistUpdates::<Test>::get(), 5);
		});
	}

	#[test]
	fn restriction_activates_after_timelock_elapses() {
		new_test_ext().execute_with(|| {
			let who = account(1);
			let dest = account(2);
			// Off by default => everything allowed.
			assert_ok!(ensure_allowed_external(&who, &ETH_ADDR_1));
			assert_ok!(ensure_allowed_internal(&who, &dest));

			// Enabling turns the restriction on immediately; with nothing configured, all blocked.
			set_timelock(&who, 1000);
			assert_err!(
				ensure_allowed_external(&who, &ETH_ADDR_1),
				Error::<Test>::DestinationNotAllowed
			);
			assert_err!(ensure_allowed_internal(&who, &dest), Error::<Test>::DestinationNotAllowed);

			// A scheduled change only takes effect once the timelock has elapsed.
			allow(&who, ETH_ADDR_1);
			allow_account(&who, &dest);
			assert_err!(
				ensure_allowed_external(&who, &ETH_ADDR_1),
				Error::<Test>::DestinationNotAllowed
			);

			advance_clock(1001);

			apply_pending();
			assert_ok!(ensure_allowed_external(&who, &ETH_ADDR_1));
			assert_ok!(ensure_allowed_internal(&who, &dest));
			// An unconfigured chain stays blocked (account-wide fail-safe).
			assert_err!(
				ensure_allowed_external(&who, &ARB_ADDR_1),
				Error::<Test>::DestinationNotAllowed
			);
		});
	}

	#[test]
	fn registered_refund_address_is_implicitly_allowed() {
		new_test_ext().execute_with(|| {
			let who = account(1);
			// An LP registers a refund address first.
			Pallet::<Test>::register_liquidity_refund_address(&who, ETH_ADDR_1);

			// With no timelock set the restriction is off, so a refund address changes nothing:
			// any address is still allowed, exactly as before the feature.
			assert_ok!(ensure_allowed_external(&who, &ETH_ADDR_2));

			// Turn the restriction on; nothing has been added to the whitelist.
			set_timelock(&who, 1000);

			// The refund address is allowed even though it was never whitelisted (and the timelock
			// is on) — refund addresses are trusted.
			assert_ok!(ensure_allowed_external(&who, &ETH_ADDR_1));

			// A different address on the same chain is still blocked.
			assert_err!(
				ensure_allowed_external(&who, &ETH_ADDR_2),
				Error::<Test>::DestinationNotAllowed
			);
		});
	}

	#[test]
	fn refund_address_update_is_timelocked() {
		new_test_ext().execute_with(|| {
			let who = account(1);
			// Establish a refund address, then turn the restriction on.
			Pallet::<Test>::register_liquidity_refund_address(&who, ETH_ADDR_1);
			set_timelock(&who, 1000);
			assert_ok!(ensure_allowed_external(&who, &ETH_ADDR_1));

			// Repointing under restriction is delayed: the old refund address stays active and the
			// new one is not yet allowed (this is what closes the stolen-key repoint bypass).
			Pallet::<Test>::register_liquidity_refund_address(&who, ETH_ADDR_2);
			assert_ok!(ensure_allowed_external(&who, &ETH_ADDR_1));
			assert_err!(
				ensure_allowed_external(&who, &ETH_ADDR_2),
				Error::<Test>::DestinationNotAllowed
			);

			// Registering again replaces the pending repoint rather than stacking a second one.
			Pallet::<Test>::register_liquidity_refund_address(&who, ETH_ADDR_3);

			// Once the timelock elapses the latest refund address takes over; the replaced one
			// never becomes active.
			advance_clock(1001);
			apply_pending();
			assert_ok!(ensure_allowed_external(&who, &ETH_ADDR_3));
			assert_err!(
				ensure_allowed_external(&who, &ETH_ADDR_1),
				Error::<Test>::DestinationNotAllowed
			);
			assert_err!(
				ensure_allowed_external(&who, &ETH_ADDR_2),
				Error::<Test>::DestinationNotAllowed
			);
		});
	}

	#[test]
	fn scheduled_refund_address_permits_chain_interaction() {
		new_test_ext().execute_with(|| {
			use cf_primitives::Asset;
			let who = account(1);

			// Restriction on, no refund address yet => chain interaction is gated.
			set_timelock(&who, 1000);
			assert_err!(
				Pallet::<Test>::ensure_has_refund_address_for_asset(&who, Asset::Eth),
				Error::<Test>::NoLiquidityRefundAddressRegistered
			);

			// Registering under restriction only *schedules* the address — it is not yet
			// effective...
			Pallet::<Test>::register_liquidity_refund_address(&who, ETH_ADDR_1);
			assert!(Pallet::<Test>::get_refund_address(&who, ForeignChain::Ethereum).is_none());
			// ...but a scheduled refund address is enough to interact with the chain.
			assert_ok!(Pallet::<Test>::ensure_has_refund_address_for_asset(&who, Asset::Eth));

			// It can't be perpetually deferred: it becomes effective within the timelock.
			advance_clock(1001);
			apply_pending();
			assert_eq!(
				Pallet::<Test>::get_refund_address(&who, ForeignChain::Ethereum),
				Some(ETH_ADDR_1)
			);
		});
	}

	#[test]
	fn timelock_updates_are_delayed_and_replace_pending_ones() {
		new_test_ext().execute_with(|| {
			let who = account(1);
			// ETH_ADDR_1 is never whitelisted here; it merely probes whether the restriction is
			// on (nothing is allowed) or off (everything is allowed).
			set_timelock(&who, 1000);

			// A timelock update is itself delayed by the current timelock, so the restriction
			// stays on (a stolen key can't instantly disable it)...
			set_timelock(&who, 0);
			assert_err!(
				ensure_allowed_external(&who, &ETH_ADDR_1),
				Error::<Test>::DestinationNotAllowed
			);

			// ...and scheduling another timelock update replaces the pending one, which therefore
			// never takes effect — this is how the owner cancels a malicious pending update.
			set_timelock(&who, 1000);
			advance_clock(1001);
			apply_pending();
			assert_err!(
				ensure_allowed_external(&who, &ETH_ADDR_1),
				Error::<Test>::DestinationNotAllowed
			);

			// An unreplaced update takes effect once the timelock elapses.
			set_timelock(&who, 0);
			advance_clock(1001);
			apply_pending();
			assert_ok!(ensure_allowed_external(&who, &ETH_ADDR_1));
		});
	}

	#[test]
	fn on_idle_carries_over_on_weight_exhaustion() {
		new_test_ext().execute_with(|| {
			let who = account(1);
			set_timelock(&who, 1000);
			allow(&who, ETH_ADDR_1);
			allow(&who, ETH_ADDR_2);
			allow(&who, ETH_ADDR_3);
			allow(&who, ARB_ADDR_1);
			advance_clock(1001);

			// Not enough weight to apply anything: all changes stay pending.
			let _ = Pallet::<Test>::on_idle(
				System::block_number(),
				<() as crate::WeightInfo>::on_idle_check(),
			);
			assert_err!(
				ensure_allowed_external(&who, &ETH_ADDR_1),
				Error::<Test>::DestinationNotAllowed
			);

			// Weight for exactly two changes: they apply in submission order and the rest are
			// carried over.
			let _ = Pallet::<Test>::on_idle(
				System::block_number(),
				<() as crate::WeightInfo>::on_idle_apply_change(2),
			);
			assert_ok!(ensure_allowed_external(&who, &ETH_ADDR_1));
			assert_ok!(ensure_allowed_external(&who, &ETH_ADDR_2));
			assert_err!(
				ensure_allowed_external(&who, &ETH_ADDR_3),
				Error::<Test>::DestinationNotAllowed
			);
			assert_err!(
				ensure_allowed_external(&who, &ARB_ADDR_1),
				Error::<Test>::DestinationNotAllowed
			);

			// The next (unconstrained) run picks up the remainder.
			apply_pending();
			assert_ok!(ensure_allowed_external(&who, &ETH_ADDR_3));
			assert_ok!(ensure_allowed_external(&who, &ARB_ADDR_1));
		});
	}

	#[test]
	fn direct_registration_discards_stale_pending_repoint() {
		new_test_ext().execute_with(|| {
			let who = account(1);
			// A repoint is scheduled under restriction, and outlives the timelock that created it:
			// the weakening (scheduled before the repoint) matures first.
			Pallet::<Test>::register_liquidity_refund_address(&who, ETH_ADDR_1);
			set_timelock(&who, 1000);
			set_timelock(&who, 0); // scheduled, applies at t+1000
			advance_clock(500);
			Pallet::<Test>::register_liquidity_refund_address(&who, ETH_ADDR_2); // applies at t+1500
			advance_clock(600);
			apply_pending(); // t=1100: timelock now off; repoint to ETH_ADDR_2 still pending

			// Registering directly must discard the stale pending repoint...
			Pallet::<Test>::register_liquidity_refund_address(&who, ETH_ADDR_3);
			assert_eq!(
				Pallet::<Test>::get_refund_address(&who, ForeignChain::Ethereum),
				Some(ETH_ADDR_3)
			);

			// ...otherwise it would fire later and overwrite the newer address.
			advance_clock(1000);
			apply_pending();
			assert_eq!(
				Pallet::<Test>::get_refund_address(&who, ForeignChain::Ethereum),
				Some(ETH_ADDR_3)
			);
		});
	}
}
