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

use crate::{Pallet, STORAGE_VERSION_U16};
use cf_runtime_utilities::PlaceholderMigration;

pub type PalletMigration<T> = (PlaceholderMigration<{ STORAGE_VERSION_U16 }, Pallet<T>>,);

#[cfg(test)]
const _: u16 =
	<PalletMigration<crate::mock::Test> as cf_runtime_utilities::MigrationSequence>::FROM;
