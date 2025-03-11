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

/// Stores each validator's vote individually, without de-duplicating identical values. This is
/// useful when a vote's encoding is close to the size of `SharedDataHash`'s or if the validator's
/// votes aren't likely to be equal.
pub struct Identity<T: Parameter + Member> {
	_phantom: core::marker::PhantomData<T>,
}
impl<T: Parameter + Member> IndividualVoteStorage for Identity<T> {
	type Vote = T;
	type PartialVote = T;

	// Cannot use `Infallible` here, as scale-codec doesn't implement the codec traits on it.
	type SharedData = ();

	fn vote_into_partial_vote<H: FnMut(Self::SharedData) -> SharedDataHash>(
		vote: &Self::Vote,
		_h: H,
	) -> Self::PartialVote {
		vote.clone()
	}
	fn partial_vote_into_vote<
		GetSharedData: FnMut(SharedDataHash) -> Result<Option<Self::SharedData>, CorruptStorageError>,
	>(
		partial_vote: &Self::PartialVote,
		_get_shared_data: GetSharedData,
	) -> Result<Option<Self::Vote>, CorruptStorageError> {
		Ok(Some(partial_vote.clone()))
	}

	fn visit_shared_data_in_vote<E, F: Fn(Self::SharedData) -> Result<(), E>>(
		_vote: Self::Vote,
		_f: F,
	) -> Result<(), E> {
		Ok(())
	}
	fn visit_shared_data_references_in_partial_vote<F: Fn(SharedDataHash)>(
		_partial_vote: &Self::PartialVote,
		_f: F,
	) {
	}
}
impl<T: Parameter + Member> super::private::Sealed for Identity<T> {}
