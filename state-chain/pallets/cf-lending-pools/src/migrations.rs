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

use cf_runtime_utilities::PlaceholderMigration;
use frame_support::migrations::VersionedMigration;

use crate::Pallet;

mod add_min_lending_pool_share;
mod boost_refactor_migration;
mod lending_config_migration;
mod loan_account_migration;

pub type PalletMigration<T> = (
	VersionedMigration<
		2,
		3,
		boost_refactor_migration::Migration<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>,
	VersionedMigration<
		3,
		4,
		add_min_lending_pool_share::Migration<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>,
	VersionedMigration<
		4,
		5,
		loan_account_migration::Migration<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>,
	VersionedMigration<
		5,
		6,
		lending_config_migration::Migration<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>,
	PlaceholderMigration<6, Pallet<T>>,
);
