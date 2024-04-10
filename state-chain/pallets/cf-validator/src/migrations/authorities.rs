use crate::{Config, CurrentAuthorities, HistoricalAuthorities, ValidatorIdOf};
use core::marker::PhantomData;
use frame_support::{sp_runtime::DispatchError, traits::OnRuntimeUpgrade, weights::Weight};
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let result = CurrentAuthorities::<T>::translate::<BTreeSet<ValidatorIdOf<T>>, _>(|btree| {
			Some(btree.unwrap().into_iter().collect::<Vec<ValidatorIdOf<T>>>())
		});
		if result.is_err() {
			println!("ERROR DURING MIGRATION");
		}

		HistoricalAuthorities::<T>::translate::<BTreeSet<ValidatorIdOf<T>>, _>(
			|_epoch_index, btree| Some(btree.into_iter().collect::<Vec<ValidatorIdOf<T>>>()),
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}
