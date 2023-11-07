use super::{Address as EthereumAddress, DeploymentStatus, EvmFetchId};
use crate::{ChainflipEnvironment, DepositChannel};
use cf_primitives::{
	chains::{assets::eth::Asset, Ethereum},
	ChannelId,
};
use cf_utilities::SliceToArray;
use codec::{Decode, Encode};
use core::panic;
use ethereum_types::H160;
use frame_support::sp_runtime::{
	traits::{Hash, Keccak256, StaticLookup},
	DispatchError,
};
use scale_info::TypeInfo;
use sp_core::Get;
use sp_std::{mem::size_of, vec::Vec};

// From master branch of chainflip-eth-contracts
// @FIXME store on and retrieve from the chain
const DEPOSIT_CONTRACT_BYTECODE: [u8; 1114] = hex_literal::hex!(
	"60a060405234801561001057600080f d5b5060405161045a38038061045a833
	98101604081905261002f91610189565 b336080526001600160a01b03811673e
	eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee eeeeeee036100b257604051600090339
	047908381818185875af1925050503d8 060008114610099576040519150601f1
	9603f3d011682016040523d82523d600 0602084013e61009e565b606091505b5
	0509050806100ac57600080fd5b50610 183565b6040516370a0823160e01b815
	23060048201526001600160a01b03821 69063a9059cbb90339083906370a0823
	190602401602060405180830381865af a158015610100573d6000803e3d6000f 
	d5b505050506040513d601f19601f820 11682018060405250810190610124919
	06101b9565b6040516001600160e01b0 31960e085901b1681526001600160a01
	b0390921660048301526024820152604 401600060405180830381600087803b1
	5801561016a57600080fd5b505af1158 01561017e573d6000803e3d6000fd5b5
	05050505b506101d2565b60006020828 403121561019b57600080fd5b8151600
	1600160a01b03811681146101b257600 080fd5b9392505050565b60006020828
	40312156101cb57600080fd5b5051919 050565b6080516102686101f26000396
	0008181602b015260d40152610268600 0f3fe608060405260043610610022576
	0003560e01c8063f109a0be146100ae5 7600080fd5b366100a95760007f00000
	00000000000000000000000000000000 00000000000000000000000000060016
	00160a01b03164760405160006040518 083038185875af1925050503d8060008
	114610094576040519150601f19603f3 d011682016040523d82523d600060208
	4013e610099565b606091505b5050905 0806100a757600080fd5b005b600080f
	d5b3480156100ba57600080fd5b50610 0a76100c93660046101e9565b3360016
	00160a01b037f0000000000000000000 00000000000000000000000000000000
	000000000000016146100fe57600080f d5b6040516370a0823160e01b8152306
	0048201526001600160a01b038216906 3a9059cbb90339083906370a08231906
	02401602060405180830381865afa158 01561014c573d6000803e3d6000fd5b5
	05050506040513d601f19601f8201168 20180604052508101906101709190610
	219565b6040517fffffffff000000000 00000000000000000000000000000000
	00000000000000060e085901b1681526 001600160a01b0390921660048301526
	02482015260440160006040518083038 1600087803b1580156101ce57600080f
	d5b505af11580156101e2573d6000803 e3d6000fd5b5050505050565b6000602
	082840312156101fb57600080fd5b813 56001600160a01b03811681146102125
	7600080fd5b9392505050565b6000602 0828403121561022b57600080fd5b505
	191905056fea26469706673582212207 a3063a75755b8b3364bcf7137526722a
	9ac4adcc81866e63e0a9dfb44df3a3e6 4736f6c63430008140033"
);

// Always the same, this is a CREATE2 constant.
const PREFIX_BYTE: u8 = 0xff;

pub const ETHEREUM_ETH_ADDRESS: EthereumAddress = H160([0xEE; 20]);

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct EthereumDepositChannel {
	pub channel_id: ChannelId,
	pub address: EthereumAddress,
	pub asset: Asset,
	pub deployment_status: DeploymentStatus,
}

impl core::hash::Hash for EthereumDepositChannel {
	fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
		// Only hash the channel id, since every channel should be uniquely defined by its channel
		// id.
		self.channel_id.hash(state);
	}
}

impl DepositChannel<Ethereum> for EthereumDepositChannel {
	type Deposit = ();

	fn generate_new<E: ChainflipEnvironment>(
		channel_id: ChannelId,
		asset: <Ethereum as crate::Chain>::ChainAsset,
	) -> Result<Self, DispatchError> {
		Ok(Self {
			channel_id,
			address: get_create_2_address(
				<E as Get<EthereumAddress>>::get(),
				match asset {
					Asset::Eth => None,
					token => Some(<E as StaticLookup>::lookup(token).map_err(|_lookup_error| {
						DispatchError::Other(
							"Failed to generate new deposit channel: unsupported asset.",
						)
					})?),
				},
				channel_id,
			),
			asset,
			deployment_status: DeploymentStatus::Undeployed,
		})
	}

	fn channel_id(&self) -> ChannelId {
		self.channel_id
	}

	/// Before deployment, Ethereum deposit channels can only be used for a specific asset.
	fn asset(&self) -> Option<<Ethereum as crate::Chain>::ChainAsset> {
		match self.deployment_status {
			DeploymentStatus::Undeployed => Some(self.asset),
			_ => None,
		}
	}

	fn address(&self) -> &EthereumAddress {
		&self.address
	}

	fn fetch_params(&self, _: ()) -> <Ethereum as crate::Chain>::FetchParams {
		match self.deployment_status {
			DeploymentStatus::Undeployed => EvmFetchId::DeployAndFetch(self.channel_id),
			DeploymentStatus::Pending => {
				if cfg!(debug_assertions) {
					panic!("Cannot fetch from a pending address")
				} else {
					// Defensively return NotRequired to avoid trying to fetch from a pending
					// deployment. This should never happen.
					log::error!("Cannot fetch from a pending address");
					EvmFetchId::NotRequired
				}
			},
			DeploymentStatus::Deployed => match self.asset {
				Asset::Eth => EvmFetchId::NotRequired,
				_erc20 => EvmFetchId::Fetch(self.address),
			},
		}
	}
}

/// Derives the CREATE2 Ethereum address for a given asset, vault, and channel id.
/// @param vault_address The address of the Ethereum Vault
/// @param token_address The token address if this is a token deposit
/// @param channel_id The numerical channel id
pub fn get_create_2_address(
	vault_address: EthereumAddress,
	token_address: Option<EthereumAddress>,
	channel_id: ChannelId,
) -> EthereumAddress {
	// This hash is used in the later CREATE2 derivation.
	// see: https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/Deposit.sol.
	let deploy_transaction_bytes_hash = Keccak256::hash(
		itertools::chain!(
			DEPOSIT_CONTRACT_BYTECODE,
			[0u8; 12], // padding
			token_address.unwrap_or(ETHEREUM_ETH_ADDRESS).to_fixed_bytes(),
		)
		.collect::<Vec<_>>()
		.as_slice(),
	);

	let create_2_args = itertools::chain!(
		[PREFIX_BYTE],
		vault_address.to_fixed_bytes(),
		get_salt(channel_id),
		deploy_transaction_bytes_hash.to_fixed_bytes()
	)
	.collect::<Vec<_>>();

	H160(Keccak256::hash(&create_2_args)[12..].as_array())
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
	const VAULT_ADDRESS: EthereumAddress =
		H160(hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512"));
	const FLIP_ADDRESS: EthereumAddress =
		H160(hex_literal::hex!("Cf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9"));

	#[test]
	fn test_eth_eth() {
		assert_eq!(
			get_create_2_address(VAULT_ADDRESS, None, 420696969),
			H160(hex_literal::hex!("311373270d730749FF22fd3c1F9836AA803Be47a"))
		);

		println!("Derivation worked for ETH:ETH! 🚀");
	}

	#[test]
	fn test_eth_flip() {
		// Based on previously verified values.
		const VAULT_ADDRESS: EthereumAddress =
			H160(hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512"));

		assert_eq!(
			get_create_2_address(VAULT_ADDRESS, Some(FLIP_ADDRESS), 42069),
			H160(hex_literal::hex!("e3477D1C61feDe43a5bbB5A7Fd40489225D18826"))
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
