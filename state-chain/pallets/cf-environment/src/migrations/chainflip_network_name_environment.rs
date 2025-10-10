use crate::*;

use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};

use cf_chains::evm::H256;
pub struct CfNetworkNameEnvironmentMigration<T>(PhantomData<T>);

impl<T: Config<Hash = H256>> UncheckedOnRuntimeUpgrade for CfNetworkNameEnvironmentMigration<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🌮 Running migration for Environment pallet: Updating ChainflipNetworkName.");

		let chainflip_network_name: ChainflipNetwork =
			match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
				cf_runtime_utilities::genesis_hashes::BERGHAIN => ChainflipNetwork::Mainnet,
				cf_runtime_utilities::genesis_hashes::PERSEVERANCE => ChainflipNetwork::Testnet,
				cf_runtime_utilities::genesis_hashes::SISYPHOS => ChainflipNetwork::TestnetDev,
				_ => ChainflipNetwork::Development,
			};
		ChainflipNetworkName::<T>::put(chainflip_network_name);

		log::info!("🌮 Environment pallet migration completed: Updated ChainflipNetworkName.");

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
			cf_runtime_utilities::genesis_hashes::BERGHAIN => {
				assert_eq!(ChainflipNetworkName::<T>::get(), ChainflipNetwork::Mainnet);
			},
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE => {
				assert_eq!(ChainflipNetworkName::<T>::get(), ChainflipNetwork::Testnet);
			},
			cf_runtime_utilities::genesis_hashes::SISYPHOS => {
				assert_eq!(ChainflipNetworkName::<T>::get(), ChainflipNetwork::TestnetDev);
			},
			_ => {
				assert_eq!(ChainflipNetworkName::<T>::get(), ChainflipNetwork::Development);
			},
		};
		Ok(())
	}
}
