#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_traits::AccountRoleRegistry;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{OnNewAccount, UnfilteredDispatchable},
};
use frame_system::RawOrigin;

const MAX_VALIDATOR_COUNT: u32 = 150;

#[benchmarks]
mod benchmarks {
	use super::*;
	use sp_std::vec;

	#[benchmark]
	fn update_accrual_ratio() {
		let call = Call::<T>::update_accrual_ratio {
			reputation_points: 2,
			number_of_blocks: 151u32.into(),
		};

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}
	}

	#[benchmark]
	fn set_penalty() {
		let call = Call::<T>::set_penalty {
			offence: PalletOffence::MissedHeartbeat.into(),
			new_penalty: Default::default(),
		};

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}
	}

	#[benchmark]
	fn update_missed_heartbeat_penalty() {
		let new_reputation_penalty = 20;
		let call = Call::<T>::update_missed_heartbeat_penalty { new_reputation_penalty };
		let heartbeat_block_interval = T::HeartbeatBlockInterval::get();

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		assert_eq!(
			Pallet::<T>::resolve_penalty_for(PalletOffence::MissedHeartbeat),
			Penalty { reputation: new_reputation_penalty, suspension: heartbeat_block_interval }
		);
	}

	#[benchmark]
	fn heartbeat() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		T::AccountRoleRegistry::register_as_validator(&caller).unwrap();
		let validator_id: T::ValidatorId = caller.clone().into();

		#[extrinsic_call]
		heartbeat(RawOrigin::Signed(caller));

		assert_eq!(LastHeartbeat::<T>::get(&validator_id), Some(1u32.into()));
	}

	#[benchmark]
	fn submit_network_state() {
		for b in 1..MAX_VALIDATOR_COUNT {
			let _caller: T::AccountId = account("doogle", b, b);
		}
		let interval = T::HeartbeatBlockInterval::get();

		// TODO: set the generated validators as active validators
		// PRO-1151
		#[block]
		{
			Pallet::<T>::on_initialize(interval);
		}
	}

	#[benchmark]
	fn on_initialize_no_action() {
		let interval = T::HeartbeatBlockInterval::get();
		let next_block_number = interval + 1u32.into();

		#[block]
		{
			Pallet::<T>::on_initialize(next_block_number);
		}
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
