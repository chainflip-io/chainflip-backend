use crate::*;
use frame_support::traits::OnRuntimeUpgrade;

use frame_support::pallet_prelude::{DispatchError, ValueQuery, Weight};

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
			let address_to_refund = <SignerIdFor<T, I> as IntoForeignChainAddress<
				T::TargetChain,
			>>::into_foreign_chain_address(signer_id);
			T::Refunding::record_gas_fee(
				address_to_refund,
				<T::TargetChain as Chain>::GAS_ASSET.into(),
				to_refund.into(),
			);
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let total_amount_of_recorded_fees =
			old::TransactionFeeDeficit::<T, I>::iter().collect::<Vec<_>>().len() as u128;
		Ok(total_amount_of_recorded_fees.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		assert_eq!(
			old::TransactionFeeDeficit::<T, I>::iter().collect::<Vec<_>>().len(),
			0,
			"TransactionFeeDeficit not empty - migration failed!"
		);
		let recorded_fees = <u128>::decode(&mut &state[..]).unwrap();
		let migrated =
			T::Refunding::get_recorded_gas_fees(<T::TargetChain as Chain>::GAS_ASSET.into());
		assert_eq!(recorded_fees, migrated, "Migrated fees do not match for asset!");
		Ok(())
	}
}
