use crate::*;

use codec::{Decode, Encode};
use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};
use scale_info::TypeInfo;

pub mod old {
	use super::*;

	#[derive(Encode, Decode, TypeInfo, MaxEncodedLen)]
	pub struct CrossChainMessage<C: Chain> {
		pub egress_id: EgressId,
		pub asset: C::ChainAsset,
		pub amount: C::ChainAmount,
		pub destination_address: C::ChainAccount,
		pub message: CcmMessage,
		// The sender of the deposit transaction.
		pub source_chain: ForeignChain,
		pub source_address: Option<ForeignChainAddress>,
		// Where funds might be returned to if the message fails.
		pub ccm_additional_data: CcmAdditionalData,
		pub gas_budget: GasAmount,
	}
}

pub struct AddCcmAuxDataLookupKeyMigration<T, I>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for AddCcmAuxDataLookupKeyMigration<T, I> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(BTreeSet::from_iter(
			ScheduledEgressCcm::<T, I>::get().into_iter().map(|ccm| ccm.egress_id),
		)
		.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		log::info!("🍗 Running migration for IngressEgress pallet: Adding Ccm aux data lookup key to `CrossChainMessage`.");
		if let Err(e) = ScheduledEgressCcm::<T, I>::translate::<
			old::CrossChainMessage<T::TargetChain>,
			_,
		>(|old_ccms| {
			Some(
				old_ccms
					.into_iter()
					.map(|ccm| crate::CrossChainMessage {
						egress_id: ccm.egress_id,
						asset: ccm.asset,
						amount: ccm.amount,
						destination_address: ccm.destination_address,
						message: ccm.message,
						source_chain: ccm.source_chain,
						source_address: ccm.source_address,
						ccm_additional_data: ccm.ccm_additional_data,
						gas_budget: ccm.gas_budget,
						aux_data_lookup_key: Default::default(),
					})
					.collect::<Vec<_>>(),
			)
		}) {
			log::error!("❌🍗 Migration for IngressEgress pallet: AddCcmAuxDataLookupKeyMigration failed. Error {:?}", e);
		};

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let pre_upgrade = BTreeSet::decode(&mut &state[..]).unwrap();
		let post_upgrade = BTreeSet::from_iter(
			ScheduledEgressCcm::<T, I>::get().into_iter().map(|ccm| ccm.egress_id),
		);

		assert_eq!(pre_upgrade, post_upgrade);

		Ok(())
	}
}
