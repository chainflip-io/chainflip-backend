#![cfg(test)]
use crate::{
	mock::*, Call as PalletCall, ChainState, CurrentChainState, Error, Event as PalletEvent,
	FeeMultiplier,
};
use cf_chains::mocks::MockTrackedData;
use frame_support::{assert_noop, assert_ok, pallet_prelude::DispatchResult, traits::OriginTrait};
use sp_runtime::FixedU128;

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

#[test]
fn can_update_fee_multiplier() {
	new_test_ext().execute_with(|| {
		const NEW_FEE_MULTIPLIER: FixedU128 = FixedU128::from_u32(2);
		assert_ne!(FeeMultiplier::<Test>::get(), NEW_FEE_MULTIPLIER);
		assert_ok!(MockChainTracking::update_fee_multiplier(
			OriginTrait::root(),
			NEW_FEE_MULTIPLIER
		));
		assert_noop!(
			MockChainTracking::update_fee_multiplier(OriginTrait::signed(100), NEW_FEE_MULTIPLIER),
			sp_runtime::traits::BadOrigin
		);
		assert_eq!(FeeMultiplier::<Test>::get(), NEW_FEE_MULTIPLIER);
	});
}

#[test]
fn can_update_chain_state() {
	new_test_ext().execute_with(|| {
		let new_chain_state = ChainState {
			block_height: 1,
			tracked_data: MockTrackedData::new(Default::default(), Default::default()),
		};
		assert_ne!(
			CurrentChainState::<Test>::get().unwrap().block_height,
			new_chain_state.block_height
		);
		assert_noop!(
			MockChainTracking::update_chain_state(OriginTrait::none(), new_chain_state.clone()),
			sp_runtime::DispatchError::BadOrigin,
		);
		assert_ok!(MockChainTracking::update_chain_state(
			OriginTrait::root(),
			new_chain_state.clone(),
		));
		assert_eq!(
			CurrentChainState::<Test>::get().unwrap().block_height,
			new_chain_state.block_height
		);
	});
}
