use crate::*;
use cf_chains::dot::RuntimeVersion;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let old_metadata = super::v1::archived::PolkadotNetworkMetadata::<T>::take();

		PolkadotRuntimeVersion::<T>::put(RuntimeVersion {
			spec_version: old_metadata.spec_version,
			transaction_version: old_metadata.transaction_version,
		});
		PolkadotGenesisHash::<T>::set(old_metadata.genesis_hash.into());

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, &'static str> {
		let before = super::v1::archived::PolkadotNetworkMetadata::<T>::get();

		Ok(before.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: sp_std::vec::Vec<u8>) -> Result<(), &'static str> {
		let before_metadata =
			super::v1::archived::PolkadotMetadata::decode(&mut &state[..]).unwrap();

		let after_version = PolkadotRuntimeVersion::<T>::get();
		assert_eq!(
			before_metadata.spec_version, after_version.spec_version,
			"Spec version mismatch"
		);
		assert_eq!(
			before_metadata.transaction_version, after_version.transaction_version,
			"Transaction version mismatch"
		);
		assert_eq!(before_metadata.genesis_hash.into(), PolkadotGenesisHash::<T>::get());
		Ok(())
	}
}
