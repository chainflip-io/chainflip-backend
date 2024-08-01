use crate::*;
use frame_support::traits::OnRuntimeUpgrade;

#[cfg(feature = "try-runtime")]
use frame_support::sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

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
		let current_aggkey = Keys::<T, I>::get(CurrentKeyEpoch::<T, I>::get().unwrap_or_default())
			.unwrap_or_default();

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
		Ok(old::Signature::<T, I>::iter().collect::<Vec<_>>().encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let signatures =
			Vec::<(u32, AsyncResult<SignatureResultFor<T, I>>)>::decode(&mut &state[..])
				.map_err(|_| DispatchError::Other("Failed to decode Signatures"))?;

		let current_aggkey = Keys::<T, I>::get(CurrentKeyEpoch::<T, I>::get().unwrap_or_default())
			.unwrap_or_default();

		assert_eq!(
			signatures.len(),
			SignerAndSignature::<T, I>::iter_keys().collect::<Vec<_>>().len()
		);

		for (request_id, signature) in signatures {
			let SignerAndSignatureResult { signer, signature_result } =
				SignerAndSignature::<T, I>::get(request_id).unwrap();
			assert_eq!(signer, current_aggkey);
			assert_eq!(signature, signature_result);
		}
		Ok(())
	}
}
