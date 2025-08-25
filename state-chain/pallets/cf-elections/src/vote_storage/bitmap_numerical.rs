// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use frame_support::{pallet_prelude::Member, Parameter};

use super::{AuthorityVote, VoteComponents, VoteStorage};

use crate::{CorruptStorageError, SharedDataHash};

/// This is to be used when T is smaller than an Hash(32bytes) or if we want to de-dup votes but the
/// span of possible values is big enough hence we don't want PartialVote to be an Hash to avoid not
/// reaching conensus because we don't have the corresponding full-vote
pub struct BitmapNoHash<T: Parameter + Member> {
	_phantom: core::marker::PhantomData<T>,
}
impl<T: Parameter + Member> VoteStorage for BitmapNoHash<T> {
	type Properties = ();

	type Vote = T;
	type PartialVote = T;

	type IndividualComponent = ();
	type BitmapComponent = T;
	type SharedData = ();

	fn vote_into_partial_vote<H: FnMut(Self::SharedData) -> SharedDataHash>(
		vote: &Self::Vote,
		mut _h: H,
	) -> Self::PartialVote {
		vote.clone()
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
		mut _get_shared_data: GetSharedData,
	) -> Result<
		Option<(Self::Properties, AuthorityVote<Self::PartialVote, Self::Vote>)>,
		CorruptStorageError,
	> {
		Ok(match vote_components {
			VoteComponents { bitmap_component: Some(partial_vote), individual_component: None } =>
				Some(((), AuthorityVote::Vote(partial_vote))),
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
impl<T: Parameter + Member> super::private::Sealed for BitmapNoHash<T> {}
