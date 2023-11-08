use crate::*;
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		NodeCFEVersion::<T>::translate(|_key, cfe_version| {
			Some(NodeCFEVersions { cfe: cfe_version, node: cfe_version })
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let storage_len: u64 = NodeCFEVersion::<T>::iter_keys().count() as u64;
		Ok(storage_len.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let old_storage_len = <u64>::decode(&mut &state[..]).unwrap();
		ensure!(
			NodeCFEVersion::<T>::iter_keys().count() as u64 == old_storage_len,
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
	mod old {
		use super::*;

		#[frame_support::storage_alias]
		pub type NodeCFEVersion<T: Config> =
			StorageMap<Pallet<T>, Blake2_128Concat, ValidatorIdOf<T>, SemVer, ValueQuery>;
	}
	#[test]
	fn test() {
		mock::new_test_ext().execute_with(|| {
			// pre upgrade
			old::NodeCFEVersion::<Test>::insert(
				ACCOUNT_ID,
				SemVer { major: 1, minor: 2, patch: 4 },
			);

			#[cfg(feature = "try-runtime")]
			let state = Migration::<Test>::pre_upgrade().unwrap();

			// upgrade
			Migration::<Test>::on_runtime_upgrade();

			// post upgrade
			#[cfg(feature = "try-runtime")]
			Migration::<Test>::post_upgrade(state).unwrap();

			let expected_versions = NodeCFEVersions {
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
