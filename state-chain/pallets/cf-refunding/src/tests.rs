use cf_chains::{evm::Address, ForeignChainAddress};
use cf_primitives::{Asset, AssetAmount};
use sp_runtime::AccountId32;

use crate::{mock::*, RecordedFees, WithheldTransactionFees};

fn payed_gas(asset: Asset, amount: AssetAmount, account: ForeignChainAddress) {
	Refunding::record_gas_fee(account, asset, amount);
	Refunding::withheld_transaction_fee(asset, amount);
}

// fn to_eth_address(seed: Address) -> ForeignChainAddress {
// 	ForeignChainAddress::Eth(seed)
// }

#[test]
fn refund_validators() {
	new_test_ext().execute_with(|| {
		let address_1 = generate_eth_chain_address([0; 20]);
		let address_2 = generate_eth_chain_address([1; 20]);
		let address_3 = generate_eth_chain_address([2; 20]);

		payed_gas(Asset::Eth, 100, address_1.clone());
		payed_gas(Asset::Eth, 100, address_2.clone());
		payed_gas(Asset::Eth, 100, address_3.clone());

		let recorded_fees = RecordedFees::<Test>::get(Asset::Eth);

		assert_eq!(recorded_fees.get(&address_1), Some(&100));
		assert_eq!(recorded_fees.get(&address_2), Some(&100));
		assert_eq!(recorded_fees.get(&address_3), Some(&100));

		assert_eq!(WithheldTransactionFees::<Test>::get(Asset::Eth), 300);

		Refunding::on_distribute_withheld_fees();

		let recorded_fees = RecordedFees::<Test>::get(Asset::Eth);

		assert_eq!(recorded_fees.get(&address_1), None);
		assert_eq!(recorded_fees.get(&address_2), None);
		assert_eq!(recorded_fees.get(&address_3), None);

		assert_eq!(WithheldTransactionFees::<Test>::get(Asset::Eth), 0);
	});
}

// #[test]
// fn ensure_available_funds_integrity() {
// 	new_test_ext().execute_with(|| {
// 		let address_1 = generate_eth_chain_address([0; 20]);
// 		let address_2 = generate_eth_chain_address([1; 20]);
// 		payed_gas(Asset::Eth, 100, address_1.clone());
// 		payed_gas(Asset::Eth, 99, address_2.clone());
// 		assert_eq!(WithheldTransactionFees::<Test>::get(Asset::Eth), 199);
// 		Refunding::on_distribute_withheld_fees();
// 		let recorded_fees = RecordedFees::<Test>::get(Asset::Eth);
// 		assert_eq!(WithheldTransactionFees::<Test>::get(Asset::Eth), 199);
// 		assert_eq!(recorded_fees.get(&address_1), Some(&100));
// 		assert_eq!(recorded_fees.get(&address_2), Some(&100));
// 	});
// }
