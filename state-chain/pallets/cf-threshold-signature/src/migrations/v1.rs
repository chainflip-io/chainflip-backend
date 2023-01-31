use crate::*;
use frame_support::{pallet_prelude::ValueQuery, weights::Weight, Twox64Concat};
use sp_std::marker::PhantomData;

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

mod old {

	use super::*;

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
		pub key_id: Option<T::KeyId>,
		pub retry_policy: RetryPolicy,
	}

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct CeremonyContext<T: Config<I>, I: 'static> {
		pub request_context: RequestContext<T, I>,
		pub remaining_respondents: BTreeSet<T::ValidatorId>,
		pub blame_counts: BTreeMap<T::ValidatorId, u32>,
		pub participant_count: u32,
		pub key_id: T::KeyId,
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

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		for (k, v) in old::RetryQueues::<T, I>::iter().drain() {
			CeremonyRetryQueues::<T, I>::insert(k, v);
		}

		for (
			k,
			old::CeremonyContext {
				request_context:
					old::RequestContext { request_id, attempt_count, payload, key_id: _, retry_policy },
				remaining_respondents,
				blame_counts,
				participant_count,
				key_id,
				_phantom,
			},
		) in old::PendingCeremonies::<T, I>::iter().drain()
		{
			let new_request_context = RequestContext { request_id, attempt_count, payload };

			PendingCeremonies::<T, I>::insert(
				k,
				CeremonyContext {
					request_context: new_request_context,
					remaining_respondents,
					blame_counts,
					participant_count,
					key_id,
					threshold_ceremony_type: if retry_policy == old::RetryPolicy::Never {
						ThresholdCeremonyType::KeygenVerification
					} else {
						ThresholdCeremonyType::Standard
					},
				},
			);
		}

		old::LiveCeremonies::<T, I>::iter().drain().for_each(|_| {
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
