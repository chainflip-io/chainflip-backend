use crate::*;
use frame_support::traits::OnRuntimeUpgrade;

mod old {
	use super::*;

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct PrewitnessedDeposit<C: Chain> {
		pub asset: C::ChainAsset,
		pub amount: C::ChainAmount,
		pub deposit_address: C::ChainAccount,
		pub block_height: C::ChainBlockNumber,
		pub deposit_details: C::DepositDetails,
	}

	#[frame_support::storage_alias]
	pub type PrewitnessedDeposits<T: Config<I>, I: 'static> = StorageDoubleMap<
		Pallet<T, I>,
		Twox64Concat,
		ChannelId,
		Twox64Concat,
		PrewitnessedDepositId,
		PrewitnessedDeposit<<T as Config<I>>::TargetChain>,
	>;
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		let _ = old::PrewitnessedDeposits::<T, I>::clear(u32::MAX, None);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
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

		new_test_ext().execute_with(|| {
			let address1 = ScriptPubkey::Taproot([0u8; 32]);

			old::PrewitnessedDeposits::<Test, _>::insert(
				1,
				2,
				old::PrewitnessedDeposit {
					asset: cf_chains::assets::btc::Asset::Btc,
					amount: 0,
					deposit_address: address1.clone(),
					block_height: 0,
					deposit_details: UtxoId { tx_id: H256::zero(), vout: 0 },
				},
			);

			// Perform runtime migration.
			super::Migration::<Test, _>::on_runtime_upgrade();

			// Test that we delete all entries as part of the migration:
			assert_eq!(old::PrewitnessedDeposits::<Test, _>::iter_keys().count(), 0);
		});
	}
}
