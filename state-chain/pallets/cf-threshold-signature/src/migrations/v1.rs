use crate::*;
use frame_support::{pallet_prelude::ValueQuery, weights::Weight, Twox64Concat};
use sp_std::marker::PhantomData;

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

pub mod old {

	use super::*;

	pub type KeyId = Vec<u8>;

	#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
	pub enum RetryPolicy {
		Always,
		Never,
	}

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct RequestContext<T: Config<I>, I: 'static> {
		pub request_id: RequestId,
		pub attempt_count: AttemptCount,
		pub payload: PayloadFor<T, I>,
		pub key_id: Option<KeyId>,
		pub retry_policy: RetryPolicy,
	}

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct CeremonyContext<T: Config<I>, I: 'static> {
		pub request_context: RequestContext<T, I>,
		pub remaining_respondents: BTreeSet<T::ValidatorId>,
		pub blame_counts: BTreeMap<T::ValidatorId, u32>,
		pub participant_count: u32,
		pub key_id: KeyId,
		pub _phantom: PhantomData<I>,
	}

	#[frame_support::storage_alias]
	pub type RetryQueues<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, BlockNumberFor<T>, Vec<CeremonyId>, ValueQuery>;

	#[frame_support::storage_alias]
	pub type PendingCeremonies<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, CeremonyId, CeremonyContext<T, I>>;

	#[frame_support::storage_alias]
	pub type LiveCeremonies<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, RequestId, (CeremonyId, AttemptCount)>;
}

// Types that are not old in the context of this migration,
// but are old in the context of the pallet
pub mod archived {

	use super::*;

	// We migrate to the old key id. The next migration migrates to the new key.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct CeremonyContext<T: Config<I>, I: 'static> {
		pub request_context: RequestContext<T, I>,
		/// The respondents that have yet to reply.
		pub remaining_respondents: BTreeSet<T::ValidatorId>,
		/// The number of blame votes (accusations) each authority has received.
		pub blame_counts: BTreeMap<T::ValidatorId, AuthorityCount>,
		/// The total number of signing participants (ie. the threshold set size).
		pub participant_count: AuthorityCount,
		/// The key id being used for verification of this ceremony.
		pub key_id: old::KeyId,
		/// Determines how/if we deal with ceremony failure.
		pub threshold_ceremony_type: ThresholdCeremonyType,
	}

	#[frame_support::storage_alias]
	pub type PendingCeremonies<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, CeremonyId, CeremonyContext<T, I>>;
}

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		for (k, v) in old::RetryQueues::<T, I>::drain().collect::<Vec<_>>() {
			CeremonyRetryQueues::<T, I>::insert(k, v);
		}

		archived::PendingCeremonies::<T, I>::translate_values::<old::CeremonyContext<T, I>, _>(
			|old::CeremonyContext {
			     request_context:
			         old::RequestContext { request_id, attempt_count, payload, key_id: _, retry_policy },
			     remaining_respondents,
			     blame_counts,
			     participant_count,
			     key_id,
			     _phantom,
			 }| {
				Some(archived::CeremonyContext {
					request_context: RequestContext { request_id, attempt_count, payload },
					remaining_respondents,
					blame_counts,
					participant_count,
					key_id,
					threshold_ceremony_type: if retry_policy == old::RetryPolicy::Never {
						ThresholdCeremonyType::KeygenVerification
					} else {
						ThresholdCeremonyType::Standard
					},
				})
			},
		);

		old::LiveCeremonies::<T, I>::drain().for_each(|_| {
			// drain it
		});

		// No need to initialise the `RequestRetryQueue` as it was a new introduction
		// so it can start empty.

		// No need to initialise PendingRequestInstructions since this is only used
		// if the is no key at the point of signing, which is not possible on v0.

		Weight::zero()
	}

	// We need to test this on a runtime that actually has some pending ceremonies.
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
		Ok((
			old::RetryQueues::<T, I>::iter_keys().collect::<Vec<_>>().len() as u32,
			old::PendingCeremonies::<T, I>::iter_keys().collect::<Vec<_>>().len() as u32,
		)
			.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
		let (old_retry_queue_len, old_pending_ceremonies_len) =
			<(u32, u32)>::decode(&mut &state[..]).map_err(|_| "Invalid state")?;

		assert_eq!(
			old_retry_queue_len,
			CeremonyRetryQueues::<T, I>::iter_keys().collect::<Vec<_>>().len() as u32
		);
		assert_eq!(
			old_pending_ceremonies_len,
			PendingCeremonies::<T, I>::iter_keys().collect::<Vec<_>>().len() as u32
		);

		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	use super::*;
	use cf_chains::mocks::MockEthereum;
	use cf_traits::mocks::key_provider::MockKeyProvider;

	#[test]
	fn migration_successful_with_retry_queue_and_pending_ceremony_items() {
		mock::new_test_ext().execute_with(|| {
			let mut retry_queue = Vec::new();

			for i in 0..10 {
				retry_queue.push(i);
			}

			let key_id = MockKeyProvider::<MockEthereum>::current_epoch_key();

			const BLOCK_NUMBER: u64 = 4;

			old::RetryQueues::<mock::Test, mock::Instance1>::insert(
				BLOCK_NUMBER,
				retry_queue.clone(),
			);

			const CEREMONY_ID: CeremonyId = 1;
			const PAYLOAD: [u8; 4] = [7; 4];
			const REQUEST_ID: RequestId = 1;
			const ATTEMPT_COUNT: AttemptCount = 4;
			const BLAME_COUNTS: [(u64, u32); 3] = [(1, 1), (7, 4), (3, 6)];
			const PARTICIPANT_COUNT: u32 = 25;

			let pending_ceremony = old::CeremonyContext {
				request_context: old::RequestContext {
					request_id: REQUEST_ID,
					attempt_count: ATTEMPT_COUNT,
					payload: PAYLOAD,
					key_id: None,
					retry_policy: old::RetryPolicy::Always,
				},
				remaining_respondents: BTreeSet::default(),
				blame_counts: BTreeMap::from(BLAME_COUNTS),
				participant_count: PARTICIPANT_COUNT,
				key_id: key_id.key.0.to_vec(),
				_phantom: PhantomData,
			};

			old::PendingCeremonies::<mock::Test, mock::Instance1>::insert(
				CEREMONY_ID,
				pending_ceremony.clone(),
			);

			assert_eq!(
				old::RetryQueues::<mock::Test, mock::Instance1>::get(BLOCK_NUMBER),
				retry_queue
			);
			assert_eq!(
				old::PendingCeremonies::<mock::Test, mock::Instance1>::get(CEREMONY_ID).unwrap(),
				pending_ceremony
			);

			#[cfg(feature = "try-runtime")]
			let state = Migration::<mock::Test, mock::Instance1>::pre_upgrade().unwrap();
			Migration::<mock::Test, mock::Instance1>::on_runtime_upgrade();
			#[cfg(feature = "try-runtime")]
			Migration::<mock::Test, mock::Instance1>::post_upgrade(state).unwrap();

			assert_eq!(
				CeremonyRetryQueues::<mock::Test, mock::Instance1>::get(BLOCK_NUMBER),
				retry_queue
			);

			assert_eq!(
				archived::PendingCeremonies::<mock::Test, mock::Instance1>::get(CEREMONY_ID)
					.unwrap(),
				archived::CeremonyContext {
					request_context: RequestContext {
						request_id: REQUEST_ID,
						attempt_count: ATTEMPT_COUNT,
						payload: PAYLOAD,
					},
					remaining_respondents: BTreeSet::default(),
					blame_counts: BTreeMap::from(BLAME_COUNTS),
					participant_count: PARTICIPANT_COUNT,
					key_id: key_id.key.0.to_vec(),
					threshold_ceremony_type: ThresholdCeremonyType::Standard,
				}
			);
		})
	}
}
