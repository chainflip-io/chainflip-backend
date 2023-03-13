use super::{MockPallet, MockPalletStorage};
use crate::EgressApi;
use cf_chains::Chain;
use cf_primitives::{AssetAmount, EgressId, ForeignChain};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::marker::PhantomData;

pub struct MockEgressHandler<C>(PhantomData<C>);

impl<C> MockPallet for MockEgressHandler<C> {
	const PREFIX: &'static [u8] = b"MockEgressHandler";
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockEgressParameter<C: Chain> {
	pub foreign_asset: C::ChainAsset,
	pub amount: AssetAmount,
	pub egress_address: C::ChainAccount,
	pub message: Vec<u8>,
}

impl<C: Chain> PartialOrd for MockEgressParameter<C> {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		self.amount.partial_cmp(&other.amount)
	}
}
impl<C: Chain> Ord for MockEgressParameter<C> {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		self.amount.cmp(&other.amount)
	}
}

impl<C: Chain> MockEgressHandler<C> {
	pub fn get_scheduled_egresses() -> Vec<MockEgressParameter<C>> {
		<Self as MockPalletStorage>::get_value(b"SCHEDULED_EGRESSES").unwrap_or_default()
	}
}

impl<C: Chain> EgressApi<C> for MockEgressHandler<C> {
	fn schedule_egress(
		foreign_asset: <C as Chain>::ChainAsset,
		amount: AssetAmount,
		egress_address: <C as Chain>::ChainAccount,
		message: Vec<u8>,
	) -> EgressId {
		<Self as MockPalletStorage>::mutate_value(b"SCHEDULED_EGRESSES", |storage| {
			if storage.is_none() {
				*storage = Some(vec![]);
			}
			storage.as_mut().map(|v| {
				v.push(MockEgressParameter::<C> { foreign_asset, amount, egress_address, message });
			})
		});
		(ForeignChain::Ethereum, 1)
	}
}
