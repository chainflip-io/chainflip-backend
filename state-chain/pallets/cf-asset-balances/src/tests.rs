use cf_chains::{Ethereum, ForeignChain, ForeignChainAddress};
use cf_primitives::{AccountId, AssetAmount};
use cf_traits::{
	mocks::egress_handler::MockEgressParameter, AssetWithholding, LiabilityTracker, SetSafeMode,
};

use crate::FreeBalances;
use cf_chains::AnyChain;
use cf_primitives::{accounting::AssetBalance, Asset};
use cf_test_utilities::assert_has_event;
use cf_traits::{mocks::egress_handler::MockEgressHandler, BalanceApi, SafeMode};
use frame_support::{assert_noop, assert_ok, traits::OnKilledAccount};

use crate::{mock::*, ExternalOwner, Liabilities, Pallet, WithheldAssets};

fn payed_gas(chain: ForeignChain, amount: AssetAmount, account: ForeignChainAddress) {
	Pallet::<Test>::record_liability(account, AssetBalance::mint(amount, chain.gas_asset()));
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

		let recorded_fees_eth =
			Liabilities::<Test>::get(ForeignChain::Ethereum.gas_asset()).unwrap();

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1.into()).unwrap().amount(), 100);
		assert_eq!(
			WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset())
				.unwrap()
				.amount(),
			100
		);

		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			refunding: crate::PalletSafeMode::CODE_RED,
		});

		Pallet::<Test>::trigger_reconciliation();

		assert_egress(0, None);

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1.into()).unwrap().amount(), 100);
		assert_eq!(
			WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset())
				.unwrap()
				.amount(),
			100
		);
	});
}

#[test]
pub fn keep_fees_in_storage_if_egress_fails() {
	new_test_ext().execute_with(|| {
		MockEgressHandler::<AnyChain>::return_failure(true);

		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_1.clone());

		let recorded_fees_eth =
			Liabilities::<Test>::get(ForeignChain::Ethereum.gas_asset()).unwrap();

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1.into()).unwrap().amount(), 100);
		assert_eq!(
			WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset())
				.unwrap()
				.amount(),
			100
		);

		Pallet::<Test>::trigger_reconciliation();

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1.into()).unwrap().amount(), 100);
		assert_eq!(
			WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset())
				.unwrap()
				.amount(),
			100
		);
	});
}

#[test]
pub fn not_enough_withheld_fees() {
	new_test_ext().execute_with(|| {
		const BTC_OWED: AssetAmount = 100;
		const BTC_AVAILABLE: AssetAmount = 99;
		const ETH_OWED: AssetAmount = 200;
		const ETH_AVAILABLE: AssetAmount = 199;

		payed_gas(ForeignChain::Bitcoin, BTC_OWED, BTC_ADDR_1.clone());
		WithheldAssets::<Test>::insert(
			ForeignChain::Bitcoin.gas_asset(),
			AssetBalance::mint(BTC_AVAILABLE, Asset::Btc),
		);

		payed_gas(ForeignChain::Ethereum, ETH_OWED, ETH_ADDR_1.clone());
		WithheldAssets::<Test>::insert(
			ForeignChain::Ethereum.gas_asset(),
			AssetBalance::mint(ETH_AVAILABLE, Asset::Eth),
		);

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
			Liabilities::<Test>::get(ForeignChain::Bitcoin.gas_asset()).unwrap()
				[&ExternalOwner::Vault]
				.amount(),
			BTC_OWED - BTC_AVAILABLE
		);
		assert_eq!(
			WithheldAssets::<Test>::get(ForeignChain::Bitcoin.gas_asset()).unwrap().amount(),
			0
		);

		// For Ethereum, either refund the entirety or do nothing.
		let recorded_fees_eth =
			Liabilities::<Test>::get(ForeignChain::Ethereum.gas_asset()).unwrap();
		assert_eq!(recorded_fees_eth[&ExternalOwner::Account(ETH_ADDR_1)].amount(), ETH_OWED);
		assert_eq!(
			WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset())
				.unwrap()
				.amount(),
			ETH_AVAILABLE
		);
	});
}

#[test]
pub fn do_not_refund_if_amount_is_too_low() {
	new_test_ext().execute_with(|| {
		const REFUND_AMOUNT: u128 = 10;
		payed_gas(ForeignChain::Ethereum, REFUND_AMOUNT, ETH_ADDR_1.clone());

		MockEgressHandler::<Ethereum>::set_fee(REFUND_AMOUNT * 2);
		assert_eq!(
			WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset())
				.unwrap()
				.amount(),
			REFUND_AMOUNT
		);

		Pallet::<Test>::trigger_reconciliation();

		assert_has_event::<Test>(
			crate::Event::RefundSkipped {
				reason: crate::Error::<Test>::RefundAmountTooLow.into(),
				chain: ForeignChain::Ethereum,
				address: ETH_ADDR_1,
			}
			.into(),
		);

		assert_eq!(
			WithheldAssets::<Test>::get(ForeignChain::Ethereum.gas_asset())
				.unwrap()
				.amount(),
			REFUND_AMOUNT
		);
		assert_eq!(
			Liabilities::<Test>::get(ForeignChain::Ethereum.gas_asset())
				.unwrap()
				.get(&ExternalOwner::Account(ETH_ADDR_1))
				.unwrap()
				.amount(),
			REFUND_AMOUNT
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

		assert_eq!(WithheldAssets::<Test>::get(asset).unwrap().amount(), total_gas_paid);

		liabilities.into_iter().for_each(|liability| {
			assert_eq!(
				Liabilities::<Test>::get(asset).unwrap().get(&liability).unwrap().amount(),
				REFUND_AMOUNT
			)
		});

		Pallet::<Test>::trigger_reconciliation();

		// Check all fees owed have been paid out.
		assert!(Liabilities::<Test>::get(asset).is_none());
		assert_eq!(WithheldAssets::<Test>::get(asset).unwrap().amount(), 0);

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
			assert_eq!(
				WithheldAssets::<Test>::get(chain.gas_asset()).unwrap().amount(),
				REFUND_AMOUNT
			);
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
			assert!(Liabilities::<Test>::get(asset).is_none());
			assert_eq!(WithheldAssets::<Test>::get(asset).unwrap().amount(), 0);
		});
	});
}

pub mod balance_api {
	use crate::DeleteAccount;

	use super::*;

	use cf_primitives::chains::assets::eth;

	#[test]
	pub fn credit_and_debit() {
		new_test_ext().execute_with(|| {
			let alice = AccountId::from([1; 32]);
			const AMOUNT: u128 = 100;
			const DELTA: u128 = 10;
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
					new_balance: AMOUNT,
				}
				.into(),
			);
			assert_eq!(
				FreeBalances::<Test>::get(&alice, ForeignChain::Ethereum.gas_asset())
					.unwrap()
					.amount(),
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
				)
				.unwrap()
				.amount(),
				DELTA
			);
			assert_has_event::<Test>(
				crate::Event::AccountDebited {
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
				AssetBalance::mint(100, Asset::Eth),
			);
			DeleteAccount::<Test>::on_killed_account(&AccountId::from([1; 32]));
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
				AssetBalance::mint(100, Asset::Eth),
			);
			assert_eq!(
				Pallet::<Test>::free_balances(&AccountId::from([1; 32])).eth,
				eth::AssetMap { eth: 100, flip: 0, usdc: 0, usdt: 0 }
			);
		});
	}
}
