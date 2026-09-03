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
/// that into its own `Unauthorised` error.
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
		block_number: frame_system::Pallet::<T>::block_number(),
	}))
}

/// The set of `pallet-cf-elections` instances a validator votes in, as one unit.
pub trait ElectionInstancesVoting<T: Chainflip> {
	/// Votes for each instance, each optional so a caller can target any subset.
	type Votes: Parameter + Member;

	/// The weight of [`authorise_voter`], which is instance-agnostic and so is paid once
	/// however many instances are voted in.
	fn authorise_voter_weight() -> Weight;

	/// The weight of [`Self::vote_all`], summed over the instances `votes` actually target.
	fn vote_all_weight(votes: &Self::Votes) -> Weight;

	/// Record `votes` in every instance they target.
	///
	/// Instances are independent: one failing must neither abort the rest nor roll back their
	/// storage, which is what a caller submitting a separate extrinsic per instance gets today.
	/// Returns the failures - each paired with an implementation-defined index identifying the
	/// instance - for the caller to report, rather than failing the whole call.
	fn vote_all(context: &VoterContext<T>, votes: Self::Votes) -> Vec<(u32, DispatchError)>;
}

/// No elections instances to vote in - for mock runtimes that do not include the elections
/// pallet.
impl<T: Chainflip> ElectionInstancesVoting<T> for () {
	type Votes = ();

	fn authorise_voter_weight() -> Weight {
		Weight::zero()
	}

	fn vote_all_weight(_votes: &Self::Votes) -> Weight {
		Weight::zero()
	}

	fn vote_all(_context: &VoterContext<T>, _votes: Self::Votes) -> Vec<(u32, DispatchError)> {
		Vec::new()
	}
}
