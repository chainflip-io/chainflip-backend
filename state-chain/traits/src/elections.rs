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

//! Shared pieces of election voting.
//!
//! `pallet-cf-elections` is instanced once per chain, so a validator that votes in every instance
//! separately re-does the same authorisation for each one, and pays a whole extrinsic's overhead
//! (signature verification, nonce, fee) every time. These live here rather than in the elections
//! pallet so that a runtime-level pallet can offer a single batched extrinsic without depending on
//! the elections pallet or naming any instance.

use crate::{AccountRoleRegistry, Chainflip, EpochInfo};
use cf_primitives::{AuthorityCount, EpochIndex};
use frame_support::{
	pallet_prelude::{DispatchError, Member},
	weights::Weight,
	Parameter,
};
use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};
use sp_std::prelude::*;

/// Everything recording a vote needs about the caller that does *not* depend on which elections
/// instance is being voted in.
///
/// Establishing this is the same work no matter how many instances the caller then votes in, so
/// [`authorise_voter`] does it once and each instance is handed the result.
pub struct VoterContext<T: Chainflip> {
	pub epoch_index: EpochIndex,
	pub authority: <T as Chainflip>::ValidatorId,
	pub authority_index: AuthorityCount,
	pub block_number: BlockNumberFor<T>,
}

/// Check that the origin is a validator, and gather the parts of [`VoterContext`] that are shared
/// across elections instances.
///
/// `None` when the caller is a validator but not in the current authority set - the caller turns
/// that into its own `Unauthorised` error, since this is not tied to an instance (or even to the
/// elections pallet) and so cannot name one.
///
/// Deliberately does *not* check whether an instance is accepting votes: that is the one check
/// that varies per instance, so it stays with the instance.
pub fn authorise_voter<T: Chainflip>(
	origin: OriginFor<T>,
) -> Result<Option<VoterContext<T>>, DispatchError> {
	let epoch_index = T::EpochInfo::epoch_index();
	let authority: <T as Chainflip>::ValidatorId =
		T::AccountRoleRegistry::ensure_validator(origin)?.into();
	Ok(T::EpochInfo::authority_index(epoch_index, &authority).map(|authority_index| VoterContext {
		epoch_index,
		authority,
		authority_index,
		// Constant for the whole extrinsic, so read it once rather than for every vote.
		// `block_number()` is what `BlockNumberProvider::current_block_number` resolves to for
		// `frame_system`, without needing that trait in scope here.
		block_number: frame_system::Pallet::<T>::block_number(),
	}))
}

/// The set of `pallet-cf-elections` instances a validator votes in, as one unit.
///
/// Implemented by the runtime, which is the only place that knows every instance. Lets a
/// runtime-level pallet expose one batched vote extrinsic without depending on the elections
/// pallet.
pub trait ElectionInstanceVoting<T: Chainflip> {
	/// Votes for each instance, each optional so a caller can target any subset.
	type Votes: Parameter + Member;

	/// The weight of recording `votes`, summed over the instances they actually target.
	fn weight(votes: &Self::Votes) -> Weight;

	/// Record `votes` in every instance they target.
	///
	/// Instances are independent: one failing must neither abort the rest nor roll back their
	/// storage, which is what a caller submitting a separate extrinsic per instance gets today.
	/// Returns the failures - paired with the instance's index in `Votes` - for the caller to
	/// report, rather than failing the whole call.
	fn vote_all(context: &VoterContext<T>, votes: Self::Votes) -> Vec<(u32, DispatchError)>;
}

/// No elections instances to vote in - for mock runtimes that do not include the elections
/// pallet. Accepts only the empty vote set.
impl<T: Chainflip> ElectionInstanceVoting<T> for () {
	type Votes = ();

	fn weight(_votes: &Self::Votes) -> Weight {
		Weight::zero()
	}

	fn vote_all(_context: &VoterContext<T>, _votes: Self::Votes) -> Vec<(u32, DispatchError)> {
		Vec::new()
	}
}