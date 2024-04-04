use super::{MockPallet, MockPalletStorage};
use crate::IngressEgressFeeApi;
use cf_chains::Chain;

use sp_std::marker::PhantomData;

pub struct MockIngressEgressFeeHandler<C>(PhantomData<C>);

impl<C> MockPallet for MockIngressEgressFeeHandler<C> {
	const PREFIX: &'static [u8] = b"MockIngressEgressFeeHandler";
}

const WITHHELD_FEES: &[u8] = b"WITHHELD_FEES";

impl<C: Chain> MockIngressEgressFeeHandler<C> {
	pub fn get_withheld_transaction_fees(asset: C::ChainAsset) -> C::ChainAmount {
		let asset: cf_primitives::Asset = asset.into();
		Self::get_storage(WITHHELD_FEES, asset).unwrap_or_default()
	}
}

impl<C: Chain> IngressEgressFeeApi<C> for MockIngressEgressFeeHandler<C> {
	fn accrue_withheld_fee(asset: C::ChainAsset, fee: C::ChainAmount) {
		let asset: cf_primitives::Asset = asset.into();
		Self::mutate_storage::<cf_primitives::Asset, _, _, _, _>(
			WITHHELD_FEES,
			&asset,
			|fees: &mut Option<C::ChainAmount>| {
				*fees = Some(fees.unwrap_or_default() + fee);
			},
		);
	}
}
