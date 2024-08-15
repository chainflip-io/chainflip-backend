use frame_support::{pallet_prelude::Member, Parameter};

use super::IndividualVoteStorage;
use crate::{CorruptStorageError, SharedDataHash};

/// De-duplicates identical validator vote data, ensuring they will only be stored once. When vote
/// data is large, this can significantly decrease the amount of data that needs to be stored.
pub struct Shared<T: Parameter + Member> {
	_phantom: core::marker::PhantomData<T>,
}
impl<T: Parameter + Member> IndividualVoteStorage for Shared<T> {
	type Vote = T;
	type PartialVote = SharedDataHash;

	type SharedData = T;

	fn vote_into_partial_vote<H: FnMut(Self::SharedData) -> SharedDataHash>(
		vote: &Self::Vote,
		mut h: H,
	) -> Self::PartialVote {
		h(vote.clone())
	}
	fn partial_vote_into_vote<
		GetSharedData: FnMut(SharedDataHash) -> Result<Option<Self::SharedData>, CorruptStorageError>,
	>(
		partial_vote: &Self::PartialVote,
		mut get_shared_data: GetSharedData,
	) -> Result<Option<Self::Vote>, CorruptStorageError> {
		get_shared_data(*partial_vote)
	}

	fn visit_shared_data_in_vote<E, F: Fn(Self::SharedData) -> Result<(), E>>(
		vote: Self::Vote,
		f: F,
	) -> Result<(), E> {
		f(vote)
	}
	fn visit_shared_data_references_in_partial_vote<F: Fn(SharedDataHash)>(
		partial_vote: &Self::PartialVote,
		f: F,
	) {
		f(*partial_vote)
	}
}
impl<T: Parameter + Member> super::private::Sealed for Shared<T> {}
