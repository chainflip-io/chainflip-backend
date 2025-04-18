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

//! Key rotator to be used by the Validator pallet to control the rotation of multiple keys

use cf_chains::ChainCrypto;
use cf_traits::{AsyncResult, StartKeyActivationResult, VaultActivator};
use core::marker::PhantomData;
use sp_std::vec::Vec;

pub struct MultiVaultActivator<A, B> {
	_phantom: PhantomData<(A, B)>,
}

impl<A, B, C: ChainCrypto> VaultActivator<C> for MultiVaultActivator<A, B>
where
	A: VaultActivator<C>,
	B: VaultActivator<C, ValidatorId = A::ValidatorId>,
{
	type ValidatorId = A::ValidatorId;

	fn activate_key() {
		A::activate_key();
		B::activate_key();
	}

	/// Start all key rotations with the provided `candidates`.
	fn start_key_activation(
		new_key: C::AggKey,
		maybe_old_key: Option<C::AggKey>,
	) -> Vec<StartKeyActivationResult> {
		[
			A::start_key_activation(new_key, maybe_old_key),
			B::start_key_activation(new_key, maybe_old_key),
		]
		.concat()
	}

	fn status() -> AsyncResult<()> {
		let async_results = [A::status(), B::status()];

		// if any of the inner rotations are void, then the overall key rotation result is void.
		if async_results.iter().any(|item| matches!(item, AsyncResult::Void)) {
			return AsyncResult::Void
		}

		// We must wait until all of these are ready before we do any action
		if async_results.iter().all(|item| matches!(item, AsyncResult::Ready(..))) {
			AsyncResult::Ready(())
		} else {
			AsyncResult::Pending
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<()>) {
		A::set_status(outcome);
		B::set_status(outcome);
	}
}
