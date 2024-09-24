use codec::{Decode, Encode};
use frame_support::{pallet_prelude::Member, Parameter};
use frame_support::pallet_prelude::TypeInfo;

use super::{AuthorityVote, VoteComponents, VoteStorage};

use crate::{CorruptStorageError, SharedDataHash};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
pub struct NonceVote<Value, Slot> {
	pub value: Value,
	pub slot: Slot
}
pub struct NonceStorage<T: Parameter + Member, S: Parameter + Member> {
	_phantom: core::marker::PhantomData<(T, S)>,
}
impl<T: Parameter + Member, S: Parameter + Member> VoteStorage for NonceStorage<T, S> {
	type Properties = ();
	type Vote = NonceVote<T,S>;
	type PartialVote = NonceVote<T,S>;
	type IndividualComponent = S;
	type BitmapComponent = T;
	type SharedData = ();

	fn vote_into_partial_vote<H: FnMut(Self::SharedData) -> SharedDataHash>(
		vote: &Self::Vote,
		mut _h: H,
	) -> Self::PartialVote {
		(*vote).clone()
	}
	fn partial_vote_into_components(
		_properties: Self::Properties,
		partial_vote: Self::PartialVote,
	) -> Result<VoteComponents<Self>, CorruptStorageError> {
		Ok(VoteComponents {
			bitmap_component: Some(partial_vote.value),
			individual_component: Some((_properties, partial_vote.slot)),
		})
	}
	fn components_into_authority_vote<
		GetSharedData: FnMut(SharedDataHash) -> Result<Option<Self::SharedData>, CorruptStorageError>,
	>(
		vote_components: VoteComponents<Self>,
		mut _get_shared_data: GetSharedData,
	) -> Result<
		Option<(Self::Properties, AuthorityVote<Self::PartialVote, Self::Vote>)>,
		CorruptStorageError,
	> {
		Ok(match vote_components {
			VoteComponents {
				bitmap_component: Some(bitmap_component),
				individual_component: Some((_properties, individual_component)),
			} => Some(((), AuthorityVote::Vote(NonceVote{value: bitmap_component, slot: individual_component}))),
			_ => None,
		})
	}
	fn visit_shared_data_in_vote<E, F: Fn(Self::SharedData) -> Result<(), E>>(
		_vote: Self::Vote,
		_f: F,
	) -> Result<(), E> {
		Ok(())
	}
	fn visit_shared_data_references_in_individual_component<F: Fn(SharedDataHash)>(
		_individual_component: &Self::IndividualComponent,
		_f: F,
	) {
	}
	fn visit_shared_data_references_in_bitmap_component<F: Fn(SharedDataHash)>(
		_bitmap_component: &Self::BitmapComponent,
		_f: F,
	) {
	}
}
impl<T: Parameter + Member, S: Parameter + Member> super::private::Sealed for NonceStorage<T, S> {}
