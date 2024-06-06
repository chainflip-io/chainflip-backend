use cf_primitives::{Asset, AssetAmount};
use sp_runtime::AccountId32;

use crate::{mock::*, RecordedFees, WithheldTransactionFees};

fn payed_gas(asset: Asset, amount: AssetAmount, account: AccountId32) {
	Refunding::record_gas_fee(account, asset, amount);
	Refunding::withheld_transaction_fee(asset, amount);
}

#[test]
fn refund_validators() {
	new_test_ext().execute_with(|| {
		payed_gas(Asset::Eth, 100, AccountId32::from(ACCOUNT));
		payed_gas(Asset::Eth, 100, AccountId32::from(ACCOUNT_2));
		payed_gas(Asset::Eth, 100, AccountId32::from(ACCOUNT_3));

		let recorded_fees = RecordedFees::<Test>::get(Asset::Eth);

		assert_eq!(recorded_fees.get(&AccountId32::from(ACCOUNT)).unwrap(), &100);
		assert_eq!(recorded_fees.get(&AccountId32::from(ACCOUNT_2)).unwrap(), &100);
		assert_eq!(recorded_fees.get(&AccountId32::from(ACCOUNT_3)).unwrap(), &100);

		assert_eq!(WithheldTransactionFees::<Test>::get(Asset::Eth), 300);

		Refunding::on_distribute_withheld_fees();

		let recorded_fees = RecordedFees::<Test>::get(Asset::Eth);

		assert_eq!(recorded_fees.get(&AccountId32::from(ACCOUNT)), None);
		assert_eq!(recorded_fees.get(&AccountId32::from(ACCOUNT_2)), None);
		assert_eq!(recorded_fees.get(&AccountId32::from(ACCOUNT_3)), None);

		assert_eq!(WithheldTransactionFees::<Test>::get(Asset::Eth), 0);
	});
}

#[test]
fn ensure_available_funds_integrity() {
	new_test_ext().execute_with(|| {
		payed_gas(Asset::Eth, 100, AccountId32::from(ACCOUNT));
		payed_gas(Asset::Eth, 100, AccountId32::from(ACCOUNT_2));
		assert_eq!(WithheldTransactionFees::<Test>::get(Asset::Eth), 199);
		Refunding::on_distribute_withheld_fees();
		assert_eq!(WithheldTransactionFees::<Test>::get(Asset::Eth), 199);
		assert_eq!(recorded_fees.get(&AccountId32::from(ACCOUNT)), Some(100));
		assert_eq!(recorded_fees.get(&AccountId32::from(ACCOUNT_2)), Some(100));
	});
}
