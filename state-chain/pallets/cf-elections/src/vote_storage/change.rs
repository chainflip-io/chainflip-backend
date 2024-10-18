use codec::{Decode, Encode};
use frame_support::{
	pallet_prelude::{Member, TypeInfo},
	Parameter,
};

use super::{AuthorityVote, VoteComponents, VoteStorage};

use crate::{CorruptStorageError, SharedDataHash};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
pub struct MonotonicChangeVote<Value, BlockHeight> {
	pub value: Value,
	pub block: BlockHeight,
}
pub struct MonotonicChange<T: Parameter + Member, S: Parameter + Member> {
	_phantom: core::marker::PhantomData<(T, S)>,
}
impl<T: Parameter + Member, S: Parameter + Member> VoteStorage for MonotonicChange<T, S> {
	type Properties = ();
	type Vote = MonotonicChangeVote<T, S>;
	type PartialVote = MonotonicChangeVote<SharedDataHash, S>;
	type IndividualComponent = S;
	type BitmapComponent = SharedDataHash;
	type SharedData = T;

	fn vote_into_partial_vote<H: FnMut(Self::SharedData) -> SharedDataHash>(
		vote: &Self::Vote,
		mut h: H,
	) -> Self::PartialVote {
		MonotonicChangeVote { value: h((vote.value).clone()), block: vote.block.clone() }
	}
	fn partial_vote_into_components(
		_properties: Self::Properties,
		partial_vote: Self::PartialVote,
	) -> Result<VoteComponents<Self>, CorruptStorageError> {
		Ok(VoteComponents {
			bitmap_component: Some(partial_vote.value),
			individual_component: Some(((), partial_vote.block)),
		})
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
			VoteComponents {
				bitmap_component: Some(bitmap_component),
				individual_component: Some((_properties, individual_component)),
			} => Some((
				(),
				match get_shared_data(bitmap_component)? {
					Some(vote) => AuthorityVote::Vote(MonotonicChangeVote {
						value: vote,
						block: individual_component,
					}),
					None => AuthorityVote::PartialVote(MonotonicChangeVote {
						value: bitmap_component,
						block: individual_component,
					}),
				},
			)),
			_ => None,
		})
	}
	fn visit_shared_data_in_vote<E, F: Fn(Self::SharedData) -> Result<(), E>>(
		vote: Self::Vote,
		f: F,
	) -> Result<(), E> {
		f(vote.value)?;
		Ok(())
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
		f(*bitmap_component)
	}
}
impl<T: Parameter + Member, S: Parameter + Member> super::private::Sealed
	for MonotonicChange<T, S>
{
}
