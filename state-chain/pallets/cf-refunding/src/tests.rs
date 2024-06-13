use cf_chains::{evm::Address, ForeignChainAddress};
use cf_primitives::{Asset, AssetAmount};
use cf_test_utilities::assert_event_sequence;
use sp_core::H160;

use cf_chains::AnyChain;
use cf_traits::mocks::egress_handler::{MockEgressHandler, MockEgressParameter};

use crate::{mock::*, Event, RecordedFees, WithheldTransactionFees};

fn payed_gas(asset: Asset, amount: AssetAmount, account: ForeignChainAddress) {
	Refunding::record_gas_fee(account, asset, amount);
	Refunding::withheld_transaction_fee(asset, amount);
}

#[test]
fn refund_validators_on_epoch_transition() {
	new_test_ext().execute_with(|| {
		payed_gas(Asset::Eth, 100, ETH_ADDR_1.clone());
		payed_gas(Asset::Eth, 100, ETH_ADDR_2.clone());
		payed_gas(Asset::Eth, 100, ETH_ADDR_3.clone());
		payed_gas(Asset::Dot, 100, DOT_ADDR_1.clone());

		let recorded_fees_eth = RecordedFees::<Test>::get(Asset::Eth);
		let recorded_fees_dot = RecordedFees::<Test>::get(Asset::Dot);

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1), Some(&100));
		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_2), Some(&100));
		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_3), Some(&100));
		assert_eq!(recorded_fees_dot.get(&DOT_ADDR_1), Some(&100));

		assert_eq!(WithheldTransactionFees::<Test>::get(Asset::Eth), 300);
		assert_eq!(WithheldTransactionFees::<Test>::get(Asset::Dot), 100);

		Refunding::on_distribute_withheld_fees(1);

		let egresses = MockEgressHandler::<AnyChain>::get_scheduled_egresses();

		let recorded_fees_eth = RecordedFees::<Test>::get(Asset::Eth);
		let recorded_fees_dot = RecordedFees::<Test>::get(Asset::Eth);

		assert_eq!(egresses.len(), 4);

		for egress in egresses {
			assert_eq!(egress.amount(), 100);
		}

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1), None);
		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_2), None);
		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_3), None);
		assert_eq!(recorded_fees_dot.get(&DOT_ADDR_1), None);

		assert_eq!(WithheldTransactionFees::<Test>::get(Asset::Eth), 0);
		assert_eq!(WithheldTransactionFees::<Test>::get(Asset::Dot), 0);
	});
}

#[test]
fn skips_refund_if_integrity_checks_fails() {
	new_test_ext().execute_with(|| {
		payed_gas(Asset::Eth, 100, ETH_ADDR_1.clone());
		payed_gas(Asset::Eth, 100, ETH_ADDR_2.clone());
		payed_gas(Asset::Eth, 100, ETH_ADDR_3.clone());

		WithheldTransactionFees::<Test>::insert(Asset::Eth, 299);

		Refunding::on_distribute_withheld_fees(1);

		assert_eq!(WithheldTransactionFees::<Test>::get(Asset::Eth), 299);

		let recorded_fees_eth = RecordedFees::<Test>::get(Asset::Eth);

		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_1), Some(&100));
		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_2), Some(&100));
		assert_eq!(recorded_fees_eth.get(&ETH_ADDR_3), Some(&100));

		assert_event_sequence!(
			Test,
			RuntimeEvent::Refunding(Event::RefundIntegrityCheckFailed {
				epoch: 1,
				asset: Asset::Eth
			})
		);
	});
}
