use crate::*;
use cf_chains::dot::POLKADOT_METADATA;
use sp_std::marker::PhantomData;

/// My first migration.
pub struct Migration<T: Config>(PhantomData<T>);

mod old {

	use super::*;

	#[frame_support::storage_alias]
	pub type SupportedEthAssets<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, Asset, EthereumAddress, ValueQuery>;

	#[frame_support::storage_alias]
	pub type StakeManagerAddress<T: Config> = StorageValue<Pallet<T>, EthereumAddress, ValueQuery>;

	#[frame_support::storage_alias]
	pub type KeyManagerAddress<T: Config> = StorageValue<Pallet<T>, EthereumAddress, ValueQuery>;

	#[frame_support::storage_alias]
	pub type EthVaultAddress<T: Config> = StorageValue<Pallet<T>, EthereumAddress, ValueQuery>;

	// #[frame_support::storage_alias]
	// pub type EthereumChainId<T: Config> = StorageValue<Pallet<T>, u64, ValueQuery>;

	// #[frame_support::storage_alias]
	// pub type CfeSettings<T: Config> = StorageValue<Pallet<T>, cfe::CfeSettings, ValueQuery>;

	// #[frame_support::storage_alias]
	// pub type CurrentSystemState<T: Config> = StorageValue<Pallet<T>, SystemState, ValueQuery>;

	#[frame_support::storage_alias]
	pub type GlobalSignatureNonce<T: Config> = StorageValue<Pallet<T>, SignatureNonce, ValueQuery>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		for (k, v) in old::SupportedEthAssets::<T>::iter().drain() {
			EthereumSupportedAssets::<T>::insert(k, v)
		}
		EthereumStakeManagerAddress::<T>::put(old::StakeManagerAddress::<T>::take());
		EthereumKeyManagerAddress::<T>::put(old::KeyManagerAddress::<T>::take());
		EthereumVaultAddress::<T>::put(old::EthVaultAddress::<T>::take());
		EthereumSignatureNonce::<T>::put(old::GlobalSignatureNonce::<T>::take());

		PolkadotVaultAccountId::<T>::set(None);
		PolkadotNetworkMetadata::<T>::set(POLKADOT_METADATA);
		PolkadotProxyAccountNonce::<T>::set(0);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, &'static str> {
		todo!()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: sp_std::vec::Vec<u8>) -> Result<(), &'static str> {
		todo!()
	}
}
