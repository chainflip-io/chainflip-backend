use crate::*;

use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};

use cf_chains::evm::H256;
pub struct OracleQueryEnvironmentMigration<T>(PhantomData<T>);

impl<T: Config<Hash = H256>> UncheckedOnRuntimeUpgrade for OracleQueryEnvironmentMigration<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!("🌮 Running migration for Environment pallet: Updating AddressCheckerAddress.");

		let new_eth_address_checker_address =
			match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
				cf_runtime_utilities::genesis_hashes::BERGHAIN => {
					hex_literal::hex!("0000000000000000000000000000000000000000").into() // TODO: To update in
					                                                      // PRO-2320
				},
				cf_runtime_utilities::genesis_hashes::PERSEVERANCE => {
					hex_literal::hex!("0000000000000000000000000000000000000000").into() // TODO: To update in
					                                                      // PRO-2320
				},
				cf_runtime_utilities::genesis_hashes::SISYPHOS => {
					hex_literal::hex!("0000000000000000000000000000000000000000").into() // TODO: To update in
					                                                      // PRO-2320
				},
				// We shouldn't need to update localnet because it will be in the same address
				_ => EthereumAddressCheckerAddress::<T>::get(),
			};
		EthereumAddressCheckerAddress::<T>::put(new_eth_address_checker_address);

		let new_arb_address_checker_address =
			match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
				cf_runtime_utilities::genesis_hashes::BERGHAIN => {
					hex_literal::hex!("0000000000000000000000000000000000000000").into() // TODO: To update in
					                                                      // PRO-2320
				},
				cf_runtime_utilities::genesis_hashes::PERSEVERANCE => {
					hex_literal::hex!("0000000000000000000000000000000000000000").into() // TODO: To update in
					                                                      // PRO-2320
				},
				cf_runtime_utilities::genesis_hashes::SISYPHOS => {
					hex_literal::hex!("0000000000000000000000000000000000000000").into() // TODO: To update in
					                                                      // PRO-2320
				},
				// We shouldn't need to update localnet because it will be in the same address
				_ => ArbitrumAddressCheckerAddress::<T>::get(),
			};
		ArbitrumAddressCheckerAddress::<T>::put(new_arb_address_checker_address);

		Weight::zero()
	}
}
