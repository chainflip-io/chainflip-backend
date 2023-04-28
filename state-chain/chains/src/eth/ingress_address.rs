use cf_primitives::{IntentId, ETHEREUM_ETH_ADDRESS};
use sp_runtime::traits::{Hash, Keccak256};
use sp_std::{mem::size_of, vec::Vec};

// From master branch of chainflip-eth-contracts
// @FIXME store on and retrieve from the chain
const DEPLOY_BYTECODE: [u8; 1202] = hex_literal::hex!(
	"
	60a060405234801561001057600080fd 5b506040516104b23803806104b28339
	8101604081905261002f91610189565b 336080526001600160a01b03811673ee
	eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee eeeeee036100b2576040516000903390
	47908381818185875af1925050503d80 60008114610099576040519150601f19
	603f3d011682016040523d82523d6000 602084013e61009e565b606091505b50
	509050806100ac57600080fd5b506101 83565b6040516370a0823160e01b8152
	3060048201526001600160a01b038216 9063a9059cbb90339083906370a08231
	90602401602060405180830381865afa 158015610100573d6000803e3d6000fd
	5b505050506040513d601f19601f8201 16820180604052508101906101249190
	6101b9565b6040516001600160e01b03 1960e085901b1681526001600160a01b
	03909216600483015260248201526044 01600060405180830381600087803b15
	801561016a57600080fd5b505af11580 1561017e573d6000803e3d6000fd5b50
	5050505b506101d2565b600060208284 03121561019b57600080fd5b81516001
	600160a01b03811681146101b2576000 80fd5b9392505050565b600060208284
	0312156101cb57600080fd5b50519190 50565b6080516102c66101ec60003960
	00606801526102c66000f3fe60806040 52600436106100225760003560e01c80
	63f109a0be1461002e57600080fd5b36 61002957005b600080fd5b3480156100
	3a57600080fd5b5061004e6100493660 0461023a565b610050565b005b3373ff
	ffffffffffffffffffffffffffffffff ffffff7f000000000000000000000000
	00000000000000000000000000000000 00000000161461009257600080fd5b73
	ffffffffffffffffffffffffffffffff ffffffff811673eeeeeeeeeeeeeeeeee
	eeeeeeeeeeeeeeeeeeeeee0361011c57 60405160009033904790838181818587
	5af1925050503d806000811461010557 6040519150601f19603f3d0116820160
	40523d82523d6000602084013e61010a 565b606091505b505090508061011857
	600080fd5b5050565b6040517f70a082 31000000000000000000000000000000
	00000000000000000000000000815230 600482015273ffffffffffffffffffff
	ffffffffffffffffffff82169063a905 9cbb90339083906370a0823190602401
	602060405180830381865afa15801561 0190573d6000803e3d6000fd5b505050
	506040513d601f19601f820116820180 604052508101906101b4919061027756
	5b6040517fffffffff00000000000000 00000000000000000000000000000000
	000000000060e085901b16815273ffff ffffffffffffffffffffffffffffffff
	ffff9092166004830152602482015260 4401600060405180830381600087803b
	15801561021f57600080fd5b505af115 8015610233573d6000803e3d6000fd5b
	5050505050565b600060208284031215 61024c57600080fd5b813573ffffffff
	ffffffffffffffffffffffffffffffff 8116811461027057600080fd5b939250
	5050565b600060208284031215610289 57600080fd5b505191905056fea26469
	70667358221220f20e2187ee3007a8c2 fdd8b26fd231816810d78f0080630c8d
	f66826371d35c064736f6c63430008130033"
);

// Always the same, this is a CREATE2 constant.
const PREFIX_BYTE: u8 = 0xff;

/// Derives the CREATE2 Ethereum address for a given asset, vault, and intent.
/// @param asset_id The asset in "CHAIN:ASSET" form e.g. "ETH:ETH" or "ETH:USDC"
/// @param vault_address The address of the Ethereum Vault
/// @param intent_id The numerical intent id
pub fn get_create_2_address(
	vault_address: [u8; 20],
	deploy_constructor_argument: Option<Vec<u8>>,
	intent_id: IntentId,
) -> [u8; 20] {
	let deploy_bytecode: &'static [u8] = &DEPLOY_BYTECODE;

	// We hash the concatenated deploy_bytecode and deploy_constructor_argument.
	// This hash is used in the later CREATE2 derivation.
	// see: https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/Deposit.sol.
	let deploy_transaction_bytes_hash = Keccak256::hash(
		&[
			deploy_bytecode,
			&deploy_constructor_argument.map_or(Default::default(), |mut token_addr| {
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
		get_create_2_address(VAULT_ADDRESS, Some(ETHEREUM_ETH_ADDRESS.to_vec()), 420696969),
		hex_literal::hex!("Edf07a740a5D2d06b73f36fd5cc155d4240EaEEA")
	);
	println!("Derivation worked for ETH:ETH! 🚀");
}

#[test]
fn test_eth_flip() {
	// Based on previously verified values.
	const VAULT_ADDRESS: [u8; 20] = hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512");
	const FLIP_ADDRESS: [u8; 20] = hex_literal::hex!("Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9");

	assert_eq!(
		get_create_2_address(VAULT_ADDRESS, Some(FLIP_ADDRESS.to_vec()), 42069),
		hex_literal::hex!("334AE5875C2ce967d82611cc0bfEDdf5316f2477")
	);
	println!("Derivation worked for ETH:FLIP! 🚀");
}
