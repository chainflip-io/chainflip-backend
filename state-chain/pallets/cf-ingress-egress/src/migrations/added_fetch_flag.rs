use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;

pub mod old {
	use super::*;

	#[derive(Clone, PartialEq, Eq, Encode, Decode)]
	pub struct TransactionRejectionDetails<T: Config<I>, I: 'static> {
		pub deposit_address: Option<TargetChainAccount<T, I>>,
		pub refund_address: Option<ForeignChainAddress>,
		pub asset: TargetChainAsset<T, I>,
		pub amount: TargetChainAmount<T, I>,
		pub deposit_details: <T::TargetChain as Chain>::DepositDetails,
	}

	#[frame_support::storage_alias]
	pub type ScheduledTransactionsForRejection<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, Vec<TransactionRejectionDetails<T, I>>, ValueQuery>;

	#[frame_support::storage_alias]
	pub(crate) type FailedRejections<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, Vec<TransactionRejectionDetails<T, I>>, ValueQuery>;
}

pub struct RenameScheduledTxForReject<T: Config<I>, I: 'static = ()>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for RenameScheduledTxForReject<T, I> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let count = old::ScheduledTransactionsForRejection::<T, I>::get().len() as u64;
		Ok(count.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let current_scheduled_txs = old::ScheduledTransactionsForRejection::<T, I>::take();
		let mut translated_scheduled_txs: Vec<_> = Vec::new();
		for tx in current_scheduled_txs {
			translated_scheduled_txs.push(crate::TransactionRejectionDetails::<T, I> {
				deposit_address: tx.deposit_address,
				refund_address: tx.refund_address,
				asset: tx.asset,
				amount: tx.amount,
				deposit_details: tx.deposit_details,
				should_fetch: true,
			});
		}
		crate::ScheduledTransactionsForRejection::<T, I>::put(translated_scheduled_txs);

		let failed_rejections = old::FailedRejections::<T, I>::take();
		let mut translated_failed_rejections: Vec<_> = Vec::new();

		for tx in failed_rejections {
			translated_failed_rejections.push(crate::TransactionRejectionDetails::<T, I> {
				deposit_address: tx.deposit_address,
				refund_address: tx.refund_address,
				asset: tx.asset,
				amount: tx.amount,
				deposit_details: tx.deposit_details,
				should_fetch: true,
			});
		}

		crate::FailedRejections::<T, I>::put(translated_failed_rejections);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let pre_upgrade_count = <u64>::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let post_upgrade_count =
			crate::ScheduledTransactionsForRejection::<T, I>::get().len() as u64;

		assert_eq!(pre_upgrade_count, post_upgrade_count);
		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	use crate::{
		mock_btc::{new_test_ext, Test},
		tests::screening::helpers::generate_btc_deposit,
	};
	use cf_chains::assets::btc::Asset;
	use sp_core::H256;

	use super::*;

	#[test]
	fn test_migration() {
		new_test_ext().execute_with(|| {
			let deposit_details = generate_btc_deposit(H256::zero());
			old::ScheduledTransactionsForRejection::<Test, ()>::put(vec![
				old::TransactionRejectionDetails {
					deposit_address: None,
					refund_address: None,
					asset: Asset::Btc,
					amount: 1000000000000000000,
					deposit_details: deposit_details.clone(),
				},
			]);

			RenameScheduledTxForReject::<Test, ()>::on_runtime_upgrade();

			assert_eq!(
				ScheduledTransactionsForRejection::<Test, ()>::get(),
				vec![TransactionRejectionDetails {
					deposit_address: None,
					refund_address: None,
					asset: Asset::Btc,
					amount: 1000000000000000000,
					deposit_details,
					should_fetch: true,
				},]
			);
		});
	}
}
