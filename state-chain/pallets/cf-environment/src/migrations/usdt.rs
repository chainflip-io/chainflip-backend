use crate::*;
use cf_chains::eth::Address as EthereumAddress;
use frame_support::traits::OnRuntimeUpgrade;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let usdt_address: EthereumAddress = match ChainflipNetworkEnvironment::<T>::get() {
			NetworkEnvironment::Mainnet =>
				hex_literal::hex!("dAC17F958D2ee523a2206206994597C13D831ec7").into(),
			NetworkEnvironment::Testnet =>
				hex_literal::hex!("7169D38820dfd117C3FA1f22a697dBA58d90BA06").into(),
			NetworkEnvironment::Development =>
				hex_literal::hex!("0DCd1Bf9A1b36cE34237eEaFef220932846BCD82").into(),
		};
		EthereumSupportedAssets::<T>::insert(EthAsset::Usdt, usdt_address);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}
