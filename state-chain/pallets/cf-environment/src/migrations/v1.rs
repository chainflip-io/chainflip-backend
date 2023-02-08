use crate::*;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

pub mod old {

	use cf_chains::dot::{PolkadotSpecVersion, PolkadotTransactionVersion};

	use super::*;

	#[derive(Debug, Encode, Decode, TypeInfo, Eq, PartialEq, Clone, Default)]
	pub struct PolkadotMetadata {
		pub spec_version: PolkadotSpecVersion,
		pub transaction_version: PolkadotTransactionVersion,
		pub genesis_hash: [u8; 32],
	}

	#[frame_support::storage_alias]
	pub type SupportedEthAssets<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, Asset, EthereumAddress, ValueQuery>;

	#[frame_support::storage_alias]
	pub type StakeManagerAddress<T: Config> = StorageValue<Pallet<T>, EthereumAddress, ValueQuery>;

	#[frame_support::storage_alias]
	pub type KeyManagerAddress<T: Config> = StorageValue<Pallet<T>, EthereumAddress, ValueQuery>;

	#[frame_support::storage_alias]
	pub type EthVaultAddress<T: Config> = StorageValue<Pallet<T>, EthereumAddress, ValueQuery>;

	#[frame_support::storage_alias]
	pub type GlobalSignatureNonce<T: Config> = StorageValue<Pallet<T>, SignatureNonce, ValueQuery>;

	#[frame_support::storage_alias]
	pub type PolkadotNetworkMetadata<T: Config> =
		StorageValue<Pallet<T>, PolkadotMetadata, ValueQuery>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// migrate the renamed storage items (type not changed)
		for (k, v) in old::SupportedEthAssets::<T>::iter().drain() {
			EthereumSupportedAssets::<T>::insert(k, v)
		}
		EthereumStakeManagerAddress::<T>::put(old::StakeManagerAddress::<T>::take());
		EthereumKeyManagerAddress::<T>::put(old::KeyManagerAddress::<T>::take());
		EthereumVaultAddress::<T>::put(old::EthVaultAddress::<T>::take());
		EthereumSignatureNonce::<T>::put(old::GlobalSignatureNonce::<T>::take());

		// new storage items related to polkadot integration

		// Polkadot metadata is initialized with the config that is used in the persistent polkadot
		// testnet
		old::PolkadotNetworkMetadata::<T>::put(old::PolkadotMetadata {
			spec_version: 9320,
			transaction_version: 16,
			genesis_hash: hex_literal::hex!(
				"5f551688012d25a98e729752169f509c6186af8079418c118844cc852b332bf5"
			),
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, &'static str> {
		// Assert that the old storages exist
		assert!(
			old::SupportedEthAssets::<T>::iter_keys().collect::<sp_std::vec::Vec<_>>().len() as u32 >
				0
		);
		assert!(old::StakeManagerAddress::<T>::exists());
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
		assert!(!old::StakeManagerAddress::<T>::exists());
		assert!(!old::KeyManagerAddress::<T>::exists());
		assert!(!old::EthVaultAddress::<T>::exists());
		assert!(!old::GlobalSignatureNonce::<T>::exists());

		// assert that the new storages exist
		assert!(
			EthereumSupportedAssets::<T>::iter_keys().collect::<sp_std::vec::Vec<_>>().len() as u32 >
				0
		);
		assert!(EthereumStakeManagerAddress::<T>::exists());
		assert!(EthereumKeyManagerAddress::<T>::exists());
		assert!(EthereumVaultAddress::<T>::exists());
		assert!(EthereumSignatureNonce::<T>::exists());
		assert!(PolkadotRuntimeVersion::<T>::exists());

		Ok(())
	}
}
