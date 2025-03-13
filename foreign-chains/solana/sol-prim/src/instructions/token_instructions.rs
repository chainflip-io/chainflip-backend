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

//! Program instructions
use crate::{
	consts::{ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID},
	AccountMeta, Instruction, Pubkey,
};
use borsh::{BorshDeserialize, BorshSerialize};
use sp_std::vec;

// https://docs.rs/spl-associated-token-account/2.3.0/src/spl_associated_token_account/instruction.rs.html#1-161

/// Instructions supported by the AssociatedTokenAccount program
#[derive(Clone, Debug, PartialEq, BorshDeserialize, BorshSerialize)]
pub enum AssociatedTokenAccountInstruction {
	/// Creates an associated token account for the given wallet address and
	/// token mint Returns an error if the account exists.
	///
	///   0. `[writeable,signer]` Funding account (must be a system account)
	///   1. `[writeable]` Associated token account address to be created
	///   2. `[]` Wallet address for the new associated token account
	///   3. `[]` The token mint for the new associated token account
	///   4. `[]` System program
	///   5. `[]` SPL Token program
	Create,
	/// Creates an associated token account for the given wallet address and
	/// token mint, if it doesn't already exist.  Returns an error if the
	/// account exists, but with a different owner.
	///
	///   0. `[writeable,signer]` Funding account (must be a system account)
	///   1. `[writeable]` Associated token account address to be created
	///   2. `[]` Wallet address for the new associated token account
	///   3. `[]` The token mint for the new associated token account
	///   4. `[]` System program
	///   5. `[]` SPL Token program
	CreateIdempotent,
	/// Transfers from and closes a nested associated token account: an
	/// associated token account owned by an associated token account.
	///
	/// The tokens are moved from the nested associated token account to the
	/// wallet's associated token account, and the nested account lamports are
	/// moved to the wallet.
	///
	/// Note: Nested token accounts are an anti-pattern, and almost always
	/// created unintentionally, so this instruction should only be used to
	/// recover from errors.
	///
	///   0. `[writeable]` Nested associated token account, must be owned by `3`
	///   1. `[]` Token mint for the nested associated token account
	///   2. `[writeable]` Wallet's associated token account
	///   3. `[]` Owner associated token account address, must be owned by `5`
	///   4. `[]` Token mint for the owner associated token account
	///   5. `[writeable, signer]` Wallet address for the owner associated token account
	///   6. `[]` SPL Token program
	RecoverNested,
}

impl AssociatedTokenAccountInstruction {
	/// Creates CreateIdempotent instruction
	/// Note that the associated account address is passed as a parameter instead of being
	/// derived in this function, which is the SDK implementation.
	pub fn create_associated_token_account_idempotent_instruction(
		funding_address: &Pubkey,
		wallet_address: &Pubkey,
		token_mint_address: &Pubkey,
		associated_account_address: &Pubkey,
	) -> Instruction {
		let account_metas = vec![
			AccountMeta::new(*funding_address, true),
			AccountMeta::new(*associated_account_address, false),
			AccountMeta::new_readonly(*wallet_address, false),
			AccountMeta::new_readonly(*token_mint_address, false),
			AccountMeta::new_readonly(SYSTEM_PROGRAM_ID.into(), false),
			AccountMeta::new_readonly(TOKEN_PROGRAM_ID.into(), false),
		];
		Instruction::new_with_borsh(
			// program id of the system program
			ASSOCIATED_TOKEN_PROGRAM_ID.into(),
			&Self::CreateIdempotent,
			account_metas,
		)
	}
}
