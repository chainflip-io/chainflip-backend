use cf_primitives::ChannelId;
use sp_runtime::traits::{Hash, Keccak256};
use sp_std::{mem::size_of, vec::Vec};

// From master branch of chainflip-eth-contracts
// @FIXME store on and retrieve from the chain
const DEPLOY_BYTECODE: [u8; 1112] = hex_literal::hex!(
	"
   60a060405234801561001057600080fd5 b5060405161045838038061045883398
   101604081905261002f91610189565b33 6080526001600160a01b03811673eeee
   eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee eee036100b2576040516000903390479
   08381818185875af1925050503d806000 8114610099576040519150601f19603f
   3d011682016040523d82523d600060208 4013e61009e565b606091505b5050905
   0806100ac57600080fd5b50610183565b 6040516370a0823160e01b8152306004
   8201526001600160a01b0382169063a90 59cbb90339083906370a082319060240
   1602060405180830381865afa15801561 0100573d6000803e3d6000fd5b505050
   506040513d601f19601f8201168201806 040525081019061012491906101b9565
   b6040516001600160e01b031960e08590 1b1681526001600160a01b0390921660
   048301526024820152604401600060405 180830381600087803b15801561016a5
   7600080fd5b505af115801561017e573d 6000803e3d6000fd5b505050505b5061
   01d2565b60006020828403121561019b5 7600080fd5b81516001600160a01b038
   11681146101b257600080fd5b93925050 50565b6000602082840312156101cb57
   600080fd5b5051919050565b608051610 26c6101ec6000396000605b015261026
   c6000f3fe608060405260043610610022 5760003560e01c8063f109a0be146100
   2e57600080fd5b3661002957005b60008 0fd5b34801561003a57600080fd5b506
   1004e6100493660046101ed565b610050 565b005b336001600160a01b037f0000
   000000000000000000000000000000000 00000000000000000000000000016146
   1008557600080fd5b6001600160a01b03 811673eeeeeeeeeeeeeeeeeeeeeeeeee
   eeeeeeeeeeeeee0361010257604051600 090339047908381818185875af192505
   0503d80600081146100eb576040519150 601f19603f3d011682016040523d8252
   3d6000602084013e6100f0565b6060915 05b50509050806100fe57600080fd5b5
   050565b6040516370a0823160e01b8152 3060048201526001600160a01b038216
   9063a9059cbb90339083906370a082319 0602401602060405180830381865afa1
   58015610150573d6000803e3d6000fd5b 505050506040513d601f19601f820116
   820180604052508101906101749190610 21d565b6040517fffffffff000000000
   000000000000000000000000000000000 0000000000000060e085901b16815260
   01600160a01b039092166004830152602 48201526044016000604051808303816
   00087803b1580156101d257600080fd5b 505af11580156101e6573d6000803e3d
   6000fd5b5050505050565b60006020828 40312156101ff57600080fd5b8135600
   1600160a01b0381168114610216576000 80fd5b9392505050565b600060208284
   03121561022f57600080fd5b505191905 056fea264697066735822122043c07ba
   6dfc49c343f65b75b6ce67a44707058a1 293a71305e6c9e4172aff15364736f6c
   63430008130033"
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
			DEPLOY_BYTECODE,
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
			hex_literal::hex!("8e4f261Ec4e75B0a5B980fCB09a573BabbaD46d9")
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
			hex_literal::hex!("32b0685C0B3604113E3390dC7c0d3d100BF8d255")
		);
		println!("Derivation worked for ETH:FLIP! 🚀");
	}
}
