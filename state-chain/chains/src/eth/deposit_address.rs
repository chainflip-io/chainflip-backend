use cf_primitives::ChannelId;
use sp_runtime::traits::{Hash, Keccak256};
use sp_std::{mem::size_of, vec::Vec};

// From master branch of chainflip-eth-contracts
// @FIXME store on and retrieve from the chain
const DEPOSIT_CONTRACT_BYTECODE: [u8; 1217] = hex_literal::hex!(
	"
	60a060405234801561001057600080fd 5b506040516104c13803806104c18339
	8101604081905261002f916101bc565b 336080526001600160a01b03811673ee
	eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee eeeeee036100e5576040514781527f49
	28e6f1957db520f45d8e69da428456a8 006acc572adf44f643009a27e1bf1e90
	60200160405180910390a16040516000 90339047908381818185875af1925050
	503d80600081146100cc576040519150 601f19603f3d011682016040523d8252
	3d6000602084013e6100d1565b606091 505b50509050806100df57600080fd5b
	506101b6565b6040516370a0823160e0 1b81523060048201526001600160a01b
	0382169063a9059cbb90339083906370 a0823190602401602060405180830381
	865afa158015610133573d6000803e3d 6000fd5b505050506040513d601f1960
	1f820116820180604052508101906101 5791906101ec565b6040516001600160
	e01b031960e085901b16815260016001 60a01b03909216600483015260248201
	52604401600060405180830381600087 803b15801561019d57600080fd5b505a
	f11580156101b1573d6000803e3d6000 fd5b505050505b50610205565b600060
	2082840312156101ce57600080fd5b81 516001600160a01b03811681146101e5
	57600080fd5b9392505050565b600060 2082840312156101fe57600080fd5b50
	51919050565b60805161029b61022660 003960008181605e0152610107015261
	029b6000f3fe60806040526004361061 00225760003560e01c8063f109a0be14
	6100e157600080fd5b366100dc576040 514781527f4928e6f1957db520f45d8e
	69da428456a8006acc572adf44f64300 9a27e1bf1e9060200160405180910390
	a160007f000000000000000000000000 00000000000000000000000000000000
	000000006001600160a01b0316476040 5160006040518083038185875af19250
	50503d80600081146100c75760405191 50601f19603f3d011682016040523d82
	523d6000602084013e6100cc565b6060 91505b50509050806100da57600080fd
	5b005b600080fd5b3480156100ed5760 0080fd5b506100da6100fc3660046102
	1c565b336001600160a01b037f000000 00000000000000000000000000000000
	00000000000000000000000000161461 013157600080fd5b6040516370a08231
	60e01b81523060048201526001600160 a01b0382169063a9059cbb9033908390
	6370a082319060240160206040518083 0381865afa15801561017f573d600080
	3e3d6000fd5b505050506040513d601f 19601f82011682018060405250810190
	6101a3919061024c565b6040517fffff ffff0000000000000000000000000000
	000000000000000000000000000060e0 85901b1681526001600160a01b039092
	16600483015260248201526044016000 60405180830381600087803b15801561
	020157600080fd5b505af11580156102 15573d6000803e3d6000fd5b50505050
	50565b60006020828403121561022e57 600080fd5b81356001600160a01b0381
	16811461024557600080fd5b93925050 50565b60006020828403121561025e57
	600080fd5b505191905056fea2646970 6673582212204f98a98e4619158c80c0
	3650a6be93587b13a58388353a073663 514a65af1aee64736f6c634300081400
	33
	"
);

// Always the same, this is a CREATE2 constant.
const PREFIX_BYTE: u8 = 0xff;

/// Derives the CREATE2 Ethereum address for a given asset, vault, and channel id.
/// @param vault_address The address of the Ethereum Vault
/// @param token_address The token address if this is a token deposit
/// @param channel_id The numerical channel id
pub fn get_create_2_address(
	vault_address: [u8; 20],
	token_address: Option<[u8; 20]>,
	channel_id: ChannelId,
) -> [u8; 20] {
	// This hash is used in the later CREATE2 derivation.
	// see: https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/Deposit.sol.
	let deploy_transaction_bytes_hash = Keccak256::hash(
		itertools::chain!(
			DEPOSIT_CONTRACT_BYTECODE,
			[0u8; 12], // padding
			token_address.unwrap_or(cf_primitives::ETHEREUM_ETH_ADDRESS),
		)
		.collect::<Vec<_>>()
		.as_slice(),
	);

	let create_2_args = itertools::chain!(
		[PREFIX_BYTE],
		vault_address,
		get_salt(channel_id),
		deploy_transaction_bytes_hash.to_fixed_bytes()
	)
	.collect::<Vec<_>>();

	Keccak256::hash(&create_2_args).to_fixed_bytes()[12..].try_into().unwrap()
}

/// Get the CREATE2 salt for a given channel_id, equivalent to the big-endian u32, left-padded to 32
/// bytes.
pub fn get_salt(channel_id: ChannelId) -> [u8; 32] {
	let mut salt = [0u8; 32];
	let offset = 32 - size_of::<ChannelId>();
	salt.get_mut(offset..).unwrap().copy_from_slice(&channel_id.to_be_bytes());
	salt
}

#[cfg(test)]
mod test_super {
	use super::*;
	// Based on previously verified values.
	const VAULT_ADDRESS: [u8; 20] = hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512");
	const FLIP_ADDRESS: [u8; 20] = hex_literal::hex!("Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9");

	#[test]
	fn test_eth_eth() {
		assert_eq!(
			get_create_2_address(VAULT_ADDRESS, None, 420696969),
			hex_literal::hex!("26D71cEa73eEEEcA04a2b05Ca121fc04A0b3107b")
		);

		println!("Derivation worked for ETH:ETH! 🚀");
	}

	#[test]
	fn test_eth_flip() {
		// Based on previously verified values.
		const VAULT_ADDRESS: [u8; 20] =
			hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512");

		assert_eq!(
			get_create_2_address(VAULT_ADDRESS, Some(FLIP_ADDRESS), 42069),
			hex_literal::hex!("f29742a3904a7E0cD8E4B22972C60e1cA99d6eE0")
		);
		println!("Derivation worked for ETH:FLIP! 🚀");
	}

	#[test]
	fn assert_bytecode_matches() {
		let expected_bytecode_hex = include_str!(concat!(
			env!("CF_ETH_CONTRACT_ABI_ROOT"),
			"/",
			env!("CF_ETH_CONTRACT_ABI_TAG"),
			"/Deposit_bytecode.json",
		))
		.trim()
		.trim_matches('"');
		println!("CF_ETH_CONTRACT_ABI_TAG: {}", env!("CF_ETH_CONTRACT_ABI_TAG"));
		assert_eq!(
			DEPOSIT_CONTRACT_BYTECODE,
			hex::decode(expected_bytecode_hex).unwrap().as_slice(),
			"Expected: {expected_bytecode_hex:?}, Actual: {:?}",
			hex::encode(DEPOSIT_CONTRACT_BYTECODE.as_slice()),
		);
	}
}
