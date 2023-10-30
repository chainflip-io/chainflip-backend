use crate::*;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

use self::old::NodeCFEVersionOld;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {
	use super::*;

	#[frame_support::storage_alias]
	pub type NodeCFEVersionOld<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, ValidatorIdOf<T>, SemVer, ValueQuery>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let cfe_versions = NodeCFEVersionOld::<T>::drain();

		for old_storage in cfe_versions {
			NodeCFEVersion::<T>::insert(
				old_storage.0,
				Versions { cfe: old_storage.1, node: old_storage.1 },
			);
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let storage_len = NodeCFEVersionOld::<T>::iter_keys().count;

		Ok(storage_len.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let old_storage_len = <usize>::decode(&mut &state[..]).unwrap();
		ensure!(
			NodeCFEVersionOld::<T>::iter_keys().count == old_storage_len,
			"NodeCFEVersion migration failed."
		);
		Ok(())
	}
}

#[cfg(test)]
mod test_runtime_upgrade {
	use super::*;
	use mock::Test;
	pub const ACCOUNT_ID: <mock::Test as frame_system::Config>::AccountId = 12345;

	#[test]
	fn test() {
		mock::new_test_ext().execute_with(|| {
			// pre upgrade
			NodeCFEVersionOld::<Test>::insert(ACCOUNT_ID, SemVer { major: 1, minor: 2, patch: 4 });

			#[cfg(feature = "try-runtime")]
			let state = Migration::<Test>::pre_upgrade().unwrap();

			// upgrade
			Migration::<Test>::on_runtime_upgrade();

			// post upgrade
			#[cfg(feature = "try-runtime")]
			Migration::<Test>::post_upgrade(state).unwrap();

			let expected_versions = Versions {
				cfe: SemVer { major: 1, minor: 2, patch: 4 },
				node: SemVer { major: 1, minor: 2, patch: 4 },
			};
			assert_eq!(
				NodeCFEVersion::<Test>::get(ACCOUNT_ID),
				expected_versions,
				"Versions are incorrect."
			);
		});
	}
}
