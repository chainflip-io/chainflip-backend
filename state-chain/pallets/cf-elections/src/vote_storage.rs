use crate::{CorruptStorageError, SharedDataHash};

use codec::{Decode, Encode};
use frame_support::{pallet_prelude::Member, Parameter};
use scale_info::TypeInfo;

pub(crate) mod bitmap;
pub mod composite;
pub(crate) mod individual;

#[derive(PartialEq, Eq, Clone, Debug, Encode, Decode, TypeInfo)]
pub enum AuthorityVote<PartialVote, Vote> {
	PartialVote(PartialVote),
	Vote(Vote),
}

pub struct VoteComponents<VS: VoteStorage> {
	pub individual_component:
		Option<(<VS as VoteStorage>::Properties, <VS as VoteStorage>::IndividualComponent)>,
	pub bitmap_component: Option<<VS as VoteStorage>::BitmapComponent>,
}

/// Describes a method of storing vote information.
///
/// Implementations of this trait should *NEVER* directly access the storage of the election pallet,
/// and only access it through the passed-in accessors.
pub trait VoteStorage: private::Sealed + Sized {
	/// A vote's properties. These are generated by the `ElectoralSystem` when a vote is first
	/// recorded. A `Vote` value is created/set by the validator who is voting, but the on-chain
	/// logic decides on a vote's `Properties`, NOT the validator. This is intended to allow
	/// implementation of vote timeouts, among other possibilities.
	type Properties: Parameter + Member;

	/// A validator's vote.
	type Vote: Parameter + Member;
	/// The validator's vote with all the shared data extracted. This should be a small
	/// representation of the `Vote` which can be used to determine if a given `Vote` is equivalent
	/// to the validator's original.
	type PartialVote: Parameter + Member;

	/// Instances of this type are stored in a storage map whose key is the validator's id. It is
	/// intended for situations where validators are likely to not provide matching vote data,
	/// thereby making the bitmap storage inefficient. Not this feature/type doesn't have to be
	/// used, and in which case no additional stores or loads will occur.
	type IndividualComponent: Parameter + Member;
	/// Unique instances of this type are stored alongside a bitmap of who used that value in their
	/// vote. Assuming most people use the same value, this allows us to significantly decrease the
	/// storage needed to store votes.
	type BitmapComponent: Parameter + Member;
	/// This type describes all the data which can be shared/de-duplicated between different
	/// validator's votes.
	type SharedData: Parameter + Member;

	fn vote_into_partial_vote<H: FnMut(Self::SharedData) -> SharedDataHash>(
		vote: &Self::Vote,
		h: H,
	) -> Self::PartialVote;
	fn partial_vote_into_components(
		properties: Self::Properties,
		partial_vote: Self::PartialVote,
	) -> Result<VoteComponents<Self>, CorruptStorageError>;

	/// Note: If all components are `None` this *MUST* always return `None`.
	#[allow(clippy::type_complexity)]
	fn components_into_authority_vote<
		GetSharedData: FnMut(SharedDataHash) -> Result<Option<Self::SharedData>, CorruptStorageError>,
	>(
		vote_components: VoteComponents<Self>,
		get_shared_data: GetSharedData,
	) -> Result<
		Option<(Self::Properties, AuthorityVote<Self::PartialVote, Self::Vote>)>,
		CorruptStorageError,
	>;

	fn visit_shared_data_in_vote<E, F: Fn(Self::SharedData) -> Result<(), E>>(
		vote: Self::Vote,
		f: F,
	) -> Result<(), E>;
	fn visit_shared_data_references_in_individual_component<F: Fn(SharedDataHash)>(
		individual_component: &Self::IndividualComponent,
		f: F,
	);
	fn visit_shared_data_references_in_bitmap_component<F: Fn(SharedDataHash)>(
		bitmap_component: &Self::BitmapComponent,
		f: F,
	);
}

mod private {
	/// Ensures `VoteStorage` can only be implemented here.
	pub trait Sealed {}
}