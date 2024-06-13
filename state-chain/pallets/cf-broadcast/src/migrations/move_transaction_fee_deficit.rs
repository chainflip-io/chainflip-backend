use crate::*;
use frame_support::traits::OnRuntimeUpgrade;

use frame_support::pallet_prelude::{ValueQuery, Weight};

mod old {
	use super::*;

	#[frame_support::storage_alias]
	pub type TransactionFeeDeficit<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, SignerIdFor<T, I>, ChainAmountFor<T, I>, ValueQuery>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		for (signer_id, to_refund) in old::TransactionFeeDeficit::<T, I>::drain() {
			T::Refunding::record_gas_fees(
				signer_id.clone().into(),
				<T::TargetChain as Chain>::GAS_ASSET,
				to_refund,
			);
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		assert_eq!(
			old::TransactionFeeDeficit::<T, I>::decoded_len(),
			None,
			"TransactionFeeDeficit not empty - migration failed!"
		);
		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	use cf_chains::btc::UtxoId;
	use sp_core::H256;

	#[test]
	fn test_migration() {
		use cf_chains::btc::ScriptPubkey;

		use super::*;
		use crate::mock_btc::*;

		new_test_ext().execute_with(|| {});
	}
}
