use cf_chains::{ForeignChain, ForeignChainAddress};
use cf_primitives::AssetAmount;
use cf_traits::SetSafeMode;

use cf_chains::AnyChain;
use cf_traits::{mocks::egress_handler::MockEgressHandler, SafeMode};

use crate::{mock::*, RecordedFees, WithheldTransactionFees};

fn payed_gas(chain: ForeignChain, amount: AssetAmount, account: ForeignChainAddress) {
	Refunding::record_gas_fee(account, chain, amount);
	Refunding::withhold_transaction_fee(chain, amount);
}

#[test]
fn refund_validators_evm() {
	new_test_ext().execute_with(|| {
		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_1.clone());
		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_2.clone());
		payed_gas(ForeignChain::Arbitrum, 100, ARB_ADDR_1.clone());

		let recorded_fees_eth = RecordedFees::<Test>::get(ForeignChain::Ethereum).unwrap();
		let recorded_fees_arb = RecordedFees::<Test>::get(ForeignChain::Arbitrum).unwrap();

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1), Some(&100));
		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_2), Some(&100));
		assert_eq!(recorded_fees_arb.get(&ARB_ADDR_1), Some(&100));

		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Ethereum), 200);
		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Arbitrum), 100);

		Refunding::on_distribute_withheld_fees(1);

		let egresses = MockEgressHandler::<AnyChain>::get_scheduled_egresses();

		let recorded_fees_eth = RecordedFees::<Test>::get(ForeignChain::Ethereum);
		let recorded_fees_arb = RecordedFees::<Test>::get(ForeignChain::Arbitrum);

		assert_eq!(egresses.len(), 3);

		for egress in egresses {
			assert_eq!(egress.amount(), 100);
		}

		assert_eq!(recorded_fees_eth, None);
		assert_eq!(recorded_fees_arb, None);

		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Ethereum), 0);
		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Arbitrum), 0);
	});
}

#[test]
fn skip_refunding_if_safe_mode_is_disabled() {
	new_test_ext().execute_with(|| {
		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_1.clone());

		let recorded_fees_eth = RecordedFees::<Test>::get(ForeignChain::Ethereum).unwrap();

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

		let recorded_fees_eth = RecordedFees::<Test>::get(ForeignChain::Ethereum).unwrap();

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

		let recorded_fees_btc = RecordedFees::<Test>::get(ForeignChain::Bitcoin).unwrap();

		assert_eq!(recorded_fees_btc.get(&BTC_ADDR_1), Some(&100));

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

		System::assert_last_event(RuntimeEvent::Refunding(
			crate::Event::RefundedMoreThanWithheld {
				chain: ForeignChain::Bitcoin,
				withhold: 99,
				refunded: 100,
			},
		));

		let recorded_fees_btc = RecordedFees::<Test>::get(ForeignChain::Bitcoin);

		assert_eq!(recorded_fees_btc, None);

		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Bitcoin), 99);
	});
}

#[test]
pub fn refund_validators_polkadot() {
	new_test_ext().execute_with(|| {
		payed_gas(ForeignChain::Polkadot, 100, DOT_ADDR_1.clone());

		let recorded_fees_dot = RecordedFees::<Test>::get(ForeignChain::Polkadot).unwrap();

		assert_eq!(recorded_fees_dot.get(&DOT_ADDR_1), Some(&100));

		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Polkadot), 100);

		Refunding::on_distribute_withheld_fees(1);

		let recorded_fees_dot = RecordedFees::<Test>::get(ForeignChain::Polkadot);

		assert_eq!(recorded_fees_dot, None);

		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Polkadot), 0);
	});
}
