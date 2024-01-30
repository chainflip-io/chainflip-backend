#![cfg(test)]
use crate::{mock::*, Call as PalletCall, ChainState, Error, Event as PalletEvent};
use cf_chains::mocks::MockTrackedData;
use frame_support::{pallet_prelude::DispatchResult, traits::OriginTrait};

trait TestChainTracking {
	fn test_chain_tracking_update(
		self,
		external_block_height: u64,
		expectation: DispatchResult,
	) -> Self;
}

impl TestChainTracking for TestRunner<()> {
	fn test_chain_tracking_update(
		self,
		external_block_height: u64,
		expectation: DispatchResult,
	) -> Self {
		self.then_apply_extrinsics(|_| {
			[(
				OriginTrait::root(),
				RuntimeCall::MockChainTracking(PalletCall::update_chain_state {
					new_chain_state: ChainState {
						block_height: external_block_height,
						tracked_data: MockTrackedData::new(Default::default(), Default::default()),
					},
				}),
				expectation,
			)]
		})
		.then_process_events(|_, event| match event {
			// If the update succeeded, we expect an event to be emitted.
			RuntimeEvent::MockChainTracking(PalletEvent::ChainStateUpdated { new_chain_state }) => {
				assert_eq!(
					new_chain_state,
					ChainState {
						block_height: external_block_height,
						tracked_data: MockTrackedData::new(Default::default(), Default::default())
					}
				);
				None::<()>
			},
			_ => None,
		})
		.map_context(|_| ())
	}
}

#[test]
fn chain_tracking_can_only_advance() {
	const START_BLOCK: u64 = 1000;

	new_test_ext()
		.test_chain_tracking_update(START_BLOCK, Ok(()))
		.test_chain_tracking_update(START_BLOCK, Err(Error::<Test>::StaleDataSubmitted.into()))
		.test_chain_tracking_update(START_BLOCK - 1, Err(Error::<Test>::StaleDataSubmitted.into()))
		.test_chain_tracking_update(START_BLOCK + 1, Ok(()))
		.test_chain_tracking_update(START_BLOCK, Err(Error::<Test>::StaleDataSubmitted.into()))
		.test_chain_tracking_update(START_BLOCK + 1, Err(Error::<Test>::StaleDataSubmitted.into()))
		.test_chain_tracking_update(START_BLOCK + 2, Ok(()))
		// We can skip ahead but then we can't go back again
		.test_chain_tracking_update(START_BLOCK + 10, Ok(()))
		.test_chain_tracking_update(START_BLOCK + 10, Err(Error::<Test>::StaleDataSubmitted.into()))
		.test_chain_tracking_update(START_BLOCK + 9, Err(Error::<Test>::StaleDataSubmitted.into()));
}
