//! Program instructions
use super::{AccountMeta, Instruction, Pubkey};
use crate::vec;
use borsh::{BorshDeserialize, BorshSerialize};

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
			AccountMeta::new_readonly(sol_prim::consts::SYSTEM_PROGRAM_ID.into(), false),
			AccountMeta::new_readonly(sol_prim::consts::TOKEN_PROGRAM_ID.into(), false),
		];
		Instruction::new_with_borsh(
			// program id of the system program
			sol_prim::consts::ASSOCIATED_TOKEN_PROGRAM_ID.into(),
			&Self::CreateIdempotent,
			account_metas,
		)
	}
}