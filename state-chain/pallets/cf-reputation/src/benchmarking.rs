#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_traits::{AccountRoleRegistry, EpochInfo};
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
	fn submit_network_state(o: Linear<0, MAX_VALIDATOR_COUNT>) {
		// o: number of offline validators to report

		// Generate validators and set as validators
		let validators =
			(0..o).map(|v| account("validator", v, v)).collect::<BTreeSet<T::ValidatorId>>();
		T::EpochInfo::set_authorities(validators);

		let interval = T::HeartbeatBlockInterval::get();

		// Without heartbeat, all nodes are automatically disqualified.
		let _old_heartbeat = LastHeartbeat::<T>::clear(u32::MAX, None);

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
