use cf_primitives::ChannelId;
use sp_runtime::traits::{Hash, Keccak256};
use sp_std::{mem::size_of, vec::Vec};

// From master branch of chainflip-eth-contracts
// @FIXME store on and retrieve from the chain
const DEPOSIT_CONTRACT_BYTECODE: [u8; 1071] = hex_literal::hex!(
	"
	60a060405234801561000f575f80fd5b 5060405161042f38038061042f833981
	01604081905261002e9161017d565b33 6080526001600160a01b03811673eeee
	eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee eeee036100ad576040515f9033904790
	8381818185875af1925050503d805f81 14610095576040519150601f19603f3d
	011682016040523d82523d5f60208401 3e61009a565b606091505b5050905080
	6100a7575f80fd5b50610177565b6040 516370a0823160e01b81523060048201
	526001600160a01b0382169063a9059c bb90339083906370a082319060240160
	2060405180830381865afa1580156100 f9573d5f803e3d5ffd5b505050506040
	513d601f19601f820116820180604052 5081019061011d91906101aa565b6040
	516001600160e01b031960e085901b16 81526001600160a01b03909216600483
	015260248201526044015f6040518083 03815f87803b158015610160575f80fd
	5b505af1158015610172573d5f803e3d 5ffd5b505050505b506101c1565b5f60
	20828403121561018d575f80fd5b8151 6001600160a01b03811681146101a357
	5f80fd5b9392505050565b5f60208284 0312156101ba575f80fd5b5051919050
	565b6080516102576101d85f395f6057 01526102575ff3fe6080604052600436
	10610020575f3560e01c8063f109a0be 1461002b575f80fd5b3661002757005b
	5f80fd5b348015610036575f80fd5b50 61004a6100453660046101dd565b6100
	4c565b005b336001600160a01b037f00 00000000000000000000000000000000
	00000000000000000000000000000016 14610080575f80fd5b6001600160a01b
	03811673eeeeeeeeeeeeeeeeeeeeeeee eeeeeeeeeeeeeeee036100f957604051
	5f90339047908381818185875af19250 50503d805f81146100e3576040519150
	601f19603f3d011682016040523d8252 3d5f602084013e6100e8565b60609150
	5b50509050806100f5575f80fd5b5050 565b6040516370a0823160e01b815230
	60048201526001600160a01b03821690 63a9059cbb90339083906370a0823190
	602401602060405180830381865afa15 8015610145573d5f803e3d5ffd5b5050
	50506040513d601f19601f8201168201 8060405250810190610169919061020a
	565b6040517fffffffff000000000000 00000000000000000000000000000000
	00000000000060e085901b1681526001 600160a01b0390921660048301526024
	8201526044015f604051808303815f87 803b1580156101c4575f80fd5b505af1
	1580156101d6573d5f803e3d5ffd5b50 50505050565b5f602082840312156101
	ed575f80fd5b81356001600160a01b03 81168114610203575f80fd5b93925050
	50565b5f6020828403121561021a575f 80fd5b505191905056fea26469706673
	58221220b25880e1acf8095e11ae54c6 90c9bcd80e8428d2d0a7f8b35aceaf84
	8d884a6164736f6c63430008140033
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
			hex_literal::hex!("309403aa87Cd7c70697A6643e561E34b74496133")
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
			hex_literal::hex!("2d7380194b0debD2686af8EFCF5E4a3D02cf5ec3")
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

		assert_eq!(
			DEPOSIT_CONTRACT_BYTECODE,
			hex::decode(expected_bytecode_hex).unwrap().as_slice(),
			"Expected: {expected_bytecode_hex:?}, Actual: {:?}",
			hex::encode(DEPOSIT_CONTRACT_BYTECODE.as_slice()),
		);
	}
}
