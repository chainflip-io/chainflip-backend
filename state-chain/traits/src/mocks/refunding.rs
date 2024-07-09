use cf_chains::ForeignChainAddress;
use cf_primitives::{Asset, AssetAmount};
use std::cell::RefCell;

use crate::Refunding;

thread_local! {
	pub static WITHHELD_TRANSACTION_FEES: std::cell::RefCell<AssetAmount> = std::cell::RefCell::new(0);
	pub static TRANSACTION_FEE_DEFICIT: RefCell<AssetAmount> = RefCell::new(0);
}
pub struct MockRefunding;

impl MockRefunding {
	pub fn get_withheld_transaction_fees() -> AssetAmount {
		WITHHELD_TRANSACTION_FEES.with(|cell| *cell.borrow())
	}
	pub fn get_transaction_fee_deficit() -> AssetAmount {
		TRANSACTION_FEE_DEFICIT.with(|cell| *cell.borrow())
	}
}

impl Refunding for MockRefunding {
	fn record_gas_fee(_: ForeignChainAddress, _: Asset, amount: AssetAmount) {
		TRANSACTION_FEE_DEFICIT.with(|cell| {
			*cell.borrow_mut() += amount;
		});
	}

	fn withhold_transaction_fee(_: Asset, amount: AssetAmount) {
		WITHHELD_TRANSACTION_FEES.with(|cell| {
			*cell.borrow_mut() += amount;
		});
	}
}
