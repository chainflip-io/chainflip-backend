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

//! Moves the `FlipToBeSentToGateway` accumulator from `cf-swapping` to `cf-flip`, alongside the
//! rest of the Flip supply accounting.

use crate::Runtime;
use frame_support::{sp_runtime::Saturating, traits::OnRuntimeUpgrade, weights::Weight};

#[cfg(feature = "try-runtime")]
use cf_primitives::AssetAmount;
#[cfg(feature = "try-runtime")]
use frame_support::ensure;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod old {
	use cf_primitives::AssetAmount;
	use frame_support::{pallet_prelude::ValueQuery, storage_alias};

	#[storage_alias]
	pub type FlipToBeSentToGateway = StorageValue<Swapping, AssetAmount, ValueQuery>;
}

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		let flip_to_be_sent_to_gateway = old::FlipToBeSentToGateway::take();

		// Accrued rather than overwritten so that re-running the migration is harmless.
		pallet_cf_flip::FlipToBeSentToGateway::<Runtime>::mutate(|total| {
			total.saturating_accrue(flip_to_be_sent_to_gateway)
		});

		log::info!("Relocated FlipToBeSentToGateway to cf-flip: {flip_to_be_sent_to_gateway}.");

		<Runtime as frame_system::Config>::DbWeight::get().reads_writes(2, 2)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		use codec::Encode;
		Ok((
			old::FlipToBeSentToGateway::get(),
			pallet_cf_flip::FlipToBeSentToGateway::<Runtime>::get(),
		)
			.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		use codec::Decode;
		let (old_to_gateway, new_to_gateway_before) =
			<(AssetAmount, AssetAmount)>::decode(&mut &state[..])
				.map_err(|_| TryRuntimeError::Other("failed to decode pre-upgrade accumulators"))?;

		ensure!(
			pallet_cf_flip::FlipToBeSentToGateway::<Runtime>::get() ==
				new_to_gateway_before.saturating_add(old_to_gateway),
			"FlipToBeSentToGateway was not preserved across the move"
		);
		ensure!(
			!old::FlipToBeSentToGateway::exists(),
			"old FlipToBeSentToGateway storage not cleared"
		);
		Ok(())
	}
}
