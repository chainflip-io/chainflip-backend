use crate::*;
use cf_chains::{assets::btc, btc::BITCOIN_DUST_LIMIT, Bitcoin};
use frame_support::traits::OnRuntimeUpgrade;

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

// No need to migrate Eth or Dot, as they have no minimum egress for any asset, and the Value query
// will return 0.
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
		MinimumEgress::<T, Instance3>::insert(btc::Asset::Btc, BITCOIN_DUST_LIMIT);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		assert!(MinimumEgress::<T, Instance3>::get(btc::Asset::Btc).is_none());
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		assert_eq!(MinimumEgress::<T, Instance3>::get(btc::Asset::Btc), Some(BITCOIN_DUST_LIMIT));

		Ok(())
	}
}
