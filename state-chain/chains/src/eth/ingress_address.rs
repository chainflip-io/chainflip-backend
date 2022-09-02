use sp_runtime::traits::{Hash, Keccak256};
use sp_std::{mem::size_of, vec, vec::Vec};

use crate::assets::Asset;

// From master branch of chainflip-eth-contracts
// @FIXME store on and retrieve from the chain
const DEPLOY_BYTECODE_ETH: [u8; 20] = hex_literal::hex!("6080604052348015600f57600080fd5b5033fffe");

// Always the same, this is a CREATE2 constant.
const PREFIX_BYTE: u8 = 0xff;

/// Derives the CREATE2 Ethereum address for a given asset, vault, and intent.
/// @param asset_id The asset in "CHAIN:ASSET" form e.g. "ETH:ETH" or "ETH:USDC"
/// @param vault_address The address of the Ethereum Vault
/// @param intent_id The numerical intent id
pub fn get_create_2_address(asset: Asset, vault_address: [u8; 20], intent_id: u32) -> [u8; 20] {
	let deploy_bytecode = get_deploy_bytecode(asset);
	let constructor_argument_bytes = get_constructor_argument_bytes(asset);

	// We hash the concatenated deploy_bytecode and constructor_argument_bytes.
	// This hash is used in the later CREATE2 derivation.
	let deploy_transaction_bytes_hash =
		Keccak256::hash(&[deploy_bytecode, constructor_argument_bytes].concat());

	// Unique salt per intent.
	let salt = get_salt(intent_id).to_vec();

	let create_2_args = [
		[PREFIX_BYTE].to_vec(),
		vault_address.to_vec(),
		salt,
		deploy_transaction_bytes_hash.as_bytes().to_vec(),
	]
	.concat();

	Keccak256::hash(&create_2_args).to_fixed_bytes()[12..32].try_into().unwrap()
}

/// Returns the deploy bytecode for the given asset. Every ERC20 token shares
/// the same contract bytecode (but has differing constructor arguments, see
/// get_constructor_argument_bytes). ETH is not an ERC20 token, so the contract
/// bytecode is different.
fn get_deploy_bytecode(asset: Asset) -> Vec<u8> {
	match asset {
		Asset::EthEth => DEPLOY_BYTECODE_ETH.to_vec(),
	}
}

/// Returns the constructor argument bytes for the given asset. For the ETH
/// deposit contract, there are no constructor arguments. For the token
/// deposit contract, the constructor argument is the asset's address.
fn get_constructor_argument_bytes(asset: Asset) -> Vec<u8> {
	match asset {
		Asset::EthEth => vec![],
	}
}

/// Get the CREATE2 salt for a given intent_id, equivalent to the big-endian u32, left-padded to 32
/// bytes.
fn get_salt(intent_id: u32) -> [u8; 32] {
	let mut salt = [0u8; 32];
	let offset = 32 - size_of::<u32>();
	salt.get_mut(offset..).unwrap().copy_from_slice(&intent_id.to_be_bytes());
	salt
}

#[test]
fn test_eth_eth() {
	// @FIXME grab this from chain's storage instead
	const VAULT_ADDRESS: [u8; 20] = hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512");

	assert_eq!(
		get_create_2_address(Asset::EthEth, VAULT_ADDRESS, 420696969),
		hex_literal::hex!("9AF943257C1dF03EA3EeD0dFa7B5328A2E4033bb")
	);
	println!("Derivation worked for ETH:ETH! 🚀");
}
