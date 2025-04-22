use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::Config; // FailedRejections

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

use codec::{Decode, Encode};

pub mod old {
	use cf_chains::ForeignChainAddress;
	use frame_support::pallet_prelude::ValueQuery;

	use super::*;

	#[derive(PartialEq, Eq, Encode, Decode)]
	pub struct TransactionRejectionDetails<T: Config<I>, I: 'static> {
		pub deposit_address: Option<TargetChainAccount<T, I>>,
		pub refund_address: ForeignChainAddress,
		pub asset: TargetChainAsset<T, I>,
		pub amount: TargetChainAmount<T, I>,
		pub deposit_details: <T::TargetChain as Chain>::DepositDetails,
		// This migration adds refund_ccm_metadata
	}

	#[frame_support::storage_alias]
	pub type ScheduledTransactionsForRejection<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, Vec<TransactionRejectionDetails<T, I>>, ValueQuery>;

	#[frame_support::storage_alias]
	pub type FailedRejections<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, Vec<TransactionRejectionDetails<T, I>>, ValueQuery>;
}

pub struct Migration<T: Config<I>, I: 'static = ()>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for Migration<T, I> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let scheduled_count = old::ScheduledTransactionsForRejection::<T, I>::get().len() as u64;
		let failed_count = old::FailedRejections::<T, I>::get().len() as u64;
		Ok((scheduled_count, failed_count).encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let _ = crate::ScheduledTransactionsForRejection::<T, I>::translate::<
			Vec<old::TransactionRejectionDetails<T, I>>,
			_,
		>(|maybe_old_vec| {
			maybe_old_vec.map(|old_vec| {
				old_vec
					.into_iter()
					.map(|old_item| TransactionRejectionDetails::<T, I> {
						deposit_address: old_item.deposit_address,
						refund_address: old_item.refund_address,
						asset: old_item.asset,
						amount: old_item.amount,
						deposit_details: old_item.deposit_details,
						refund_ccm_metadata: None,
					})
					.collect()
			})
		});

		let _ = crate::FailedRejections::<T, I>::translate::<
			Vec<old::TransactionRejectionDetails<T, I>>,
			_,
		>(|maybe_old_vec| {
			maybe_old_vec.map(|old_vec| {
				old_vec
					.into_iter()
					.map(|old_item| TransactionRejectionDetails::<T, I> {
						deposit_address: old_item.deposit_address,
						refund_address: old_item.refund_address,
						asset: old_item.asset,
						amount: old_item.amount,
						deposit_details: old_item.deposit_details,
						refund_ccm_metadata: None,
					})
					.collect()
			})
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let (pre_scheduled_txs_rejection_count, pre_failed_rejections_count) =
			<(u64, u64)>::decode(&mut &state[..])
				.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_scheduled_txs_rejection_count =
			crate::ScheduledTransactionsForRejection::<T, I>::get().len() as u64;
		let post_failed_rejections_count = crate::FailedRejections::<T, I>::get().len() as u64;

		assert_eq!(pre_scheduled_txs_rejection_count, post_scheduled_txs_rejection_count);
		assert_eq!(pre_failed_rejections_count, post_failed_rejections_count);

		Ok(())
	}
}
