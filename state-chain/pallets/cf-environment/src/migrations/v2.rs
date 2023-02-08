use crate::*;
use cf_chains::dot::RuntimeVersion;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let old_metadata = super::v1::old::PolkadotNetworkMetadata::<T>::take();

		PolkadotRuntimeVersion::<T>::put(RuntimeVersion {
			spec_version: old_metadata.spec_version,
			transaction_version: old_metadata.transaction_version,
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, &'static str> {
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: sp_std::vec::Vec<u8>) -> Result<(), &'static str> {
		Ok(())
	}
}
