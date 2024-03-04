// use super::{
// 	vec, vec::Vec, AccountMeta, FromStr, Instruction, Pubkey, TOKEN_PROGRAM_ID,ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID
// };
// use serde::{Deserialize, Serialize};

// /// Instructions supported by the token program.
// #[derive(Clone, Debug, PartialEq)]
// pub enum TokenInstruction {
//     TransferChecked {
//         /// The amount of tokens to transfer.
//         amount: u64,
//         /// Expected number of base 10 digits to the right of the decimal place.
//         decimals: u8,
//     },
// }


// // https://docs.rs/spl-token/latest/spl_token/instruction/index.html
// impl TokenInstruction {
// 	pub fn transfer_checked(source: &Pubkey, mint: &Pubkey,destination: &Pubkey, owner: &Pubkey, amount: u64, decimals: u8) -> Instruction {
// 		let account_metas = vec![
// 			AccountMeta::new(*mint, false),
// 			AccountMeta::new_readonly(*source, false),
// 			AccountMeta::new(*destination, false),

// 		];
// 		Instruction::new_with_bincode(
// 			// program id of the system program
// 			Pubkey::from_str(TOKEN_PROGRAM_ID).unwrap(),
// 			&Self::TransferChecked { amount, decimals},
// 			account_metas,
// 		)
// 	}
// }

// // https://docs.rs/spl-associated-token-account/2.3.0/spl_associated_token_account/instruction/fn.create_associated_token_account_idempotent.html

// /// Instructions supported by the AssociatedTokenAccount program
// #[derive(Clone, Debug, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
// pub enum AssociatedTokenAccountInstruction {
//     CreateIdempotent,
// }

// // https://docs.rs/spl-associated-token-account/2.3.0/spl_associated_token_account/instruction/index.html

// impl AssociatedTokenAccountInstruction {
// 	pub fn transfer_checked(funding_address: &Pubkey,wallet_address: &Pubkey,token_mint_address: &Pubkey, associated_account_address: &Pubkey) -> Instruction {

// 		let account_metas = vec![
//             AccountMeta::new(*funding_address, true),
//             AccountMeta::new(*associated_account_address, false),
//             AccountMeta::new_readonly(*wallet_address, false),
//             AccountMeta::new_readonly(*token_mint_address, false),
//             AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID).unwrap(), false),
//             AccountMeta::new_readonly(Pubkey::from_str(TOKEN_PROGRAM_ID).unwrap(), false),

// 		];
// 		Instruction::new_with_bincode(
// 			// program id of the system program
// 			Pubkey::from_str(ASSOCIATED_TOKEN_PROGRAM_ID).unwrap(),
// 			&Self::CreateIdempotent,
// 			account_metas,
// 		)
// 	}
// }