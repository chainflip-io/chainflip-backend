//! Contains code for Deriving addresses and deposit channels for the Solana Environment.
use cf_primitives::{chains::assets::sol::Asset as SolAsset, ChannelId};
use sol_prim::AccountBump;

use crate::{
	deposit_channel::DepositChannel,
	sol::{consts::*, AddressDerivationError, DerivedAddressBuilder, SolAddress},
	Solana,
};
use core::str::FromStr;

/// Derive address/deposit channels for a channel ID
pub fn derive_deposit_channel(
	channel_id: ChannelId,
	_asset: SolAsset,
	vault_program: SolAddress,
) -> Result<DepositChannel<Solana>, AddressDerivationError> {
	let seed = channel_id.to_le_bytes();
	derive_address(seed, vault_program)
		// .and_then(|(address, bump)| match asset {
		// 	SolAsset::Sol => Ok((address, bump)),
		// 	SolAsset::Usd => {
		// 	    let mint_pubkey =
		// 	Env::lookup_account(crate::sol::api::SolanaEnvAccountLookupKey::TokenMintPubkey)?;
		// 	    derive_associated_token_account(address, mint_pubkey)
		// 	},
		// })
		.map(|(address, bump)| DepositChannel::<Solana> {
			channel_id,
			address,
			asset: SolAsset::Sol,
			state: bump,
		})
}

/// Derive an address from our Vault program key. Produces an Address and a bump.
fn derive_address(
	seed: impl AsRef<[u8]>,
	vault_program: SolAddress,
) -> Result<(SolAddress, AccountBump), AddressDerivationError> {
	DerivedAddressBuilder::from_address(vault_program)?.chain_seed(seed)?.finish()
}

#[allow(dead_code)]
/// Derive a Associated Token Account (ATA) of a main target account.
fn derive_associated_token_account(
	target: SolAddress,
	mint_pubkey: SolAddress,
) -> Result<(SolAddress, AccountBump), AddressDerivationError> {
	let associated_token_program_id = SolAddress::from_str(ASSOCIATED_TOKEN_PROGRAM_ID)
		.expect("Associated token program ID must be valid");
	let token_program_id =
		SolAddress::from_str(TOKEN_PROGRAM_ID).expect("Token program ID must be valid");

	DerivedAddressBuilder::from_address(associated_token_program_id)?
		.chain_seed(target)?
		.chain_seed(token_program_id)?
		.chain_seed(mint_pubkey)?
		.finish()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sol::sol_tx_core::sol_test_values;

	#[test]
	fn derive_associated_token_account_on_curve() {
		let wallet_address =
			SolAddress::from_str("HfasueN6RNPjSM6rKGH5dga6kS2oUF8siGH3m4MXPURp").unwrap();
		let mint_pubkey = SolAddress::from_str(sol_test_values::MINT_PUB_KEY).unwrap();

		assert_eq!(
			derive_associated_token_account(wallet_address, mint_pubkey).unwrap(),
			(SolAddress::from_str("BeRexE9vZSdQMNg65PAnhy3rRPUxF6oWsxyNegYxySZD").unwrap(), 253u8)
		);
	}

	#[test]
	fn derive_associated_token_account_off_curve() {
		let pda_address =
			SolAddress::from_str("9j17hjg8wR2uFxJAJDAFahwsgTCNx35sc5qXSxDmuuF6").unwrap();
		let mint_pubkey = SolAddress::from_str(sol_test_values::MINT_PUB_KEY).unwrap();

		assert_eq!(
			derive_associated_token_account(pda_address, mint_pubkey).unwrap(),
			(SolAddress::from_str("DUjCLckPi4g7QAwBEwuFL1whpgY6L9fxwXnqbWvS2pcW").unwrap(), 251u8)
		);
	}

	#[test]
	fn can_derive_address() {
		let channel_0_seed = 0u64.to_le_bytes();
		let channel_1_seed = 1u64.to_le_bytes();

		let vault_program = SolAddress::from_str(sol_test_values::VAULT_PROGRAM).unwrap();

		assert_eq!(
			derive_address(channel_0_seed, vault_program).unwrap(),
			(SolAddress::from_str("JDtAzKWKzQJCiHCfK4PU7qYuE4wChxuqfDqQhRbv6kwX").unwrap(), 254u8)
		);
		assert_eq!(
			derive_address(channel_1_seed, vault_program).unwrap(),
			(SolAddress::from_str("32qRitYeor2v7Rb3M2iL8PHkoyqhcoCCqYuWCNKqstN7").unwrap(), 255u8)
		);

		assert_eq!(
			derive_address([11u8, 12u8, 13u8, 55u8], vault_program).unwrap(),
			(SolAddress::from_str("XFmi41e1L9t732KoZdmzMSVige3SjjzsLzk1rW4rhwP").unwrap(), 255u8)
		);

		assert_eq!(
			derive_address([1], vault_program).unwrap(),
			(SolAddress::from_str("5N72J9YQKpky5yFnrWWpFcBQsWpFMK4rW6b2Ue3YmYcu").unwrap(), 255u8)
		);

		assert_eq!(
			derive_address([1, 2], vault_program).unwrap(),
			(SolAddress::from_str("6PkQHEp18NgEDS5ydkgivU4pzTV6sYmoEaHvbbv4un73").unwrap(), 255u8)
		);
	}

	#[test]
	fn can_derive_deposit_channel_for_sol() {
		let vault_program = SolAddress::from_str(sol_test_values::VAULT_PROGRAM).unwrap();

		assert_eq!(
			derive_deposit_channel(0u64, SolAsset::Sol, vault_program).unwrap(),
			DepositChannel {
				channel_id: 0u64,
				address: SolAddress::from_str("JDtAzKWKzQJCiHCfK4PU7qYuE4wChxuqfDqQhRbv6kwX")
					.unwrap(),
				asset: SolAsset::Sol,
				state: 254u8,
			},
		);

		assert_eq!(
			derive_deposit_channel(1u64, SolAsset::Sol, vault_program).unwrap(),
			DepositChannel {
				channel_id: 1u64,
				address: SolAddress::from_str("32qRitYeor2v7Rb3M2iL8PHkoyqhcoCCqYuWCNKqstN7")
					.unwrap(),
				asset: SolAsset::Sol,
				state: 255u8,
			},
		);
	}
}
