use crate::*;

use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};

use cf_chains::evm::H256;
pub struct OracleQueryEnvironmentMigration<T>(PhantomData<T>);

impl<T: Config<Hash = H256>> UncheckedOnRuntimeUpgrade for OracleQueryEnvironmentMigration<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🌮 Running migration for Environment pallet: Updating AddressCheckerAddress.");

		let new_eth_address_checker_address =
			match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
				cf_runtime_utilities::genesis_hashes::BERGHAIN =>
					hex_literal::hex!("1562Ad6bb0e68980A3111F24531c964c7e155611").into(),
				cf_runtime_utilities::genesis_hashes::PERSEVERANCE =>
					hex_literal::hex!("26061f315570bddF11D9055411a3d811c5FF0148").into(),
				cf_runtime_utilities::genesis_hashes::SISYPHOS =>
					hex_literal::hex!("26061f315570bddF11D9055411a3d811c5FF0148").into(),
				// We shouldn't need to update localnet because it will be in the same address
				_ => EthereumAddressCheckerAddress::<T>::get(),
			};
		EthereumAddressCheckerAddress::<T>::put(new_eth_address_checker_address);

		let new_arb_address_checker_address =
			match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
				cf_runtime_utilities::genesis_hashes::BERGHAIN =>
					hex_literal::hex!("69C700A0DEBAb9e349dd1f52ED62eb253a3c9892").into(),
				cf_runtime_utilities::genesis_hashes::PERSEVERANCE =>
					hex_literal::hex!("564e411634189E68ecD570400eBCF783b4aF8688").into(),
				cf_runtime_utilities::genesis_hashes::SISYPHOS =>
					hex_literal::hex!("564e411634189E68ecD570400eBCF783b4aF8688").into(),
				// We shouldn't need to update localnet because it will be in the same address
				_ => ArbitrumAddressCheckerAddress::<T>::get(),
			};
		ArbitrumAddressCheckerAddress::<T>::put(new_arb_address_checker_address);

		Weight::zero()
	}
}
