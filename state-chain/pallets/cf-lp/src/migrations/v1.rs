use crate::*;
use frame_support::weights::Weight;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		frame_support::migration::move_prefix(
			&frame_support::storage::storage_prefix(
				Pallet::<T>::name().as_bytes(),
				b"EmergencyWithdrawalAddress",
			),
			&frame_support::storage::storage_prefix(
				Pallet::<T>::name().as_bytes(),
				b"LiquidityRefundAddress",
			),
		);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(frame_support::storage::migration::storage_iter(
			Pallet::<T>::name().as_bytes(),
			b"EmergencyWithdrawalAddress",
		)
		.map(|(_k, v)| v)
		.collect::<Vec<ForeignChainAddress>>()
		.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let before = <Vec<ForeignChainAddress>>::decode(&mut &state[..])
			.map_err(|_| "Failed to decode post-upgrade state.")?;
		let after = LiquidityRefundAddress::<T>::iter_values().collect::<Vec<_>>();
		ensure!(before == after, "LiquidityRefundAddress mismatch!");
		Ok(())
	}
}
