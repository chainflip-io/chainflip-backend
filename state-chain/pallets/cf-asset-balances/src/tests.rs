use cf_chains::{Ethereum, ForeignChain, ForeignChainAddress};
use cf_primitives::{AccountId, AssetAmount};
use cf_traits::{
	mocks::egress_handler::MockEgressParameter, AssetWithholding, LiabilityTracker, SetSafeMode,
};

use crate::FreeBalances;
use cf_chains::AnyChain;
use cf_test_utilities::assert_has_event;
use cf_traits::{mocks::egress_handler::MockEgressHandler, BalanceApi, SafeMode};
use frame_support::{assert_noop, assert_ok};

use crate::{mock::*, ExternalOwner, Liabilities, Pallet, WithheldAssets};

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
			refunding: crate::PalletSafeMode::CODE_RED,
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

		System::assert_has_event(RuntimeEvent::AssetBalances(crate::Event::VaultDeficitDetected {
			chain: ForeignChain::Bitcoin,
			amount_owed: BTC_OWED,
			available: BTC_AVAILABLE,
		}));
		System::assert_has_event(RuntimeEvent::AssetBalances(crate::Event::VaultDeficitDetected {
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
pub fn max_refunds_per_epoch() {
	new_test_ext().execute_with(|| {
		const SMALL_FEE: AssetAmount = 30;
		let asset = ForeignChain::Ethereum.gas_asset();
		for i in 0..crate::MAX_REFUNDED_VALIDATORS_ETH_PER_EPOCH {
			payed_gas(
				ForeignChain::Ethereum,
				100,
				ForeignChainAddress::Eth(sp_core::H160([i as u8; 20])),
			);
		}
		// Add 2 small fees, which will be payed out last.
		for i in 254u8..=255u8 {
			payed_gas(
				ForeignChain::Ethereum,
				SMALL_FEE,
				ForeignChainAddress::Eth(sp_core::H160([i; 20])),
			);
		}
		assert_eq!(
			WithheldAssets::<Test>::get(asset),
			(100 * (crate::MAX_REFUNDED_VALIDATORS_ETH_PER_EPOCH as u128) + SMALL_FEE * 2)
		);
		Pallet::<Test>::trigger_reconciliation();

		// Fees are paid out in reverse order (largest -> smallest). The 2 smallest fees are left
		// out as available funds ran out.
		assert_eq!(Liabilities::<Test>::get(asset).values().sum::<u128>(), SMALL_FEE * 2u128);
		assert_eq!(WithheldAssets::<Test>::get(asset), SMALL_FEE * 2u128);
		assert!(WithheldAssets::<Test>::get(asset) > 0);
		assert_eq!(Liabilities::<Test>::get(asset).len(), 2);

		Pallet::<Test>::trigger_reconciliation();
		assert_eq!(WithheldAssets::<Test>::get(asset), 0);
		assert_eq!(Liabilities::<Test>::get(asset).len(), 0);
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
			crate::Event::RefundSkipped {
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
	use super::*;

	use cf_primitives::chains::assets::eth;

	#[test]
	pub fn credit_and_debit() {
		new_test_ext().execute_with(|| {
			let alice = AccountId::from([1; 32]);
			const AMOUNT: u128 = 100;
			assert_ok!(Pallet::<Test>::try_credit_account(
				&alice,
				ForeignChain::Ethereum.gas_asset(),
				AMOUNT
			));
			assert_has_event::<Test>(
				crate::Event::AccountCredited {
					account_id: alice.clone(),
					asset: ForeignChain::Ethereum.gas_asset(),
					amount_credited: AMOUNT,
				}
				.into(),
			);
			assert_eq!(
				FreeBalances::<Test>::get(&alice, ForeignChain::Ethereum.gas_asset()),
				Some(AMOUNT)
			);
			assert_noop!(
				Pallet::<Test>::try_debit_account(
					&alice,
					ForeignChain::Ethereum.gas_asset(),
					AMOUNT + 10
				),
				crate::Error::<Test>::InsufficientBalance
			);
			assert_ok!(Pallet::<Test>::try_debit_account(
				&AccountId::from([1; 32]),
				ForeignChain::Ethereum.gas_asset(),
				AMOUNT - 10
			));
			assert_eq!(
				FreeBalances::<Test>::get(
					AccountId::from([1; 32]),
					ForeignChain::Ethereum.gas_asset()
				),
				Some(10)
			);
			assert_has_event::<Test>(
				crate::Event::AccountDebited {
					account_id: alice.clone(),
					asset: ForeignChain::Ethereum.gas_asset(),
					amount_debited: AMOUNT - 10,
				}
				.into(),
			);
		});
	}

	#[test]
	pub fn kill_accounts() {
		new_test_ext().execute_with(|| {
			FreeBalances::<Test>::insert(
				AccountId::from([1; 32]),
				ForeignChain::Ethereum.gas_asset(),
				100,
			);
			Pallet::<Test>::kill_account(&AccountId::from([1; 32]));
			assert!(FreeBalances::<Test>::get(
				AccountId::from([1; 32]),
				ForeignChain::Ethereum.gas_asset()
			)
			.is_none());
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
				Pallet::<Test>::free_balances(&AccountId::from([1; 32])).unwrap().eth,
				eth::AssetMap { eth: 100, flip: 0, usdc: 0, usdt: 0 }
			);
		});
	}
}
