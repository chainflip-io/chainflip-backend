use crate::*;
use cf_chains::{assets::btc, btc::BITCOIN_DUST_LIMIT, Bitcoin};
use frame_support::traits::OnRuntimeUpgrade;

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

// No need to migrate Eth or Dot, as they have no explicit minimum egress for any asset.
impl<T: Config<Instance1>> OnRuntimeUpgrade for Migration<T, Instance1> {
	fn on_runtime_upgrade() -> Weight {
		Weight::zero()
	}
}

impl<T: Config<Instance2>> OnRuntimeUpgrade for Migration<T, Instance2> {
	fn on_runtime_upgrade() -> Weight {
		Weight::zero()
	}
}

impl<T: Config<Instance3, TargetChain = Bitcoin>> OnRuntimeUpgrade for Migration<T, Instance3> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		EgressDustLimit::<T, Instance3>::set(btc::Asset::Btc, BITCOIN_DUST_LIMIT.into());

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		assert!(!EgressDustLimit::<T, Instance3>::contains_key(btc::Asset::Btc));
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		assert_eq!(
			EgressDustLimit::<T, Instance3>::get(btc::Asset::Btc),
			BITCOIN_DUST_LIMIT.into()
		);

		Ok(())
	}
}
