use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use old::maybe_get_timeout_for_type;

use crate::*;

// Constants copied from `runtime/src/constants.rs`,
// in order to use same timeout values as given in `node/src/chain_spec.rs`
pub const MILLISECONDS_PER_BLOCK_ETHEREUM: u32 = 14 * 1000;
pub const MILLISECONDS_PER_BLOCK_POLKADOT: u32 = 6 * 1000;
pub const MILLISECONDS_PER_BLOCK_ARBITRUM: u32 = 250;
pub const MILLISECONDS_PER_BLOCK_SOLANA: u32 = 400;

pub const BLOCKS_PER_MINUTE_ETHEREUM: u32 = 60000 / MILLISECONDS_PER_BLOCK_ETHEREUM;
pub const BLOCKS_PER_MINUTE_POLKADOT: u32 = 60000 / MILLISECONDS_PER_BLOCK_POLKADOT;
pub const BLOCKS_PER_MINUTE_ARBITRUM: u32 = 60000 / MILLISECONDS_PER_BLOCK_ARBITRUM;
pub const BLOCKS_PER_MINUTE_SOLANA: u32 = 60000 / MILLISECONDS_PER_BLOCK_SOLANA;

mod old {
	use cf_primitives::BlockNumber;

	use super::*;

	// Same timeout values as previously defined in `#[pallet::constant]`s
	// and same as currently used in `node/src/chain_spec.rs`
	pub const ETHEREUM_BROADCAST_TIMEOUT: BlockNumber = 5 * BLOCKS_PER_MINUTE_ETHEREUM;
	pub const POLKADOT_BROADCAST_TIMEOUT: BlockNumber = 4 * BLOCKS_PER_MINUTE_POLKADOT;
	pub const BITCOIN_BROADCAST_TIMEOUT: BlockNumber = 9;
	pub const ARBITRUM_BROADCAST_TIMEOUT: BlockNumber = 2 * BLOCKS_PER_MINUTE_ARBITRUM;
	pub const SOLANA_BROADCAST_TIMEOUT: BlockNumber = 4 * BLOCKS_PER_MINUTE_SOLANA;

	// For testing purposes we also have to set the timeout for the mock configuration,
	// following `BROADCAST_EXPIRY_BLOCKS` in `mock.rs`
	pub const MOCK_ETHEREUM_BROADCAST_TIMEOUT: BlockNumber = 4;

	pub fn maybe_get_timeout_for_type<T: Config<I>, I: 'static>(
	) -> Option<ChainBlockNumberFor<T, I>> {
		// Choose timeout value based on statically defined chain name.
		// It should be the same as the previously used constants.
		let timeout: ChainBlockNumberFor<T, I> = match T::TargetChain::NAME {
			"Ethereum" => old::ETHEREUM_BROADCAST_TIMEOUT,
			"Polkadot" => old::POLKADOT_BROADCAST_TIMEOUT,
			"Bitcoin" => old::BITCOIN_BROADCAST_TIMEOUT,
			"Arbitrum" => old::ARBITRUM_BROADCAST_TIMEOUT,
			"Solana" => old::SOLANA_BROADCAST_TIMEOUT,
			"MockEthereum" => old::MOCK_ETHEREUM_BROADCAST_TIMEOUT,
			_ => return None, // skip migration for unexpected chain name
		}
		.into();
		Some(timeout)
	}
}

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		if let Some(timeout) = maybe_get_timeout_for_type::<T, I>() {
			BroadcastTimeout::<T, I>::set(timeout);
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		assert_eq!(BroadcastTimeout::<T, I>::get(), maybe_get_timeout_for_type::<T, I>().unwrap());
		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {

	#[test]
	fn test_migration() {
		use super::*;
		use crate::mock::*;

		new_test_ext().execute_with(|| {
			// Perform runtime migration.
			super::Migration::<Test, _>::on_runtime_upgrade();
			#[cfg(feature = "try-runtime")]
			super::Migration::<Test, _>::post_upgrade(vec![]).unwrap();

			// Storage is initialized correctly
			assert_eq!(
				crate::BroadcastTimeout::<Test, _>::get(),
				maybe_get_timeout_for_type::<Test, _>().unwrap()
			);
		});
	}
}
