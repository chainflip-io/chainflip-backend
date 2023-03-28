#![cfg_attr(not(feature = "std"), no_std)]

//! Chainflip Primitives
//!
//! Primitive types to be used across Chainflip's various crates

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::crypto::AccountId32;
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	FixedU128, MultiSignature, RuntimeDebug,
};

use sp_std::vec::Vec;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

pub mod chains;

pub use chains::{assets::any::Asset, ForeignChain};
pub mod liquidity;
pub use liquidity::*;

/// An index to a block.
pub type BlockNumber = u32;

pub type FlipBalance = u128;

pub type CeremonyId = u64;

pub type EpochIndex = u32;

pub type AuthorityCount = u32;

pub type IntentId = u64;

pub type EgressCounter = u64;

pub type EgressId = (ForeignChain, EgressCounter);

pub type ExchangeRate = FixedU128;

pub type EthereumAddress = [u8; 20];

pub type EthAmount = u128;

pub type AssetAmount = u128;

pub type BasisPoints = u16;

pub type BroadcastId = u32;

/// Alias to the opaque account ID type for this chain, actually a `AccountId32`. This is always
/// 32 bytes.
pub type PolkadotAccountId = AccountId32;

pub type PolkadotBlockNumber = u32;

// Polkadot extrinsics are uniquely identified by <block number>-<extrinsic index>
// https://wiki.polkadot.network/docs/build-protocol-info
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, Debug, PartialEq, Eq)]
pub struct TxId {
	pub block_number: PolkadotBlockNumber,
	pub extrinsic_index: u32,
}

pub const ETHEREUM_ETH_ADDRESS: EthereumAddress = [0xEE; 20];

/// The very first epoch number
pub const GENESIS_EPOCH: u32 = 1;

//Addresses can have all kinds of different lengths in bitcoin but we would support upto 100 since
// we dont expect addresses higher than 100
pub const MAX_BTC_ADDRESS_LENGTH: u32 = 100;

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Roles in the Chainflip network.
///
/// Chainflip's network is permissioned and only accessible to owners of accounts with staked Flip.
/// In addition to staking, the account owner is required to indicate the role they intend to play
/// in the network. This will determine in which ways the account can interact with the chain.
///
/// Each account can only be associated with a single role, and the role can only be updated from
/// the initial [AccountRole::None] state.
#[derive(PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum AccountRole {
	/// The default account type - indicates a bare account with no special role or permissions.
	None,
	/// Validators are responsible for the maintenance and operation of the Chainflip network. This
	/// role is required for any node that wishes to participate in auctions.
	Validator,
	/// Liquidity providers can deposit assets and deploy them in trading pools.
	LiquidityProvider,
	/// Relayers submit swap intents on behalf of users.
	Relayer,
}

impl Default for AccountRole {
	fn default() -> Self {
		AccountRole::None
	}
}

pub type PublicKeyBytes = Vec<u8>;

#[derive(Encode, Decode, PartialEq, Eq, Hash, Debug, Clone, TypeInfo)]
pub struct KeyId {
	pub epoch_index: EpochIndex,
	pub public_key_bytes: PublicKeyBytes,
}

impl KeyId {
	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend_from_slice(&self.epoch_index.to_be_bytes());
		bytes.extend_from_slice(&self.public_key_bytes);
		bytes
	}

	pub fn from_bytes(bytes: &[u8]) -> Self {
		let size_of_epoch_index = sp_std::mem::size_of::<EpochIndex>();
		let epoch_index =
			EpochIndex::from_be_bytes(bytes[..size_of_epoch_index].try_into().unwrap());
		let public_key_bytes = bytes[size_of_epoch_index..].to_vec();
		Self { epoch_index, public_key_bytes }
	}
}

impl sp_std::fmt::Display for KeyId {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter<'_>) -> core::fmt::Result {
		#[cfg(feature = "std")]
		{
			write!(
				f,
				"KeyId(epoch_index: {}, public_key_bytes: {})",
				self.epoch_index,
				hex::encode(self.public_key_bytes.clone())
			)
		}
		#[cfg(not(feature = "std"))]
		{
			write!(
				f,
				"KeyId(epoch_index: {}, public_key_bytes: {:?})",
				self.epoch_index, self.public_key_bytes
			)
		}
	}
}

#[test]
fn test_key_id_to_and_from_bytes() {
	let key_ids = [
		KeyId { epoch_index: 0, public_key_bytes: vec![] },
		KeyId { epoch_index: 1, public_key_bytes: vec![1, 2, 3] },
		KeyId { epoch_index: 22, public_key_bytes: vec![0xa, 93, 145, u8::MAX, 0] },
	];

	for key_id in key_ids {
		assert_eq!(key_id, KeyId::from_bytes(&key_id.to_bytes()));
	}

	let key_id = KeyId {
		epoch_index: 29,
		public_key_bytes: vec![
			0xa,
			93,
			141,
			u8::MAX,
			0,
			82,
			2,
			39,
			144,
			241,
			29,
			91,
			3,
			241,
			120,
			194,
		],
	};
	// We check this because if this form changes then there will be an impact to how keys should be
	// loaded from the db on the CFE. Thus, we want to be notified if this changes.
	let expected_bytes =
		vec![0, 0, 0, 29, 10, 93, 141, 255, 0, 82, 2, 39, 144, 241, 29, 91, 3, 241, 120, 194];
	assert_eq!(expected_bytes, key_id.to_bytes());
	assert_eq!(key_id, KeyId::from_bytes(&expected_bytes));
}

pub type EgressBatch<Amount, EgressAddress> = Vec<(Amount, EgressAddress)>;
