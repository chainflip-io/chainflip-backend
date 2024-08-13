#[cfg(test)]
mod tests;

use frame_support::{pallet_prelude::Member, Parameter};

use super::{AuthorityVote, VoteComponents, VoteStorage};

use crate::{CorruptStorageError, SharedDataHash};

pub struct Bitmap<T: Parameter + Member> {
	_phantom: core::marker::PhantomData<T>,
}
impl<T: Parameter + Member> VoteStorage for Bitmap<T> {
	type Properties = ();

	type Vote = T;
	type PartialVote = SharedDataHash;

	type IndividualComponent = ();
	type BitmapComponent = SharedDataHash;
	type SharedData = T;

	fn vote_into_partial_vote<H: Fn(Self::SharedData) -> SharedDataHash>(
		vote: &Self::Vote,
		h: H,
	) -> Self::PartialVote {
		h(vote.clone())
	}
	fn partial_vote_into_components(
		_properties: Self::Properties,
		partial_vote: Self::PartialVote,
	) -> Result<VoteComponents<Self>, CorruptStorageError> {
		Ok(VoteComponents { bitmap_component: Some(partial_vote), individual_component: None })
	}
	fn components_into_authority_vote<
		GetSharedData: FnMut(SharedDataHash) -> Result<Option<Self::SharedData>, CorruptStorageError>,
	>(
		vote_components: VoteComponents<Self>,
		mut get_shared_data: GetSharedData,
	) -> Result<
		Option<(Self::Properties, AuthorityVote<Self::PartialVote, Self::Vote>)>,
		CorruptStorageError,
	> {
		Ok(match vote_components {
			VoteComponents { bitmap_component: Some(partial_vote), individual_component: None } =>
				Some((
					(),
					match get_shared_data(partial_vote)? {
						Some(vote) => AuthorityVote::Vote(vote),
						None => AuthorityVote::PartialVote(partial_vote),
					},
				)),
			_ => None,
		})
	}
	fn visit_shared_data_in_vote<E, F: Fn(Self::SharedData) -> Result<(), E>>(
		vote: Self::Vote,
		f: F,
	) -> Result<(), E> {
		f(vote)
	}
	fn visit_shared_data_references_in_individual_component<F: Fn(SharedDataHash)>(
		_individual_component: &Self::IndividualComponent,
		_f: F,
	) {
	}
	fn visit_shared_data_references_in_bitmap_component<F: Fn(SharedDataHash)>(
		bitmap_component: &Self::BitmapComponent,
		f: F,
	) {
		f(*bitmap_component);
	}
}
impl<T: Parameter + Member> super::private::Sealed for Bitmap<T> {}
