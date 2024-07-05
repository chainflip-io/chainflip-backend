use cf_chains::{ForeignChain, ForeignChainAddress};
use cf_primitives::AssetAmount;
use cf_traits::{mocks::egress_handler::MockEgressParameter, SetSafeMode};

use cf_chains::AnyChain;
use cf_traits::{mocks::egress_handler::MockEgressHandler, SafeMode};

use crate::{mock::*, RecordedFees, WithheldTransactionFees};

fn payed_gas(chain: ForeignChain, amount: AssetAmount, account: ForeignChainAddress) {
	Refunding::record_gas_fee(account, chain, amount);
	Refunding::withhold_transaction_fee(chain, amount);
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

		let maybe_recorded_fees_eth = RecordedFees::<Test>::get(ForeignChain::Ethereum).unwrap();
		let recorded_fees_eth = maybe_recorded_fees_eth.get_as_multiple().unwrap();

		let maybe_recorded_fees_arb = RecordedFees::<Test>::get(ForeignChain::Arbitrum).unwrap();
		let recorded_fees_arb = maybe_recorded_fees_arb.get_as_multiple().unwrap();

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1), Some(&100));
		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_2), Some(&100));
		assert_eq!(recorded_fees_arb.get(&ARB_ADDR_1), Some(&100));

		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Ethereum), 200);
		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Arbitrum), 100);

		Refunding::on_distribute_withheld_fees(1);

		let maybe_recorded_fees_eth = RecordedFees::<Test>::get(ForeignChain::Ethereum);
		let maybe_recorded_fees_arb = RecordedFees::<Test>::get(ForeignChain::Arbitrum);

		assert_egress(
			3,
			Some(|egresses: Vec<MockEgressParameter<AnyChain>>| {
				for egress in egresses {
					assert_eq!(egress.amount(), 100);
				}
			}),
		);

		assert_eq!(maybe_recorded_fees_eth, None);
		assert_eq!(maybe_recorded_fees_arb, None);

		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Ethereum), 0);
		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Arbitrum), 0);
	});
}

#[test]
fn skip_refunding_if_safe_mode_is_disabled() {
	new_test_ext().execute_with(|| {
		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_1.clone());

		let maybe_recorded_fees_eth = RecordedFees::<Test>::get(ForeignChain::Ethereum).unwrap();

		let recorded_fees_eth = maybe_recorded_fees_eth.get_as_multiple().unwrap();

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1), Some(&100));
		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Ethereum), 100);

		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			refunding: crate::PalletSafeMode::CODE_RED,
		});

		Refunding::on_distribute_withheld_fees(1);

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1), Some(&100));
		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Ethereum), 100);
	});
}

#[test]
pub fn keep_fees_in_storage_if_egress_fails() {
	new_test_ext().execute_with(|| {
		MockEgressHandler::<AnyChain>::return_failure(true);

		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_1.clone());

		let maybe_recorded_fees_eth = RecordedFees::<Test>::get(ForeignChain::Ethereum).unwrap();
		let recorded_fees_eth = maybe_recorded_fees_eth.get_as_multiple().unwrap();

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1), Some(&100));
		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Ethereum), 100);

		Refunding::on_distribute_withheld_fees(1);

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1), Some(&100));
		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Ethereum), 100);
	});
}

#[test]
pub fn refund_validators_btc() {
	new_test_ext().execute_with(|| {
		payed_gas(ForeignChain::Bitcoin, 100, BTC_ADDR_1.clone());

		assert_eq!(100, 100);

		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Bitcoin), 100);

		Refunding::on_distribute_withheld_fees(1);

		let recorded_fees_btc = RecordedFees::<Test>::get(ForeignChain::Bitcoin);

		assert_eq!(recorded_fees_btc, None);

		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Bitcoin), 0);
	});
}

#[test]
pub fn btc_to_low_withheld_fees() {
	new_test_ext().execute_with(|| {
		payed_gas(ForeignChain::Bitcoin, 100, BTC_ADDR_1.clone());

		WithheldTransactionFees::<Test>::insert(ForeignChain::Bitcoin, 99);

		Refunding::on_distribute_withheld_fees(1);

		System::assert_last_event(RuntimeEvent::Refunding(crate::Event::VaultBleeding {
			chain: ForeignChain::Bitcoin,
			withheld: 99,
			collected: 100,
		}));

		let maybe_recorded_fees_btc = RecordedFees::<Test>::get(ForeignChain::Bitcoin);

		assert_eq!(maybe_recorded_fees_btc, None);

		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Bitcoin), 99);
	});
}

#[test]
pub fn refund_validators_polkadot() {
	new_test_ext().execute_with(|| {
		payed_gas(ForeignChain::Polkadot, 100, DOT_ADDR_1.clone());

		assert_eq!(100, 100);

		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Polkadot), 100);

		Refunding::on_distribute_withheld_fees(1);

		assert_egress(
			1,
			Some(|egresses: Vec<MockEgressParameter<AnyChain>>| {
				for egress in egresses {
					assert_eq!(egress.amount(), 100);
				}
			}),
		);

		let maybe_recorded_fees_dot = RecordedFees::<Test>::get(ForeignChain::Polkadot);

		assert_eq!(maybe_recorded_fees_dot, None);

		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Polkadot), 0);
	});
}
