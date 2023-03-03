use crate::*;
use cf_primitives::KeyId;
use codec::{Decode, Encode};
use frame_support::weights::Weight;
use sp_std::marker::PhantomData;

use super::v1;

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

mod old {

	use super::*;

	// The old version was generic over key id, but it was only using a Vec<u8> so
	// we can just use that type directly
	#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum RequestType<Participants> {
		/// Will use the current key and current authority set.
		/// This signing request will be retried until success.
		Standard,
		/// Uses the recently generated key and the participants used to generate that key.
		/// This signing request will only be attemped once, as failing this ought to result
		/// in another Keygen ceremony.
		KeygenVerification { key_id: v1::old::KeyId, participants: Participants },
	}

	#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct RequestInstruction<T: Config<I>, I: 'static> {
		pub request_context: pallet::RequestContext<T, I>,
		pub request_type: RequestType<BTreeSet<T::ValidatorId>>,
	}
}

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let epoch_index = T::EpochInfo::epoch_index();
		PendingCeremonies::<T, I>::translate_values::<v1::archived::CeremonyContext<T, I>, _>(
			|v1::archived::CeremonyContext {
			     request_context,
			     remaining_respondents,
			     blame_counts,
			     participant_count,
			     key_id,
			     threshold_ceremony_type,
			 }| {
				Some(pallet::CeremonyContext::<T, I> {
					request_context,
					remaining_respondents,
					blame_counts,
					participant_count,
					key_id: KeyId {
						// Assumption is that there will be no requests
						// for previous epoch keys, which is currently true all the time.
						epoch_index,
						public_key_bytes: key_id,
					},
					threshold_ceremony_type,
				})
			},
		);

		PendingRequestInstructions::<T, I>::translate_values::<old::RequestInstruction<T, I>, _>(
			|old::RequestInstruction::<T, I> { request_context, request_type }| {
				Some(pallet::RequestInstruction::<T, I> {
					request_context,
					request_type: match request_type {
						old::RequestType::Standard => RequestType::Standard,
						old::RequestType::KeygenVerification { key_id, participants } =>
							RequestType::KeygenVerification {
								key_id: KeyId { epoch_index, public_key_bytes: key_id },
								participants,
							},
					},
				})
			},
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), &'static str> {
		Ok(())
	}
}

#[cfg(test)]
mod tests {

	use super::*;

	use cf_traits::mocks::epoch_info::MockEpochInfo;

	mod old_storage {
		use super::*;
		use frame_support::Twox64Concat;

		#[frame_support::storage_alias]
		pub type PendingRequestInstructions<T: Config<I>, I: 'static> =
			StorageMap<Pallet<T, I>, Twox64Concat, RequestId, super::old::RequestInstruction<T, I>>;
	}

	#[test]
	fn test_migration_of_key_id() {
		mock::new_test_ext().execute_with(|| {
			let request_context = RequestContext {
				request_id: 1,
				attempt_count: 902,
				payload: [0x32, 0x24, 0xab, 0x92],
			};
			let request_key_id = vec![1, 2, 3];
			let old_request_instruction = old::RequestInstruction {
				request_context: request_context.clone(),
				request_type: old::RequestType::KeygenVerification {
					key_id: request_key_id.clone(),
					participants: BTreeSet::from([1, 2, 3, 4]),
				},
			};
			let epoch_index = MockEpochInfo::epoch_index();

			const REQUEST_ID: RequestId = 1;

			old_storage::PendingRequestInstructions::<mock::Test, mock::Instance1>::insert(
				REQUEST_ID,
				old_request_instruction.clone(),
			);

			let ceremony_key_id = vec![5, 6, 7];
			let old_ceremony_context = v1::archived::CeremonyContext {
				request_context: request_context.clone(),
				remaining_respondents: BTreeSet::from([1, 2, 3, 4]),
				blame_counts: BTreeMap::from([(1, 2), (2, 3), (3, 4)]),
				participant_count: 4,
				key_id: ceremony_key_id.clone(),
				threshold_ceremony_type: ThresholdCeremonyType::Standard,
			};
			const CEREMONY_ID: CeremonyId = 1;

			v1::archived::PendingCeremonies::<mock::Test, mock::Instance1>::insert(
				CEREMONY_ID,
				old_ceremony_context.clone(),
			);

			Migration::<mock::Test, mock::Instance1>::on_runtime_upgrade();

			let new_instruction =
				PendingRequestInstructions::<mock::Test, mock::Instance1>::get(REQUEST_ID).unwrap();
			assert_eq!(new_instruction.request_context, request_context);
			match new_instruction.request_type {
				RequestType::Standard => panic!("Expected KeygenVerification"),
				RequestType::KeygenVerification { key_id, .. } => {
					assert_eq!(key_id.epoch_index, epoch_index);
					assert_eq!(key_id.public_key_bytes, request_key_id);
				},
			}

			let new_ceremony_context =
				PendingCeremonies::<mock::Test, mock::Instance1>::get(CEREMONY_ID).unwrap();

			assert_eq!(new_ceremony_context.request_context, request_context);
			assert_eq!(new_ceremony_context.key_id.epoch_index, epoch_index);
			assert_eq!(new_ceremony_context.key_id.public_key_bytes, ceremony_key_id);
		});
	}
}
