use crate::*;
use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};

use codec::{Decode, Encode};
use pallet_cf_elections::{
	electoral_systems::composite::tuple_7_impls::CompositeElectoralUnsynchronisedStateMapKey,
	ElectoralUnsynchronisedStateMap,
};
use sp_core::bounded::alloc::collections::BTreeSet;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

pub struct SolanaRemoveUnusedChannelsState;

impl UncheckedOnRuntimeUpgrade for SolanaRemoveUnusedChannelsState {
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
		for key in keys.into_iter() {
			if let CompositeElectoralUnsynchronisedStateMapKey::C((address, _asset)) = key {
				if !addresses_in_use.contains(&address) {
					ElectoralUnsynchronisedStateMap::<Runtime, SolanaInstance>::remove(key);
				}
			}
		}

		Weight::zero()
	}

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
