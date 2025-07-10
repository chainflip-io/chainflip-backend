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

use crate::Pallet;
use cf_runtime_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;

mod channel_action_ccm_refund;
mod transaction_rejection_details_ccm_migration;

pub type PalletMigration<T, I> = (
	VersionedMigration<
		25,
		26,
		channel_action_ccm_refund::ChannelActionCcmRefund<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	VersionedMigration<
		26,
		27,
		transaction_rejection_details_ccm_migration::Migration<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<27, Pallet<T, I>>,
);
