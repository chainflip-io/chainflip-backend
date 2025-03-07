// Copyright 2025 Chainflip Labs GmbH and Anza Maintainers <maintainers@anza.xyz>
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

use crate::{consts::COMPUTE_BUDGET_PROGRAM, Instruction};
use sp_std::vec;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Compute Budget Instructions
#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum ComputeBudgetInstruction {
	Unused, // deprecated variant, reserved value.
	/// Request a specific transaction-wide program heap region size in bytes.
	/// The value requested must be a multiple of 1024. This new heap region
	/// size applies to each program executed in the transaction, including all
	/// calls to CPIs.
	RequestHeapFrame(u32),
	/// Set a specific compute unit limit that the transaction is allowed to consume.
	SetComputeUnitLimit(u32),
	/// Set a compute unit price in "micro-lamports" to pay a higher transaction
	/// fee for higher transaction prioritization.
	SetComputeUnitPrice(u64),
	/// Set a specific transaction-wide account data size limit, in bytes, is allowed to load.
	SetLoadedAccountsDataSizeLimit(u32),
}

impl ComputeBudgetInstruction {
	/// Create a `ComputeBudgetInstruction::RequestHeapFrame` `Instruction`
	pub fn request_heap_frame(bytes: u32) -> Instruction {
		Instruction::new_with_borsh(
			COMPUTE_BUDGET_PROGRAM.into(),
			&Self::RequestHeapFrame(bytes),
			vec![],
		)
	}

	/// Create a `ComputeBudgetInstruction::SetComputeUnitLimit` `Instruction`
	pub fn set_compute_unit_limit(units: u32) -> Instruction {
		Instruction::new_with_borsh(
			COMPUTE_BUDGET_PROGRAM.into(),
			&Self::SetComputeUnitLimit(units),
			vec![],
		)
	}

	/// Create a `ComputeBudgetInstruction::SetComputeUnitPrice` `Instruction`
	pub fn set_compute_unit_price(micro_lamports: u64) -> Instruction {
		Instruction::new_with_borsh(
			COMPUTE_BUDGET_PROGRAM.into(),
			&Self::SetComputeUnitPrice(micro_lamports),
			vec![],
		)
	}

	/// Create a `ComputeBudgetInstruction::SetLoadedAccountsDataSizeLimit` `Instruction`
	pub fn set_loaded_accounts_data_size_limit(bytes: u32) -> Instruction {
		Instruction::new_with_borsh(
			COMPUTE_BUDGET_PROGRAM.into(),
			&Self::SetLoadedAccountsDataSizeLimit(bytes),
			vec![],
		)
	}
}
