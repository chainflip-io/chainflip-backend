use super::sol_tx_building_blocks::{Instruction, Pubkey, COMPUTE_BUDGET_PROGRAM};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::vec;
use core::str::FromStr;

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
			Pubkey::from_str(COMPUTE_BUDGET_PROGRAM).unwrap(),
			&Self::RequestHeapFrame(bytes),
			vec![],
		)
	}

	/// Create a `ComputeBudgetInstruction::SetComputeUnitLimit` `Instruction`
	pub fn set_compute_unit_limit(units: u32) -> Instruction {
		Instruction::new_with_borsh(
			Pubkey::from_str(COMPUTE_BUDGET_PROGRAM).unwrap(),
			&Self::SetComputeUnitLimit(units),
			vec![],
		)
	}

	/// Create a `ComputeBudgetInstruction::SetComputeUnitPrice` `Instruction`
	pub fn set_compute_unit_price(micro_lamports: u64) -> Instruction {
		Instruction::new_with_borsh(
			Pubkey::from_str(COMPUTE_BUDGET_PROGRAM).unwrap(),
			&Self::SetComputeUnitPrice(micro_lamports),
			vec![],
		)
	}

	/// Create a `ComputeBudgetInstruction::SetLoadedAccountsDataSizeLimit` `Instruction`
	pub fn set_loaded_accounts_data_size_limit(bytes: u32) -> Instruction {
		Instruction::new_with_borsh(
			Pubkey::from_str(COMPUTE_BUDGET_PROGRAM).unwrap(),
			&Self::SetLoadedAccountsDataSizeLimit(bytes),
			vec![],
		)
	}
}
