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

pub mod blockchain;
pub mod composite;
pub mod exact_value;
pub mod liveness;
#[cfg(test)]
pub mod mock;
pub mod monotonic_change;
pub mod monotonic_median;
pub mod solana_vault_swap_accounts;
pub mod unsafe_median;

#[cfg(test)]
pub(crate) mod mocks;
#[cfg(test)]
pub(crate) mod tests;
