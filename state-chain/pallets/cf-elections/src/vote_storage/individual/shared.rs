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
