use super::{MockPallet, MockPalletStorage};
use crate::EgressApi;
use cf_chains::Chain;
use cf_primitives::{AssetAmount, CcmIngressMetadata, EgressId, ForeignChain, ForeignChainAddress};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::marker::PhantomData;

pub struct MockEgressHandler<C>(PhantomData<C>);

impl<C> MockPallet for MockEgressHandler<C> {
	const PREFIX: &'static [u8] = b"MockEgressHandler";
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum MockEgressParameter<C: Chain> {
	Swap {
		asset: C::ChainAsset,
		amount: AssetAmount,
		egress_address: C::ChainAccount,
	},
	Ccm {
		asset: C::ChainAsset,
		amount: AssetAmount,
		egress_address: C::ChainAccount,
		message: Vec<u8>,
		refund_address: ForeignChainAddress,
	},
}

impl<C: Chain> MockEgressParameter<C> {
	pub fn amount(&self) -> AssetAmount {
		match self {
			Self::Swap { amount, .. } => *amount,
			Self::Ccm { amount, .. } => *amount,
		}
	}
}

impl<C: Chain> PartialOrd for MockEgressParameter<C> {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		self.amount().partial_cmp(&other.amount())
	}
}
impl<C: Chain> Ord for MockEgressParameter<C> {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		self.amount().cmp(&other.amount())
	}
}

impl<C: Chain> MockEgressHandler<C> {
	pub fn get_scheduled_egresses() -> Vec<MockEgressParameter<C>> {
		<Self as MockPalletStorage>::get_value(b"SCHEDULED_EGRESSES").unwrap_or_default()
	}
}

impl<C: Chain> EgressApi<C> for MockEgressHandler<C> {
	fn schedule_egress(
		asset: <C as Chain>::ChainAsset,
		amount: AssetAmount,
		egress_address: <C as Chain>::ChainAccount,
		maybe_message: Option<CcmIngressMetadata>,
	) -> EgressId {
		<Self as MockPalletStorage>::mutate_value(b"SCHEDULED_EGRESSES", |storage| {
			if storage.is_none() {
				*storage = Some(vec![]);
			}
			storage.as_mut().map(|v| {
				v.push(match maybe_message {
					Some(message) => MockEgressParameter::<C>::Ccm {
						asset,
						amount,
						egress_address,
						message: message.message,
						refund_address: message.refund_address,
					},
					None => MockEgressParameter::<C>::Swap { asset, amount, egress_address },
				});
			})
		});
		(ForeignChain::Ethereum, 1)
	}
}
