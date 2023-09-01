use super::{MockPallet, MockPalletStorage};
use crate::EgressApi;
use cf_chains::{CcmDepositMetadata, Chain};
use cf_primitives::{AssetAmount, EgressId, ForeignChain, GasUnit};
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
		amount: C::ChainAmount,
		destination_address: C::ChainAccount,
	},
	Ccm {
		asset: C::ChainAsset,
		amount: C::ChainAmount,
		destination_address: C::ChainAccount,
		message: Vec<u8>,
		cf_parameters: Vec<u8>,
		gas_limit: GasUnit,
	},
}

impl<C: Chain> MockEgressParameter<C> {
	pub fn amount(&self) -> AssetAmount {
		match self {
			Self::Swap { amount, .. } => *amount,
			Self::Ccm { amount, .. } => *amount,
		}
		.into()
	}
}

impl<C: Chain> PartialOrd for MockEgressParameter<C> {
	fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
		Some(self.cmp(other))
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
		amount: <C as Chain>::ChainAmount,
		destination_address: <C as Chain>::ChainAccount,
		maybe_message: Option<(CcmDepositMetadata, GasUnit)>,
	) -> EgressId {
		<Self as MockPalletStorage>::mutate_value(b"SCHEDULED_EGRESSES", |storage| {
			if storage.is_none() {
				*storage = Some(vec![]);
			}
			storage.as_mut().map(|v| {
				v.push(match maybe_message {
					Some((message, gas_limit)) => MockEgressParameter::<C>::Ccm {
						asset,
						amount,
						destination_address,
						message: message.channel_metadata.message,
						cf_parameters: message.channel_metadata.cf_parameters,
						gas_limit,
					},
					None => MockEgressParameter::<C>::Swap { asset, amount, destination_address },
				});
			})
		});
		let len = Self::get_scheduled_egresses().len();
		(ForeignChain::Ethereum, len as u64)
	}
}
