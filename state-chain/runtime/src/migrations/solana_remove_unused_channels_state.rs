use crate::*;
use frame_support::{pallet_prelude::Weight, traits::OnRuntimeUpgrade};

use pallet_cf_elections::{
	electoral_systems::composite::tuple_6_impls::CompositeElectoralUnsynchronisedStateMapKey,
	ElectoralUnsynchronisedStateMap,
};
use sp_core::bounded::alloc::collections::BTreeSet;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

pub struct SolanaRemoveUnusedChannelsState;

impl OnRuntimeUpgrade for SolanaRemoveUnusedChannelsState {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		assert!(ElectoralUnsynchronisedStateMap::<Runtime, SolanaInstance>::iter_keys()
			.next()
			.is_some());
		let no_of_items_pre_upgrade: u64 =
			ElectoralUnsynchronisedStateMap::<Runtime, SolanaInstance>::iter_keys()
				.count()
				.try_into()
				.unwrap();
		Ok(no_of_items_pre_upgrade.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let addresses_in_use: BTreeSet<_> = pallet_cf_ingress_egress::DepositChannelLookup::<
			Runtime,
			SolanaInstance,
		>::iter_keys()
		.chain(
			pallet_cf_ingress_egress::DepositChannelPool::<Runtime, SolanaInstance>::iter_values()
				.map(|deposit_channel| deposit_channel.address),
		)
		.collect();

		let keys = ElectoralUnsynchronisedStateMap::<Runtime, SolanaInstance>::iter_keys()
			.collect::<Vec<_>>();

		log::info!("🧮 Number of Solana Addresses stored before migration: {:?}", keys.len());
		let mut removed = 0;
		for key in keys.into_iter() {
			if let CompositeElectoralUnsynchronisedStateMapKey::B((address, _asset)) = key {
				if !addresses_in_use.contains(&address) {
					ElectoralUnsynchronisedStateMap::<Runtime, SolanaInstance>::remove(key);
					removed += 1;
				}
			}
		}
		log::info!("🚮 Number of addresses deleted: {:?}", removed);
		if removed == 0 {
			log::warn!("❗️ No Solana Addresses deleted - consider removing this migration.");
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use core::assert;

		let no_of_items_pre_upgrade = <u64>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		assert!(
			no_of_items_pre_upgrade >=
				ElectoralUnsynchronisedStateMap::<Runtime, SolanaInstance>::iter_keys()
					.count()
					.try_into()
					.unwrap()
		);

		Ok(())
	}
}
