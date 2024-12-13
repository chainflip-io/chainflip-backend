use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::{Config, CrossChainMessage};

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub mod old {
	use cf_chains::ForeignChainAddress;

	use super::*;

	#[derive(PartialEq, Eq, Encode, Decode)]
	pub struct CrossChainMessage<C: Chain> {
		pub egress_id: EgressId,
		pub asset: C::ChainAsset,
		pub amount: C::ChainAmount,
		pub destination_address: C::ChainAccount,
		pub message: CcmMessage,
		pub source_chain: ForeignChain,
		pub source_address: Option<ForeignChainAddress>,
		pub ccm_additional_data: CcmAdditionalData,
		pub gas_budget: C::ChainAmount,
	}

	#[frame_support::storage_alias]
	pub type ScheduledEgressCcm<T: Config<I>, I: 'static> = StorageValue<
		Pallet<T, I>,
		Vec<CrossChainMessage<<T as Config<I>>::TargetChain>>,
		ValueQuery,
	>;
}

pub struct ScheduledEgressCcmMigration<T: Config<I>, I: 'static = ()>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for ScheduledEgressCcmMigration<T, I> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let count = old::ScheduledEgressCcm::<T, I>::get().len() as u64;
		Ok(count.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let _ = crate::ScheduledEgressCcm::<T, I>::translate::<
			Vec<old::CrossChainMessage<T::TargetChain>>,
			_,
		>(|old_cross_chain_messages| {
			match old_cross_chain_messages {
				None => None,
				Some(old_cross_chain_messages) => {
					let mut new_cross_chain_messages =
						Vec::with_capacity(old_cross_chain_messages.len());
					for old_cross_chain_message in old_cross_chain_messages {
						new_cross_chain_messages.push(CrossChainMessage {
							egress_id: old_cross_chain_message.egress_id,
							asset: old_cross_chain_message.asset,
							amount: old_cross_chain_message.amount,
							destination_address: old_cross_chain_message.destination_address,
							message: old_cross_chain_message.message,
							source_chain: old_cross_chain_message.source_chain,
							source_address: old_cross_chain_message.source_address,
							ccm_additional_data: old_cross_chain_message.ccm_additional_data,
							// gas_budget: match T::TargetChain as Chain {
							// 	Chain::Ethereum => 300_000.into(),
							// 	Chain::Arbitrum => 1_500_000.into(),
							// 	Chain::Solana => 600_000.into(),
							// 	_ => 0.into(),
							// },
							// TODO: What gas_budget should we put here?
							gas_budget: old_cross_chain_message.gas_budget.into(),
						});
					}
					Some(new_cross_chain_messages)
				},
			}
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_scheduled_egress_ccm_count = <u64>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_scheduled_egress_ccm_count = crate::ScheduledEgressCcm::<T, I>::get().len() as u64;

		assert_eq!(pre_scheduled_egress_ccm_count, post_scheduled_egress_ccm_count);
		Ok(())
	}
}
