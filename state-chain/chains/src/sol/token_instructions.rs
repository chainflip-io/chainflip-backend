//! Program instructions
use super::sol_tx_building_blocks::{
	AccountMeta, Instruction, Pubkey, ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID,
	TOKEN_PROGRAM_ID,
};
use crate::vec;
use borsh::{BorshDeserialize, BorshSerialize};
use core::str::FromStr;

// https://docs.rs/spl-associated-token-account/2.3.0/src/spl_associated_token_account/instruction.rs.html#1-161

/// Instructions supported by the AssociatedTokenAccount program
#[derive(Clone, Debug, PartialEq, BorshDeserialize, BorshSerialize /* BorshSchema */)] // TODO: Is this right?
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
	pub fn create_associated_token_account_idempotent_instruction(
		funding_address: &Pubkey,
		wallet_address: &Pubkey,
		token_mint_address: &Pubkey,
		associated_account_address: &Pubkey,
	) -> Instruction {
		// TODO: Implement this. This ends up being a a call to Pubkey::find_program_address
		// (we can use try_find_program_address instead). In order to be able to do
		// any token transfer we must have the ATA's. If we only do ATAs in the protocol
		// then we just can't create associated token account idempotent which is certainly
		// problematic. Therefore, we need to use owner addresses in the protocol and derive
		// the ATA's from the owner addresses. In that scenario, we must do the derivation
		// in the state chain since transactions such as token transfers require to have
		// the ATA. I think there is no way to push this to the Engine. And then we either
		// pass both the owner and ata to the engine or just the latter.

		// let associated_account_address = get_associated_token_address_with_program_id(
		//     wallet_address,
		//     token_mint_address,
		//     token_program_id,
		// );
		let account_metas = vec![
			AccountMeta::new(*funding_address, true),
			AccountMeta::new(*associated_account_address, false),
			AccountMeta::new_readonly(*wallet_address, false),
			AccountMeta::new_readonly(*token_mint_address, false),
			AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID).unwrap(), false),
			AccountMeta::new_readonly(Pubkey::from_str(TOKEN_PROGRAM_ID).unwrap(), false),
		];
		Instruction::new_with_borsh(
			// program id of the system program
			Pubkey::from_str(ASSOCIATED_TOKEN_PROGRAM_ID).unwrap(),
			&Self::CreateIdempotent,
			account_metas,
		)
	}
}
