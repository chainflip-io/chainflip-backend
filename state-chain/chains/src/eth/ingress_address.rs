use cf_primitives::{chains::assets::eth, IntentId};
use sp_runtime::traits::{Hash, Keccak256};
use sp_std::{mem::size_of, vec::Vec};

// From master branch of chainflip-eth-contracts
// @FIXME store on and retrieve from the chain
const DEPLOY_BYTECODE_ETH: [u8; 20] = hex_literal::hex!("6080604052348015600f57600080fd5b5033fffe");
const DEPLOY_BYTECODE_TOKEN: [u8; 493] = hex_literal::hex!(
	"
    608060405234801561001057600080fd 5b506040516101ed3803806101ed8339
    8101604081905261002f916101aa565b 6040516370a0823160e01b8152306004
    8201526001600160a01b0382169063a9 059cbb90339083906370a08231906024
    0160206040518083038186803b158015 61007857600080fd5b505afa15801561
    008c573d6000803e3d6000fd5b505050 506040513d601f19601f820116820180
    604052508101906100b091906101d356 5b6040516001600160e01b031960e085
    901b1681526001600160a01b03909216 60048301526024820152604401602060
    405180830381600087803b1580156100 f657600080fd5b505af115801561010a
    573d6000803e3d6000fd5b5050505060 40513d601f19601f8201168201806040
    525081019061012e9190610181565b61 017e5760405162461bcd60e51b815260
    206004820152601d60248201527f4465 706f736974546f6b656e3a207472616e
    73666572206661696c65640000006044 82015260640160405180910390fd5b33
    ff5b6000602082840312156101935760 0080fd5b815180151581146101a35760
    0080fd5b9392505050565b6000602082 840312156101bc57600080fd5b815160
    01600160a01b03811681146101a35760 0080fd5b6000602082840312156101e5
    57600080fd5b505191905056fe"
);

// Always the same, this is a CREATE2 constant.
const PREFIX_BYTE: u8 = 0xff;

/// Derives the CREATE2 Ethereum address for a given asset, vault, and intent.
/// @param asset_id The asset in "CHAIN:ASSET" form e.g. "ETH:ETH" or "ETH:USDC"
/// @param vault_address The address of the Ethereum Vault
/// @param intent_id The numerical intent id
pub fn get_create_2_address(
	asset: eth::Asset,
	vault_address: [u8; 20],
	erc20_constructor_argument: Option<Vec<u8>>,
	intent_id: IntentId,
) -> [u8; 20] {
	let deploy_bytecode = get_deploy_bytecode(asset);

	// We hash the concatenated deploy_bytecode and erc20_constructor_argument.
	// This hash is used in the later CREATE2 derivation.
	// Note: For native ETH we don't need to add extra bytes because the constructor is empty
	// see: https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/DepositEth.sol.
	let deploy_transaction_bytes_hash = Keccak256::hash(
		&[
			deploy_bytecode,
			&erc20_constructor_argument.map_or(Default::default(), |mut token_addr| {
				token_addr.splice(0..0, [0u8; 12]);
				token_addr
			}),
		]
		.concat(),
	);

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
fn get_deploy_bytecode(asset: eth::Asset) -> &'static [u8] {
	match asset {
		eth::Asset::Eth => &DEPLOY_BYTECODE_ETH,
		eth::Asset::Flip | eth::Asset::Usdc => &DEPLOY_BYTECODE_TOKEN,
	}
}

/// Get the CREATE2 salt for a given intent_id, equivalent to the big-endian u32, left-padded to 32
/// bytes.
pub fn get_salt(intent_id: IntentId) -> [u8; 32] {
	let mut salt = [0u8; 32];
	let offset = 32 - size_of::<IntentId>();
	salt.get_mut(offset..).unwrap().copy_from_slice(&intent_id.to_be_bytes());
	salt
}

#[test]
fn test_eth_eth() {
	// Based on previously verified values.
	const VAULT_ADDRESS: [u8; 20] = hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512");

	assert_eq!(
		get_create_2_address(eth::Asset::Eth, VAULT_ADDRESS, None, 420696969),
		hex_literal::hex!("9AF943257C1dF03EA3EeD0dFa7B5328A2E4033bb")
	);
	println!("Derivation worked for ETH:ETH! 🚀");
}

#[test]
fn test_eth_flip() {
	// Based on previously verified values.
	const VAULT_ADDRESS: [u8; 20] = hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512");
	const FLIP_ADDRESS: [u8; 20] = hex_literal::hex!("Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9");

	assert_eq!(
		get_create_2_address(eth::Asset::Flip, VAULT_ADDRESS, Some(FLIP_ADDRESS.to_vec()), 42069),
		hex_literal::hex!("E93Ee798dE2dea25a8E8c49EE6e39e1c006b9188")
	);
	println!("Derivation worked for ETH:FLIP! 🚀");
}
