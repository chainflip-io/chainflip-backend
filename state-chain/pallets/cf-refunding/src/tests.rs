use cf_chains::{ForeignChain, ForeignChainAddress};
use cf_primitives::AssetAmount;
use cf_test_utilities::assert_event_sequence;
use cf_traits::SetSafeMode;

use cf_chains::AnyChain;
use cf_traits::{mocks::egress_handler::MockEgressHandler, SafeMode};

use crate::{mock::*, Event, RecordedFees, WithheldTransactionFees};

fn payed_gas(chain: ForeignChain, amount: AssetAmount, account: ForeignChainAddress) {
	Refunding::record_gas_fee(account, chain, amount);
	Refunding::withheld_transaction_fee(chain, amount);
}

#[test]
fn refund_validators_on_epoch_transition() {
	new_test_ext().execute_with(|| {
		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_1.clone());
		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_2.clone());
		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_3.clone());
		payed_gas(ForeignChain::Polkadot, 100, DOT_ADDR_1.clone());

		let recorded_fees_eth = RecordedFees::<Test>::get(ForeignChain::Ethereum).unwrap();
		let recorded_fees_dot = RecordedFees::<Test>::get(ForeignChain::Polkadot).unwrap();

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1), Some(&100));
		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_2), Some(&100));
		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_3), Some(&100));
		assert_eq!(recorded_fees_dot.get(&DOT_ADDR_1), Some(&100));

		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Ethereum), 300);
		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Polkadot), 100);

		Refunding::on_distribute_withheld_fees(1);

		let egresses = MockEgressHandler::<AnyChain>::get_scheduled_egresses();

		let recorded_fees_eth = RecordedFees::<Test>::get(ForeignChain::Ethereum);
		let recorded_fees_dot = RecordedFees::<Test>::get(ForeignChain::Ethereum);

		assert_eq!(egresses.len(), 4);

		for egress in egresses {
			assert_eq!(egress.amount(), 100);
		}

		assert_eq!(recorded_fees_eth, None);
		assert_eq!(recorded_fees_eth, None);
		assert_eq!(recorded_fees_eth, None);
		assert_eq!(recorded_fees_dot, None);

		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Ethereum), 0);
		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Polkadot), 0);
	});
}

#[test]
fn skips_refund_if_integrity_checks_fails() {
	new_test_ext().execute_with(|| {
		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_1.clone());
		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_2.clone());
		payed_gas(ForeignChain::Ethereum, 100, ETH_ADDR_3.clone());

		WithheldTransactionFees::<Test>::insert(ForeignChain::Ethereum, 299);

		Refunding::on_distribute_withheld_fees(1);

		assert_eq!(WithheldTransactionFees::<Test>::get(ForeignChain::Ethereum), 299);

		let recorded_fees_eth = RecordedFees::<Test>::get(ForeignChain::Ethereum).unwrap();

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1), Some(&100));
		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_2), Some(&100));
		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_3), Some(&100));

		assert_event_sequence!(
			Test,
			RuntimeEvent::Refunding(Event::RefundIntegrityCheckFailed {
				epoch: 1,
				chain: ForeignChain::Ethereum
			})
		);
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
