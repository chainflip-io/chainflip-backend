use crate::*;
#[cfg(feature = "try-runtime")]
use frame_support::dispatch::DispatchError;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sp_std::marker::PhantomData;

mod old_types {
	use super::*;
	use codec::{Decode, Encode};

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum OldRequestType<Key, Participants> {
		CurrentKey,
		SpecificKey(Key, EpochIndex),
		KeygenVerification { key: Key, epoch_index: EpochIndex, participants: Participants },
	}

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct OldRequestInstruction<T: Config<I>, I: 'static> {
		pub request_context: RequestContext<T, I>,
		pub request_type:
			OldRequestType<<T::TargetChainCrypto as ChainCrypto>::AggKey, BTreeSet<T::ValidatorId>>,
	}

	impl<T: Config<I>, I: 'static> From<OldRequestInstruction<T, I>> for RequestInstruction<T, I> {
		fn from(old: OldRequestInstruction<T, I>) -> Self {
			RequestInstruction {
				request_context: old.request_context,
				request_type: match old.request_type {
					OldRequestType::CurrentKey => <T as Config<I>>::KeyProvider::active_epoch_key()
						.map(|EpochKey { key, epoch_index, .. }| {
							RequestType::SpecificKey(key, epoch_index)
						})
						.expect("All live chains have active keys"),
					OldRequestType::SpecificKey(key, epoch_index) =>
						RequestType::SpecificKey(key, epoch_index),
					OldRequestType::KeygenVerification { key, epoch_index, participants } =>
						RequestType::KeygenVerification { key, epoch_index, participants },
				},
			}
		}
	}
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		PendingRequestInstructions::<T, I>::translate::<old_types::OldRequestInstruction<T, I>, _>(
			|_id, old| Some(old.into()),
		);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let c = PendingRequestInstructions::<T, I>::iter_keys().count() as u32;
		Ok(c.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let c = u32::decode(&mut &state[..]).map_err(|_| DispatchError::Other("Invalid state"))?;
		// Use iter_values so we get an error message if the value fails to decode.
		assert_eq!(c, PendingRequestInstructions::<T, I>::iter_values().count() as u32);
		log::info!("Migrated {} pending request instructions", c);
		Ok(())
	}
}
