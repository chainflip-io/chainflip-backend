use crate::*;
use frame_support::traits::OnRuntimeUpgrade;

#[cfg(feature = "try-runtime")]
use frame_support::sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::{vec, vec::Vec};

pub struct Migration<T, I>(sp_std::marker::PhantomData<(T, I)>);

mod old {
	use frame_support::{pallet_prelude::ValueQuery, Twox64Concat};

	use super::*;

	#[frame_support::storage_alias]
	pub type Signature<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		RequestId,
		AsyncResult<SignatureResultFor<T, I>>,
		ValueQuery,
	>;
}

impl<T: crate::Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let current_aggkey = Keys::<T, I>::get(CurrentKeyEpoch::<T, I>::get().unwrap()).unwrap();

		old::Signature::<T, I>::drain().for_each(|(request_id, signature_result)| {
			SignerAndSignature::<T, I>::insert(
				request_id,
				SignerAndSignatureResult { signer: current_aggkey, signature_result },
			);
		});

		Default::default()
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
