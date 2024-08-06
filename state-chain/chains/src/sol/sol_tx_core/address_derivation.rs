//! Provides a interface to deriving addresses for the purposes of DepositChannels
//! as well as deriving ATA for token operations (such as fetch or transfer).

use cf_primitives::ChannelId;
use sol_prim::PdaAndBump;

use crate::sol::{AddressDerivationError, DerivedAddressBuilder, SolAddress};
use sol_prim::consts::{ASSOCIATED_TOKEN_PROGRAM_ID, TOKEN_PROGRAM_ID};

/// Derive address for a given channel ID
pub fn derive_deposit_address(
	channel_id: ChannelId,
	vault_program: SolAddress,
) -> Result<PdaAndBump, AddressDerivationError> {
	let seed = channel_id.to_le_bytes();
	derive_address(seed, vault_program)
}

/// Derive a Associated Token Account (ATA) of a main target account.
pub fn derive_associated_token_account(
	target: SolAddress,
	mint_pubkey: SolAddress,
) -> Result<PdaAndBump, AddressDerivationError> {
	DerivedAddressBuilder::from_address(ASSOCIATED_TOKEN_PROGRAM_ID)?
		.chain_seed(target)?
		.chain_seed(TOKEN_PROGRAM_ID)?
		.chain_seed(mint_pubkey)?
		.finish()
}

/// Derive a fetch account from the vault program key and a deposit channel address used as a seed.
pub fn derive_fetch_account(
	deposit_channel_address: SolAddress,
	vault_program: SolAddress,
) -> Result<PdaAndBump, AddressDerivationError> {
	derive_address(deposit_channel_address, vault_program)
}

/// Derive an address from our Vault program key. Produces an Address and a bump.
fn derive_address(
	seed: impl AsRef<[u8]>,
	vault_program: SolAddress,
) -> Result<PdaAndBump, AddressDerivationError> {
	DerivedAddressBuilder::from_address(vault_program)?.chain_seed(seed)?.finish()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sol::sol_tx_core::sol_test_values;
	use std::str::FromStr;

	#[test]
	fn derive_associated_token_account_on_curve() {
		let wallet_address =
			SolAddress::from_str("HfasueN6RNPjSM6rKGH5dga6kS2oUF8siGH3m4MXPURp").unwrap();
		let mint_pubkey = sol_test_values::USDC_TOKEN_MINT_PUB_KEY;

		assert_eq!(
			derive_associated_token_account(wallet_address, mint_pubkey).unwrap(),
			PdaAndBump {
				address: SolAddress::from_str("BeRexE9vZSdQMNg65PAnhy3rRPUxF6oWsxyNegYxySZD")
					.unwrap(),
				bump: 253u8
			}
		);
	}

	#[test]
	fn derive_associated_token_account_off_curve() {
		let pda_address =
			SolAddress::from_str("9j17hjg8wR2uFxJAJDAFahwsgTCNx35sc5qXSxDmuuF6").unwrap();
		let mint_pubkey = sol_test_values::USDC_TOKEN_MINT_PUB_KEY;

		assert_eq!(
			derive_associated_token_account(pda_address, mint_pubkey).unwrap(),
			PdaAndBump {
				address: SolAddress::from_str("DUjCLckPi4g7QAwBEwuFL1whpgY6L9fxwXnqbWvS2pcW")
					.unwrap(),
				bump: 251u8
			}
		);
	}

	#[test]
	fn can_derive_address() {
		let channel_0_seed = 0u64.to_le_bytes();
		let channel_1_seed = 1u64.to_le_bytes();

		let vault_program = sol_test_values::VAULT_PROGRAM;

		assert_eq!(
			derive_address(channel_0_seed, vault_program).unwrap(),
			PdaAndBump {
				address: SolAddress::from_str("JDtAzKWKzQJCiHCfK4PU7qYuE4wChxuqfDqQhRbv6kwX")
					.unwrap(),
				bump: 254u8
			}
		);
		assert_eq!(
			derive_address(channel_1_seed, vault_program).unwrap(),
			PdaAndBump {
				address: SolAddress::from_str("32qRitYeor2v7Rb3M2iL8PHkoyqhcoCCqYuWCNKqstN7")
					.unwrap(),
				bump: 255u8
			}
		);

		assert_eq!(
			derive_address([11u8, 12u8, 13u8, 55u8], vault_program).unwrap(),
			PdaAndBump {
				address: SolAddress::from_str("XFmi41e1L9t732KoZdmzMSVige3SjjzsLzk1rW4rhwP")
					.unwrap(),
				bump: 255u8
			}
		);

		assert_eq!(
			derive_address([1], vault_program).unwrap(),
			PdaAndBump {
				address: SolAddress::from_str("5N72J9YQKpky5yFnrWWpFcBQsWpFMK4rW6b2Ue3YmYcu")
					.unwrap(),
				bump: 255u8
			}
		);

		assert_eq!(
			derive_address([1, 2], vault_program).unwrap(),
			PdaAndBump {
				address: SolAddress::from_str("6PkQHEp18NgEDS5ydkgivU4pzTV6sYmoEaHvbbv4un73")
					.unwrap(),
				bump: 255u8
			}
		);
	}

	#[test]
	fn can_derive_deposit_address_native() {
		let vault_program = sol_test_values::VAULT_PROGRAM;
		assert_eq!(
			derive_deposit_address(0u64, vault_program).unwrap(),
			PdaAndBump {
				address: SolAddress::from_str("JDtAzKWKzQJCiHCfK4PU7qYuE4wChxuqfDqQhRbv6kwX")
					.unwrap(),
				bump: 254u8
			},
		);

		assert_eq!(
			derive_deposit_address(1u64, vault_program).unwrap(),
			PdaAndBump {
				address: SolAddress::from_str("32qRitYeor2v7Rb3M2iL8PHkoyqhcoCCqYuWCNKqstN7")
					.unwrap(),
				bump: 255u8
			},
		);
	}
	#[test]
	fn can_derive_deposit_address_token() {
		let vault_program = sol_test_values::VAULT_PROGRAM;
		let token_mint_pubkey = sol_test_values::USDC_TOKEN_MINT_PUB_KEY;
		let derived_account_0 = derive_deposit_address(0u64, vault_program).unwrap();
		assert_eq!(
			derive_associated_token_account(derived_account_0.address, token_mint_pubkey).unwrap(),
			PdaAndBump {
				address: SolAddress::from_str("7QWupKVHBPUnJpuvdt7uJxXaNWKYpEUAHPG9Rb28aEXS")
					.unwrap(),
				bump: 254u8
			}
		);

		let derived_account_1 = derive_deposit_address(1u64, vault_program).unwrap();
		assert_eq!(
			derive_associated_token_account(derived_account_1.address, token_mint_pubkey).unwrap(),
			PdaAndBump {
				address: SolAddress::from_str("9roLwm8U86pj24Hwwzx71AF8axYnSc6U542Bdx5w7FUZ")
					.unwrap(),
				bump: 255u8
			}
		);
	}

	#[test]
	fn test_sol_derive_fetch_account() {
		let fetch_account = derive_fetch_account(
			SolAddress::from_str("HAMxiXdEJxiBHabZAUm8PSLvWQM2GHi5PArVZvUCeDab").unwrap(),
			SolAddress::from_str("8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf").unwrap(),
		)
		.unwrap()
		.address;
		assert_eq!(
			fetch_account,
			SolAddress::from_str("HGgUaHpsmZpB3pcYt8PE89imca6BQBRqYtbVQQqsso3o").unwrap()
		);
	}

	#[test]
	fn can_derive_fetch_account_native() {
		let vault_program = sol_test_values::VAULT_PROGRAM;
		let deposit_channel = derive_deposit_address(0u64, vault_program).unwrap().address;
		assert_eq!(
			derive_fetch_account(deposit_channel, vault_program).unwrap(),
			PdaAndBump {
				address: SolAddress::from_str("AS1fDXUeL6dYHKxvyMGyFoqrsN5zPcUsanPDqmvVvFUA")
					.unwrap(),
				bump: 255u8
			},
		);
	}
	#[test]
	fn can_derive_fetch_account_token() {
		let vault_program = sol_test_values::VAULT_PROGRAM;
		let token_mint_pubkey = sol_test_values::USDC_TOKEN_MINT_PUB_KEY;
		let deposit_channel = derive_deposit_address(0u64, vault_program).unwrap().address;
		let deposit_channel_ata =
			derive_associated_token_account(deposit_channel, token_mint_pubkey)
				.unwrap()
				.address;

		assert_eq!(
			derive_fetch_account(deposit_channel_ata, vault_program).unwrap(),
			PdaAndBump {
				address: SolAddress::from_str("FuNSXye89kBJQXp3rqkcz7oCUd5C5rVUDo7o5CRQ6T2o")
					.unwrap(),
				bump: 252u8
			}
		);
	}
}
