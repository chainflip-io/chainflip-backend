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
					hex_literal::hex!("13Ad793E7B75eaaCee34b69792552f086b301380").into(),
				cf_runtime_utilities::genesis_hashes::PERSEVERANCE =>
					hex_literal::hex!("C191c202fdc308F54fF815fb3309eCd052E75D73").into(),
				cf_runtime_utilities::genesis_hashes::SISYPHOS =>
					hex_literal::hex!("7c08ea651dA70239DA8cb87A5913c3579Ba9F6fE").into(),
				_ => hex_literal::hex!("c5a5C42992dECbae36851359345FE25997F5C42d").into(),
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
					hex_literal::hex!("13Ad793E7B75eaaCee34b69792552f086b301380").into()
				);
			},
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE => {
				assert_eq!(
					EthereumScUtilsAddress::<T>::get(),
					hex_literal::hex!("C191c202fdc308F54fF815fb3309eCd052E75D73").into()
				);
			},
			cf_runtime_utilities::genesis_hashes::SISYPHOS => {
				assert_eq!(
					EthereumScUtilsAddress::<T>::get(),
					hex_literal::hex!("7c08ea651dA70239DA8cb87A5913c3579Ba9F6fE").into()
				);
			},
			_ => {
				assert_eq!(
					EthereumScUtilsAddress::<T>::get(),
					hex_literal::hex!("c5a5C42992dECbae36851359345FE25997F5C42d").into()
				);
			},
		};
		Ok(())
	}
}
