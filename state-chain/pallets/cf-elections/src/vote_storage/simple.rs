mod composite;
pub(crate) mod individual;
pub(crate) mod shared;

#[cfg(test)]
mod tests;

use frame_support::{pallet_prelude::Member, Parameter};

use super::{AuthorityVote, VoteComponents, VoteStorage};

use crate::{CorruptStorageError, SharedDataHash};

pub struct Simple<P: Parameter + Member, T: SimpleVoteStorage> {
	_phantom: core::marker::PhantomData<(P, T)>,
}
impl<P: Parameter + Member, T: SimpleVoteStorage> VoteStorage for Simple<P, T> {
	type Properties = P;

	type Vote = T::Vote;
	type PartialVote = T::PartialVote;

	type IndividualComponent = T::PartialVote;
	type BitmapComponent = ();
	type SharedData = T::SharedData;

	fn vote_into_partial_vote<H: Fn(Self::SharedData) -> SharedDataHash>(
		vote: &Self::Vote,
		h: H,
	) -> Self::PartialVote {
		T::vote_into_partial_vote(vote, h)
	}
	fn partial_vote_into_components(
		properties: Self::Properties,
		partial_vote: Self::PartialVote,
	) -> Result<VoteComponents<Self>, CorruptStorageError> {
		Ok(VoteComponents {
			bitmap_component: None,
			individual_component: Some((properties, partial_vote)),
		})
	}
	fn components_into_vote<
		F: FnMut(SharedDataHash) -> Result<Option<Self::SharedData>, CorruptStorageError>,
	>(
		vote_components: VoteComponents<Self>,
		f: F,
	) -> Result<
		Option<(Self::Properties, AuthorityVote<Self::PartialVote, Self::Vote>)>,
		CorruptStorageError,
	> {
		Ok(match vote_components {
			VoteComponents {
				bitmap_component: None,
				individual_component: Some((properties, partial_vote)),
			} => Some((
				properties,
				match T::partial_vote_into_vote(&partial_vote, f)? {
					Some(vote) => AuthorityVote::Vote(vote),
					None => AuthorityVote::PartialVote(partial_vote),
				},
			)),
			_ => None,
		})
	}
	fn visit_vote<E, F: Fn(Self::SharedData) -> Result<(), E>>(
		vote: Self::Vote,
		f: F,
	) -> Result<(), E> {
		T::visit_vote(vote, f)
	}
	fn visit_individual_component<F: Fn(SharedDataHash)>(
		individual_component: &Self::IndividualComponent,
		f: F,
	) {
		T::visit_partial_vote(individual_component, f)
	}
	fn visit_bitmap_component<F: Fn(SharedDataHash)>(
		_bitmap_component: &Self::BitmapComponent,
		_f: F,
	) {
	}
}
impl<P: Parameter + Member, T: SimpleVoteStorage> super::private::Sealed for Simple<P, T> {}

pub trait SimpleVoteStorage: private::Sealed + Sized {
	type Vote: Parameter + Member;
	type PartialVote: Parameter + Member;

	type SharedData: Parameter + Member;

	fn vote_into_partial_vote<H: Fn(Self::SharedData) -> SharedDataHash>(
		vote: &Self::Vote,
		h: H,
	) -> Self::PartialVote;
	fn partial_vote_into_vote<
		F: FnMut(SharedDataHash) -> Result<Option<Self::SharedData>, CorruptStorageError>,
	>(
		partial_vote: &Self::PartialVote,
		f: F,
	) -> Result<Option<Self::Vote>, CorruptStorageError>;

	fn visit_vote<E, F: Fn(Self::SharedData) -> Result<(), E>>(
		vote: Self::Vote,
		f: F,
	) -> Result<(), E>;
	fn visit_partial_vote<F: Fn(SharedDataHash)>(partial_vote: &Self::PartialVote, f: F);
}

mod private {
	/// Ensures `SimpleVoteStorage` can only be implemented here.
	pub trait Sealed {}
}