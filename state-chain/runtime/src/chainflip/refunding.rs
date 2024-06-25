use crate::Refunding;

use cf_traits::Refunding as RefundingTrait;

use cf_chains::ForeignChainAddress;
use cf_primitives::{Asset, AssetAmount};

use cf_chains::ForeignChain;
use pallet_cf_refunding::{RecordedFees, WithheldTransactionFees};

use crate::Runtime;

pub struct RefundingHandler;

impl RefundingTrait for RefundingHandler {
	fn record_gas_fees(address: ForeignChainAddress, asset: Asset, amount: AssetAmount) {
		Refunding::record_gas_fee(address, asset.into(), amount);
	}
	fn withhold_transaction_fee(asset: Asset, amount: AssetAmount) {
		Refunding::withhold_transaction_fee(asset.into(), amount);
	}
	fn get_withheld_transaction_fees(asset: Asset) -> AssetAmount {
		let chain: ForeignChain = asset.into();
		WithheldTransactionFees::<Runtime>::get(chain)
	}
	fn get_recorded_gas_fees(asset: Asset) -> u128 {
		let chain: ForeignChain = asset.into();
		RecordedFees::<Runtime>::get(chain).unwrap_or_default().values().len() as u128
	}
}
