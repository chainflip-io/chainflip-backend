use super::{MockPallet, MockPalletStorage};
use crate::TransactionFeeApi;
use cf_chains::Chain;

use sp_std::marker::PhantomData;

pub struct MockTransactionFeeHandler<C>(PhantomData<C>);

impl<C> MockPallet for MockTransactionFeeHandler<C> {
	const PREFIX: &'static [u8] = b"MockTransactionFeeHandler";
}

const TRANSACTION_FEES: &str = "TRANSACTION_FEES";

impl<C: Chain> MockTransactionFeeHandler<C> {
	pub fn get_withheld_transaction_fees(asset: C::ChainAsset) -> C::ChainAmount {
		let asset: cf_primitives::Asset = asset.into();
		Self::get_value(format!("{TRANSACTION_FEES}_{asset}").as_bytes()).unwrap_or_default()
	}
}

impl<C: Chain> TransactionFeeApi<C> for MockTransactionFeeHandler<C> {
	fn accrue_transaction_fee(asset: C::ChainAsset, fee: C::ChainAmount) {
		let asset: cf_primitives::Asset = asset.into();
		Self::mutate_value(
			format!("{TRANSACTION_FEES}_{asset}").as_bytes(),
			|fees: &mut Option<C::ChainAmount>| {
				*fees = Some(fees.unwrap_or_default() + fee);
			},
		);
	}
}
