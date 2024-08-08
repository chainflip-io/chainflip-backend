use frame_support::traits::OnRuntimeUpgrade;

use crate::PendingApiCalls;

pub struct Migration<T, I>(sp_std::marker::PhantomData<(T, I)>);

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod old {
	use cf_primitives::BroadcastId;
	use frame_support::Twox64Concat;

	use crate::{ApiCallFor, Config, Pallet, ThresholdSignatureFor};

	#[frame_support::storage_alias]
	pub type ThresholdSignatureData<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		BroadcastId,
		// the threshold signature data is migrated to this new ApiCall in
		// `migrate_apicalls_to_store_signer` which is run from storage version 3->4
		(ApiCallFor<T, I>, ThresholdSignatureFor<T, I>),
	>;
}

impl<T: crate::Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok((old::ThresholdSignatureData::<T, I>::iter().count() as u32).encode())
	}

	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		for (id, (api_call, _sig)) in old::ThresholdSignatureData::<T, I>::drain() {
			PendingApiCalls::<T, I>::insert(id, api_call);
		}

		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let old_count = <u32>::decode(&mut &state[..]).expect("Failed to decode count");
		let new_count = PendingApiCalls::<T, I>::iter().count() as u32;
		assert_eq!(old_count, new_count, "Migration failed: counts do not match");
		Ok(())
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use cf_chains::{
		mocks::{MockAggKey, MockApiCall, MockEthereumChainCrypto, MockThresholdSignature},
		ChainCrypto,
	};
	use frame_support::instances::Instance1;

	use crate::mock::new_test_ext;

	#[test]
	fn migration_test() {
		const ID: u32 = 42;
		const API_CALL: MockApiCall<MockEthereumChainCrypto> =
			MockApiCall { payload: [b'p'; 4], signer_and_signature: None, tx_out_id: [b't'; 4] };

		const SIG: MockThresholdSignature<
			<MockEthereumChainCrypto as ChainCrypto>::AggKey,
			<MockEthereumChainCrypto as ChainCrypto>::Payload,
		> = MockThresholdSignature { signing_key: MockAggKey([b'k'; 4]), signed_payload: [b'p'; 4] };

		new_test_ext().execute_with(|| {
			frame_support::storage::unhashed::put(
				old::ThresholdSignatureData::<crate::mock::Test, Instance1>::hashed_key_for(ID)
					.as_ref(),
				&(API_CALL, SIG),
			);

			Migration::<crate::mock::Test, Instance1>::on_runtime_upgrade();

			assert_eq!(
				PendingApiCalls::<crate::mock::Test, Instance1>::get(ID)
					.expect("Migration should succeed"),
				API_CALL
			);
		});
	}
}
