use crate::*;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sp_std::marker::PhantomData;

mod old_types {
	use super::*;
	use codec::{Decode, Encode};

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode)]
	pub struct CeremonyContext<T: Config<I>, I: 'static> {
		pub request_context: RequestContext<T, I>,
		/// The respondents that have yet to reply.
		pub remaining_respondents: BTreeSet<T::ValidatorId>,
		/// The number of blame votes (accusations) each authority has received.
		pub blame_counts: BTreeMap<T::ValidatorId, AuthorityCount>,
		/// The total number of signing participants (ie. the threshold set size).
		pub participant_count: AuthorityCount,
		/// The epoch in which the ceremony was started.
		pub epoch: EpochIndex,
		/// The key we want to sign with.
		pub key: <T::TargetChain as ChainCrypto>::AggKey,
		/// Determines how/if we deal with ceremony failure.
		pub threshold_ceremony_type: ThresholdCeremonyType,
	}
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		PendingCeremonies::<T, I>::translate::<old_types::CeremonyContext<T, I>, _>(|_id, old| {
			Some(CeremonyContext {
				request_context: old.request_context,
				remaining_respondents: old.remaining_respondents,
				blame_counts: old.blame_counts,
				// We don't know the actual participants, but it's more important that we get the
				// set size right, otherwise the threshold will be incorrect.
				candidates: <<T as Chainflip>::EpochInfo as EpochInfo>::current_authorities()
					.into_iter()
					.take(old.participant_count as usize)
					.collect(),
				epoch: old.epoch,
				key: old.key,
				threshold_ceremony_type: old.threshold_ceremony_type,
			})
		});
		Weight::from_ref_time(0)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), &'static str> {
		Ok(())
	}
}
