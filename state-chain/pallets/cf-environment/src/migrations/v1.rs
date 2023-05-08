use crate::*;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

pub mod old {

	use super::*;

	#[frame_support::storage_alias]
	pub type SupportedEthAssets<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, Asset, EthereumAddress, ValueQuery>;

	#[frame_support::storage_alias]
	pub type StateChainGatewayAddress<T: Config> =
		StorageValue<Pallet<T>, EthereumAddress, ValueQuery>;

	#[frame_support::storage_alias]
	pub type KeyManagerAddress<T: Config> = StorageValue<Pallet<T>, EthereumAddress, ValueQuery>;

	#[frame_support::storage_alias]
	pub type EthVaultAddress<T: Config> = StorageValue<Pallet<T>, EthereumAddress, ValueQuery>;

	#[frame_support::storage_alias]
	pub type GlobalSignatureNonce<T: Config> = StorageValue<Pallet<T>, SignatureNonce, ValueQuery>;
}

// Types that are not old in the context of this migration,
// but are old in the context of the pallet
pub mod archived {

	use super::*;

	use cf_chains::dot::{PolkadotSpecVersion, PolkadotTransactionVersion};
	use cf_primitives::PolkadotBlockNumber;

	#[derive(Debug, Encode, Decode, TypeInfo, Eq, PartialEq, Clone, Default)]
	pub struct PolkadotMetadata {
		pub spec_version: PolkadotSpecVersion,
		pub transaction_version: PolkadotTransactionVersion,
		pub genesis_hash: [u8; 32],
		pub block_hash_count: PolkadotBlockNumber,
	}

	#[frame_support::storage_alias]
	pub type PolkadotNetworkMetadata<T: Config> =
		StorageValue<Pallet<T>, PolkadotMetadata, ValueQuery>;
}

const CHAINFLIP_GENESIS_PERSEVERANCE: &[u8] =
	&hex_literal::hex!("2d00bb9c87a5cccdc67d7c49b6ff87e67a854798583f9a866384d7b7cebbc9b3");
const POLKADOT_GENESIS_PERSEVERANCE: [u8; 32] =
	hex_literal::hex!("bb5111c1747c9e9774c2e6bd229806fb4d7497af2829782f39b977724e490b5c");
const POLKADOT_GENESIS_SISYPHOS: [u8; 32] =
	hex_literal::hex!("1665348821496e14ed56718d4d078e7f85b163bf4e45fa9afbeb220b34ed475a");

// Private polkadot network for sisyphos.
fn polkadot_runtime_version<T: Config>() -> archived::PolkadotMetadata {
	archived::PolkadotMetadata {
		spec_version: 9360,
		transaction_version: 19,
		genesis_hash: match frame_system::Pallet::<T>::block_hash::<T::BlockNumber>(
			Default::default(),
		)
		.as_ref()
		{
			CHAINFLIP_GENESIS_PERSEVERANCE => POLKADOT_GENESIS_PERSEVERANCE,
			// Assume any other network is a sisyphos network
			_ => POLKADOT_GENESIS_SISYPHOS,
		},
		block_hash_count: 4096,
	}
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// migrate the renamed storage items (type not changed)
		for (k, v) in old::SupportedEthAssets::<T>::iter().drain() {
			EthereumSupportedAssets::<T>::insert(k, v)
		}
		EthereumStateChainGatewayAddress::<T>::put(old::StateChainGatewayAddress::<T>::take());
		EthereumKeyManagerAddress::<T>::put(old::KeyManagerAddress::<T>::take());
		EthereumVaultAddress::<T>::put(old::EthVaultAddress::<T>::take());
		EthereumSignatureNonce::<T>::put(old::GlobalSignatureNonce::<T>::take());

		// new storage items related to polkadot integration

		// Polkadot metadata is initialized with the config that is used in the persistent polkadot
		// testnet
		archived::PolkadotNetworkMetadata::<T>::set(polkadot_runtime_version::<T>());

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, &'static str> {
		// Assert that the old storages exist
		assert!(
			old::SupportedEthAssets::<T>::iter_keys().collect::<sp_std::vec::Vec<_>>().len() as u32 >
				0
		);
		assert!(old::StateChainGatewayAddress::<T>::exists());
		assert!(old::KeyManagerAddress::<T>::exists());
		assert!(old::EthVaultAddress::<T>::exists());
		assert!(old::GlobalSignatureNonce::<T>::exists());

		//assert that the polkadot related storages do not exist
		assert!(!PolkadotVaultAccountId::<T>::exists());
		assert!(!PolkadotRuntimeVersion::<T>::exists());
		assert!(!PolkadotProxyAccountNonce::<T>::exists());

		Ok(sp_std::vec::Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: sp_std::vec::Vec<u8>) -> Result<(), &'static str> {
		//assert that the old storages don't exist anymore
		assert!(
			old::SupportedEthAssets::<T>::iter_keys().collect::<sp_std::vec::Vec<_>>().len() as u32 ==
				0
		);
		assert!(!old::StateChainGatewayAddress::<T>::exists());
		assert!(!old::KeyManagerAddress::<T>::exists());
		assert!(!old::EthVaultAddress::<T>::exists());
		assert!(!old::GlobalSignatureNonce::<T>::exists());

		// assert that the new storages exist
		assert!(
			EthereumSupportedAssets::<T>::iter_keys().collect::<sp_std::vec::Vec<_>>().len() as u32 >
				0
		);
		assert!(EthereumStateChainGatewayAddress::<T>::exists());
		assert!(EthereumKeyManagerAddress::<T>::exists());
		assert!(EthereumVaultAddress::<T>::exists());
		assert!(EthereumSignatureNonce::<T>::exists());
		assert!(archived::PolkadotNetworkMetadata::<T>::exists());

		Ok(())
	}
}
