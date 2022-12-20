use super::{MockPallet, MockPalletStorage};
use crate::EgressApi;
use cf_chains::Chain;
use cf_primitives::{AssetAmount, EgressId, ForeignChain};
use sp_std::marker::PhantomData;

pub struct MockEgressHandler<C>(PhantomData<C>);

impl<C> MockPallet for MockEgressHandler<C> {
	const PREFIX: &'static [u8] = b"MockEgressHandler";
}

impl<C: Chain> MockEgressHandler<C> {
	pub fn get_scheduled_egresses() -> Vec<(C::ChainAsset, AssetAmount, C::ChainAccount)> {
		<Self as MockPalletStorage>::get_value(b"SCHEDULED_EGRESSES").unwrap_or_default()
	}
}

impl<C: Chain> EgressApi<C> for MockEgressHandler<C> {
	fn schedule_egress(
		foreign_asset: <C as Chain>::ChainAsset,
		amount: AssetAmount,
		egress_address: <C as Chain>::ChainAccount,
	) -> EgressId {
		<Self as MockPalletStorage>::mutate_value(b"SCHEDULED_EGRESSES", |storage| {
			if storage.is_none() {
				*storage = Some(vec![]);
			}
			storage.as_mut().map(|v| {
				v.push((foreign_asset, amount, egress_address));
			})
		});
		(ForeignChain::Ethereum, 1)
	}
}
