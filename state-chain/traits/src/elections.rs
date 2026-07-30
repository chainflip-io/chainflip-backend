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
use frame_support::pallet_prelude::DispatchError;
use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};

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
