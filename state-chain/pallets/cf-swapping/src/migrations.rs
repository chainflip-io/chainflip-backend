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

pub mod network_fee_migration;
pub mod swap_request_migration;
pub mod swap_request_ccm_refund_migration;

pub type PalletMigration<T> = (
	// network_fee_migration must go before swap_request_migration
	VersionedMigration<
		9,
		10,
		network_fee_migration::Migration<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>,
	VersionedMigration<
		9,
		10,
		swap_request_migration::Migration<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>,
	VersionedMigration<
		10,
		11,
		swap_request_ccm_refund_migration::Migration<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<11, Pallet<T>>,
);
