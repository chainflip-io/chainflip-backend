use crate::*;
use core::marker::PhantomData;
use frame_support::traits::OnRuntimeUpgrade;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {

	use super::*;

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct CcmSwap {
		source_asset: Asset,
		deposit_amount: AssetAmount,
		destination_asset: Asset,
		destination_address: ForeignChainAddress,
		deposit_metadata: CcmDepositMetadata,
		principal_swap_id: Option<SwapId>,
		gas_swap_id: Option<SwapId>,
	}

	#[derive(
		Clone, Copy, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen,
	)]
	pub struct CcmSwapOutput {
		principal: Option<AssetAmount>,
		gas: Option<AssetAmount>,
	}

	#[frame_support::storage_alias]
	pub type CcmIdCounter<T: Config> = StorageValue<Pallet<T>, u64, ValueQuery>;

	#[frame_support::storage_alias]
	pub type PendingCcms<T: Config> = StorageMap<Pallet<T>, Twox64Concat, u64, CcmSwap>;

	#[frame_support::storage_alias]
	pub type CcmOutputs<T: Config> = StorageMap<Pallet<T>, Twox64Concat, u64, CcmSwapOutput>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		// Note: we don't migrate items from SwapQueue because we will
		// ensure that it is empty during the upgrade.

		log::info!("Swapping redesign migration!");

		let latest_used_ccm_id = old::CcmIdCounter::<T>::get();
		let latest_used_swap_id = SwapIdCounter::<T>::get();

		SwapRequestIdCounter::<T>::put(latest_used_ccm_id + latest_used_swap_id);

		// These are no longer used:
		old::CcmIdCounter::<T>::kill();
		let _ = old::PendingCcms::<T>::clear(u32::MAX, None);
		let _ = old::CcmOutputs::<T>::clear(u32::MAX, None);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let latest_used_ccm_id = old::CcmIdCounter::<T>::get();
		let latest_used_swap_id = SwapIdCounter::<T>::get();

		let expected_swap_request_id = latest_used_ccm_id + latest_used_swap_id;
		Ok(expected_swap_request_id.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let expected_swap_request_id = SwapRequestId::decode(&mut &state[..]).unwrap();
		let actual_swap_request_id = SwapIdCounter::<T>::get();

		assert_eq!(expected_swap_request_id, actual_swap_request_id, "Swap request id mismatch");

		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	use super::*;

	#[test]
	fn test_migration() {
		use crate::mock::{new_test_ext, Test};
		new_test_ext().then_execute_at_block(10u64, |_| {
			old::CcmIdCounter::<Test>::put(2);
			SwapIdCounter::<Test>::put(3);

			Migration::<Test>::on_runtime_upgrade();

			assert_eq!(SwapRequestIdCounter::<Test>::get(), 5);
		});
	}
}
