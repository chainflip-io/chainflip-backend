use crate::*;

use frame_support::{pallet_prelude::Weight, traits::UncheckedOnRuntimeUpgrade};

use cf_chains::evm::H256;
pub struct OracleQueryEnvironmentMigration<T>(PhantomData<T>);

impl<T: Config<Hash = H256>> UncheckedOnRuntimeUpgrade for OracleQueryEnvironmentMigration<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
			cf_runtime_utilities::genesis_hashes::BERGHAIN => {
				assert_eq!(
					EthereumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("79001a5e762f3befc8e5871b42f6734e00498920").into()
				);
				assert_eq!(
					ArbitrumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("c1b12993f760b654897f0257573202fba13d5481").into()
				);
			},
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE => {
				assert_eq!(
					EthereumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("58eacd5a40eebcbbcb660f178f9a46b1ad63f846").into()
				);
				assert_eq!(
					ArbitrumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("4f358ec5bd58c994f74b317554d7472769a0ccf8").into()
				);
			},
			cf_runtime_utilities::genesis_hashes::SISYPHOS => {
				assert_eq!(
					EthereumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("638e16dd15588b81257ebe9676fa1a0175fdb70a").into()
				);
				assert_eq!(
					ArbitrumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("2e78f26e9798ebde7f2b19736de6aae4d51d6d34").into()
				);
			},
			_ => {
				assert_eq!(
					EthereumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512").into()
				);
				assert_eq!(
					ArbitrumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0").into()
				);
			},
		};
		Ok(Default::default())
	}

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

		log::info!("🌮 Environment pallet migration completed: Updated AddressCheckerAddress.");

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		match cf_runtime_utilities::genesis_hashes::genesis_hash::<T>() {
			cf_runtime_utilities::genesis_hashes::BERGHAIN => {
				assert_eq!(
					EthereumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("1562Ad6bb0e68980A3111F24531c964c7e155611").into()
				);
				assert_eq!(
					ArbitrumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("69C700A0DEBAb9e349dd1f52ED62eb253a3c9892").into()
				);
			},
			cf_runtime_utilities::genesis_hashes::PERSEVERANCE => {
				assert_eq!(
					EthereumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("26061f315570bddF11D9055411a3d811c5FF0148").into()
				);
				assert_eq!(
					ArbitrumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("564e411634189E68ecD570400eBCF783b4aF8688").into()
				);
			},
			cf_runtime_utilities::genesis_hashes::SISYPHOS => {
				assert_eq!(
					EthereumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("26061f315570bddF11D9055411a3d811c5FF0148").into()
				);
				assert_eq!(
					ArbitrumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("564e411634189E68ecD570400eBCF783b4aF8688").into()
				);
			},
			_ => {
				assert_eq!(
					EthereumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("e7f1725E7734CE288F8367e1Bb143E90bb3F0512").into()
				);
				assert_eq!(
					ArbitrumAddressCheckerAddress::<T>::get(),
					hex_literal::hex!("9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0").into()
				);
			},
		};
		Ok(())
	}
}
