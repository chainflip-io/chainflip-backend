use crate::*;
use frame_support::traits::OnRuntimeUpgrade;

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let net_type = T::NetworkEnvironment::get_network_environment();

		let safety_margin: Option<u32> = match T::TargetChain::NAME {
			"Bitcoin" => Some({
				match net_type {
					cf_primitives::NetworkEnvironment::Mainnet |
					cf_primitives::NetworkEnvironment::Development => 2,
					cf_primitives::NetworkEnvironment::Testnet => 5,
				}
			}),
			"Ethereum" => Some({
				match net_type {
					cf_primitives::NetworkEnvironment::Mainnet |
					cf_primitives::NetworkEnvironment::Testnet => 6,
					cf_primitives::NetworkEnvironment::Development => 2,
				}
			}),
			"Polkadot" => None,
			_ => unreachable!("Unsupported chain"),
		};

		WitnessSafetyMargin::<T, I>::set(safety_margin.map(Into::into));

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		let margin = WitnessSafetyMargin::<T, I>::get();

		match T::TargetChain::NAME {
			"Bitcoin" | "Ethereum" => {
				assert!(margin.is_some())
			},
			"Polkadot" => {
				assert!(margin.is_none())
			},
			_ => unreachable!("Unsupported chain"),
		}

		Ok(())
	}
}
