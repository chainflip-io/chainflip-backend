use frame_support::{pallet_prelude::Member, Parameter};

use super::SimpleVoteStorage;
use crate::{CorruptStorageError, SharedDataHash};

/// Stores each validator's vote individually, without de-duplicating identical values. This is
/// useful when a vote's encoding is close to the size of `SharedDataHash`'s or if the validator's
/// votes aren't likely to be equal.
pub struct Individual<T: Parameter + Member> {
	_phantom: core::marker::PhantomData<T>,
}
impl<T: Parameter + Member> SimpleVoteStorage for Individual<T> {
	type Vote = T;
	type PartialVote = T;

	// Cannot use `Infallible` here, as scale-codec doesn't implement the codec traits on it.
	type SharedData = ();

	fn vote_into_partial_vote<H: Fn(Self::SharedData) -> SharedDataHash>(
		vote: &Self::Vote,
		_h: H,
	) -> Self::PartialVote {
		vote.clone()
	}
	fn partial_vote_into_vote<
		F: FnMut(SharedDataHash) -> Result<Option<Self::SharedData>, CorruptStorageError>,
	>(
		partial_vote: &Self::PartialVote,
		_f: F,
	) -> Result<Option<Self::Vote>, CorruptStorageError> {
		Ok(Some(partial_vote.clone()))
	}

	fn visit_vote<E, F: Fn(Self::SharedData) -> Result<(), E>>(
		_vote: Self::Vote,
		_f: F,
	) -> Result<(), E> {
		Ok(())
	}
	fn visit_partial_vote<F: Fn(SharedDataHash)>(_partial_vote: &Self::PartialVote, _f: F) {}
}
impl<T: Parameter + Member> super::private::Sealed for Individual<T> {}
