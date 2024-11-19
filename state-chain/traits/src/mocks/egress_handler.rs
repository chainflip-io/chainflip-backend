use super::{MockPallet, MockPalletStorage};
use crate::{EgressApi, ScheduledEgressDetails};
use cf_chains::{CcmAdditionalData, CcmDepositMetadata, CcmMessage, Chain};
use cf_primitives::{AssetAmount, EgressCounter, GasAmount};
use codec::{Decode, Encode};
use frame_support::sp_runtime::{
	traits::{Saturating, Zero},
	DispatchError,
};
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
		fee: C::ChainAmount,
		destination_address: C::ChainAccount,
	},
	Ccm {
		asset: C::ChainAsset,
		amount: C::ChainAmount,
		destination_address: C::ChainAccount,
		message: CcmMessage,
		ccm_additional_data: CcmAdditionalData,
		gas_budget: GasAmount,
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

	pub fn set_fee(amount: C::ChainAmount) {
		<Self as MockPalletStorage>::put_value(b"EGRESS_FEE", amount);
	}
	pub fn return_failure(fail: bool) {
		<Self as MockPalletStorage>::put_value(b"EGRESS_FAIL", fail);
	}
}

impl<C: Chain> EgressApi<C> for MockEgressHandler<C> {
	type EgressError = DispatchError;

	fn schedule_egress(
		asset: <C as Chain>::ChainAsset,
		amount: <C as Chain>::ChainAmount,
		destination_address: <C as Chain>::ChainAccount,
		maybe_ccm_deposit_metadata: Option<CcmDepositMetadata>,
	) -> Result<ScheduledEgressDetails<C>, DispatchError> {
		if amount.is_zero() && maybe_ccm_deposit_metadata.is_none() {
			return Err(DispatchError::from("Ignoring zero egress amount."))
		}
		if <Self as MockPalletStorage>::get_value(b"EGRESS_FAIL").unwrap_or_default() {
			return Err(DispatchError::from("Egress failed."));
		}
		let egress_fee = <Self as MockPalletStorage>::get_value(b"EGRESS_FEE").unwrap_or_default();
		<Self as MockPalletStorage>::mutate_value(b"SCHEDULED_EGRESSES", |storage| {
			if storage.is_none() {
				*storage = Some(vec![]);
			}
			storage.as_mut().map(|v| {
				v.push(match &maybe_ccm_deposit_metadata {
					Some(message) => MockEgressParameter::<C>::Ccm {
						asset,
						amount,
						destination_address,
						message: message.channel_metadata.message.clone(),
						ccm_additional_data: message.channel_metadata.ccm_additional_data.clone(),
						gas_budget: message.channel_metadata.gas_budget,
					},
					None => MockEgressParameter::<C>::Swap {
						asset,
						amount: amount.saturating_sub(egress_fee),
						destination_address,
						fee: egress_fee,
					},
				});
			})
		});
		let len = Self::get_scheduled_egresses().len();
		Ok(ScheduledEgressDetails {
			egress_id: (asset.into(), len as EgressCounter),
			egress_amount: amount,
			fee_withheld: egress_fee,
		})
	}
}
