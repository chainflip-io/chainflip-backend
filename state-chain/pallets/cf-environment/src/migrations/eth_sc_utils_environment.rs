use crate::*;

use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};

use cf_chains::evm::H256;
pub struct EthScUtilsEnvironmentMigration<T>(PhantomData<T>);

impl<T: Config<Hash = H256>> UncheckedOnRuntimeUpgrade for EthScUtilsEnvironmentMigration<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🌮 Running migration for Environment pallet: Updating AddressCheckerAddress.");

		let eth_sc_utils_address: EvmAddress =
			match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
				cf_runtime_utilities::genesis_hashes::BERGHAIN =>
					hex_literal::hex!("0000000000000000000000000000000000000000").into(), /* TODO: To update */
				cf_runtime_utilities::genesis_hashes::PERSEVERANCE =>
					hex_literal::hex!("0000000000000000000000000000000000000000").into(), /* TODO: To update */
				cf_runtime_utilities::genesis_hashes::SISYPHOS =>
					hex_literal::hex!("0000000000000000000000000000000000000000").into(), /* TODO: To update */
				_ => hex_literal::hex!("7a2088a1bFc9d81c55368AE168C2C02570cB814F").into(),
			};
		EthereumScUtilsAddress::<T>::put(eth_sc_utils_address);

		log::info!("🌮 Environment pallet migration completed: Updated AddressCheckerAddress.");

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
			cf_runtime_utilities::genesis_hashes::BERGHAIN => {
				assert_eq!(
					EthereumScUtilsAddress::<T>::get(),
					hex_literal::hex!("0000000000000000000000000000000000000000").into() /* TODO: To update */
				);
			},
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE => {
				assert_eq!(
					EthereumScUtilsAddress::<T>::get(),
					hex_literal::hex!("0000000000000000000000000000000000000000").into() /* TODO: To update */
				);
			},
			cf_runtime_utilities::genesis_hashes::SISYPHOS => {
				assert_eq!(
					EthereumScUtilsAddress::<T>::get(),
					hex_literal::hex!("0000000000000000000000000000000000000000").into() /* TODO: To update */
				);
			},
			_ => {
				assert_eq!(
					EthereumScUtilsAddress::<T>::get(),
					hex_literal::hex!("7a2088a1bFc9d81c55368AE168C2C02570cB814F").into()
				);
			},
		};
		Ok(())
	}
}
