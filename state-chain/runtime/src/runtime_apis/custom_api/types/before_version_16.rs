use super::*;
#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Clone)]
pub struct VaultAddresses {
	pub ethereum: EncodedAddress,
	pub arbitrum: EncodedAddress,
	pub bitcoin: Vec<(AccountId32, EncodedAddress)>,
	pub sol_vault_program: EncodedAddress,
	pub sol_swap_endpoint_program_data_account: EncodedAddress,
	pub usdc_token_mint_pubkey: EncodedAddress,

	pub bitcoin_vault: Option<EncodedAddress>,
	pub solana_sol_vault: Option<EncodedAddress>,
	pub solana_usdc_token_vault_ata: EncodedAddress,
	pub solana_vault_swap_account: Option<EncodedAddress>,

	pub predicted_seconds_until_next_vault_rotation: u64,
}

impl From<VaultAddresses> for super::VaultAddresses {
	fn from(old: VaultAddresses) -> Self {
		Self {
			ethereum: old.ethereum,
			arbitrum: old.arbitrum,
			bitcoin: old.bitcoin,
			sol_vault_program: old.sol_vault_program,
			sol_swap_endpoint_program_data_account: old.sol_swap_endpoint_program_data_account,
			usdc_token_mint_pubkey: old.usdc_token_mint_pubkey,
			bitcoin_vault: old.bitcoin_vault,
			solana_sol_vault: old.solana_sol_vault,
			solana_usdc_token_vault_ata: old.solana_usdc_token_vault_ata,
			solana_vault_swap_account: old.solana_vault_swap_account,
			predicted_seconds_until_next_vault_rotation: old
				.predicted_seconds_until_next_vault_rotation,
			// Set usdt token pubkey and ata to null addresses
			usdt_token_mint_pubkey: EncodedAddress::Sol([0u8; 32]),
			solana_usdt_token_vault_ata: EncodedAddress::Sol([0u8; 32]),
		}
	}
}
