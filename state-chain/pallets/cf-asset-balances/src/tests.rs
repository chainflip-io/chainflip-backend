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
fn refund_validators_evm() {
	new_test_ext().execute_with(|| {
		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_1.clone());
		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_2.clone());
		payed_gas(ForeignChain::Arbitrum, 100, ARB_ADDR_1.clone());

		let recorded_fees_eth = Liabilities::<Test>::get(ForeignChain::Ethereum.gas_asset());

		let recorded_fees_arb = Liabilities::<Test>::get(ForeignChain::Arbitrum.gas_asset());

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1.into()), Some(&100));
		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_2.into()), Some(&100));
		assert_eq!(recorded_fees_arb.get(&ARB_ADDR_1.into()), Some(&100));

		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset()), 200);
		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Arbitrum.gas_asset()), 100);

		Pallet::<Test>::trigger_reconciliation();

		let recorded_fees_eth = Liabilities::<Test>::get(ForeignChain::Ethereum.gas_asset());
		let recorded_fees_arb = Liabilities::<Test>::get(ForeignChain::Arbitrum.gas_asset());

		assert_egress(
			3,
			Some(|egresses: Vec<MockEgressParameter<AnyChain>>| {
				for egress in egresses {
					assert_eq!(egress.amount(), 100);
				}
			}),
		);

		assert!(recorded_fees_eth.is_empty());
		assert!(recorded_fees_arb.is_empty());

		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset()), 0);
		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Arbitrum.gas_asset()), 0);
	});
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
pub fn refund_validators_btc() {
	new_test_ext().execute_with(|| {
		payed_gas(ForeignChain::Bitcoin, 100, BTC_ADDR_1.clone());

		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Bitcoin.gas_asset()), 100);

		Pallet::<Test>::trigger_reconciliation();

		let recorded_fees_btc = Liabilities::<Test>::get(ForeignChain::Bitcoin.gas_asset());

		assert!(recorded_fees_btc.is_empty());

		assert_egress(0, None);

		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Bitcoin.gas_asset()), 0);
	});
}

#[test]
pub fn not_enough_withheld_fees() {
	new_test_ext().execute_with(|| {
		payed_gas(ForeignChain::Bitcoin, 100, BTC_ADDR_1.clone());

		WithheldAssets::<Test>::insert(ForeignChain::Bitcoin.gas_asset(), 99);

		Pallet::<Test>::trigger_reconciliation();

		System::assert_last_event(RuntimeEvent::AssetBalances(
			crate::Event::VaultDeficitDetected {
				chain: ForeignChain::Bitcoin,
				amount_owed: 100,
				available: 99,
			},
		));

		let recorded_fees_btc = Liabilities::<Test>::get(ForeignChain::Bitcoin.gas_asset());

		assert_eq!(recorded_fees_btc[&ExternalOwner::Vault], 1);
		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Bitcoin.gas_asset()), 0);
	});
}

#[test]
pub fn refund_validators_polkadot() {
	new_test_ext().execute_with(|| {
		payed_gas(ForeignChain::Polkadot, 100, DOT_ADDR_1.clone());

		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Polkadot.gas_asset()), 100);

		Pallet::<Test>::trigger_reconciliation();

		assert_egress(
			1,
			Some(|egresses: Vec<MockEgressParameter<AnyChain>>| {
				for egress in egresses {
					assert_eq!(egress.amount(), 100);
				}
			}),
		);

		let recorded_fees_dot = Liabilities::<Test>::get(ForeignChain::Polkadot.gas_asset());

		assert!(recorded_fees_dot.is_empty());

		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Polkadot.gas_asset()), 0);
	});
}

#[test]
pub fn max_refunds_per_epoch() {
	new_test_ext().execute_with(|| {
		for i in 0..crate::MAX_REFUNDED_VALIDATORS_ETH_PER_EPOCH + 2 {
			payed_gas(
				ForeignChain::Ethereum,
				100,
				ForeignChainAddress::Eth(sp_core::H160([i as u8; 20])),
			);
		}
		assert_eq!(
			WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset()),
			(100 * (crate::MAX_REFUNDED_VALIDATORS_ETH_PER_EPOCH as u128 + 2))
		);
		Pallet::<Test>::trigger_reconciliation();
		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset()), 200);
		assert!(WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset()) > 0);
		assert_eq!(Liabilities::<Test>::get(ForeignChain::Ethereum.gas_asset()).len(), 2);
		Pallet::<Test>::trigger_reconciliation();
		assert_eq!(WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset()), 0);
		assert_eq!(Liabilities::<Test>::get(ForeignChain::Ethereum.gas_asset()).len(), 0);
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

pub mod balance_api {
	use super::*;

	use crate::{CollectedNetworkFee, CollectedRejectedFunds, HistoricalEarnedFees};
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
	pub fn kill_balances() {
		new_test_ext().execute_with(|| {
			FreeBalances::<Test>::insert(
				AccountId::from([1; 32]),
				ForeignChain::Ethereum.gas_asset(),
				100,
			);
			HistoricalEarnedFees::<Test>::insert(
				AccountId::from([1; 32]),
				ForeignChain::Ethereum.gas_asset(),
				100,
			);
			Pallet::<Test>::kill_balance(&AccountId::from([1; 32]));
			assert!(FreeBalances::<Test>::get(
				AccountId::from([1; 32]),
				ForeignChain::Ethereum.gas_asset()
			)
			.is_none());
			assert_eq!(
				HistoricalEarnedFees::<Test>::get(
					AccountId::from([1; 32]),
					ForeignChain::Ethereum.gas_asset()
				),
				0
			);
		});
	}

	#[test]
	pub fn record_fees() {
		new_test_ext().execute_with(|| {
			Pallet::<Test>::record_fees(
				&AccountId::from([1; 32]),
				100,
				ForeignChain::Ethereum.gas_asset(),
			);
			assert_eq!(
				HistoricalEarnedFees::<Test>::get(
					AccountId::from([1; 32]),
					ForeignChain::Ethereum.gas_asset()
				),
				100
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
				Pallet::<Test>::free_balances(&AccountId::from([1; 32])).unwrap().eth,
				eth::AssetMap { eth: 100, flip: 0, usdc: 0, usdt: 0 }
			);
		});
	}

	#[test]
	pub fn record_network_fee() {
		new_test_ext().execute_with(|| {
			Pallet::<Test>::record_network_fee(100);
			assert_eq!(CollectedNetworkFee::<Test>::get(), 100);
		});
	}

	#[test]
	pub fn collected_rejected_funds() {
		new_test_ext().execute_with(|| {
			Pallet::<Test>::collected_rejected_funds(ForeignChain::Ethereum.gas_asset(), 100);
			assert_eq!(
				CollectedRejectedFunds::<Test>::get(ForeignChain::Ethereum.gas_asset()),
				100
			);
		});
	}
}
