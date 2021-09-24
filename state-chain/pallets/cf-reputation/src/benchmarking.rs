//! Benchmarking setup for reputation
#![cfg(feature = "runtime-benchmarks")]

use super::*;

#[allow(unused)]
use crate::Pallet as pallet_cf_reputation;
use frame_benchmarking::account;
use frame_benchmarking::{benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec, vec::Vec};

type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
const VALIDATOR_COUNT: u32 = 150;

fn as_validators<T: Config>(accounts: &Vec<AccountIdOf<T>>) -> Vec<T::ValidatorId> {
	accounts.iter().map(|a| a.clone().into()).collect()
}

fn create_accounts<T: Config>(count: u32) -> Vec<AccountIdOf<T>> {
	(0..=count).map(|i| account("doogle", i, 0)).collect()
}

fn initialise_pallet<T: Config>(count: u32) -> Vec<AccountIdOf<T>> {
	let accounts = create_accounts::<T>(count);

	Pallet::<T>::on_new_epoch(&as_validators::<T>(&accounts), 0u32.into());

	accounts
}

fn expected_reputation_penalty<T: Config>() -> ReputationPoints {
	let ReputationPenalty { points, blocks } = T::ReputationPointPenalty::get();

	(T::HeartbeatBlockInterval::get().try_into().unwrap_or(0) * points as u32
		/ blocks.try_into().unwrap_or(0)) as ReputationPoints
}

benchmarks! {
	// heartbeat {
	// 	let b in 0 .. VALIDATOR_COUNT;
	// 	let accounts = initialise_pallet::<T>(VALIDATOR_COUNT);
	// }: _(RawOrigin::Signed(accounts[b as usize].clone()))
	// verify {
	// 	let validator_id: T::ValidatorId = accounts[b as usize].clone().into();
	// 	let expected_credits = Pallet::<T>::online_credit_reward();
	// 	let reputation_for_validator = Pallet::<T>::reputation(&validator_id);
	// 	assert_eq!(reputation_for_validator.online_credits, expected_credits);
	// }
	heartbeat {
		let b in 0 .. VALIDATOR_COUNT;
		let accounts = initialise_pallet::<T>(VALIDATOR_COUNT);

		// Give the validators reputation, this is equivalent to a heartbeat interval having passed
		for validator_id in &as_validators::<T>(&accounts) {
			<Pallet<T> as Store>::Reputations::insert(&validator_id, Reputation {
				online_credits: Pallet::<T>::online_credit_reward(),
				reputation_points: 1,
			});
		}
	}: _(RawOrigin::Signed(accounts[b as usize].clone()))
	verify {
		let validator_id: T::ValidatorId = accounts[b as usize].clone().into();
		let expected_credits_after_two_heartbeat_intervals =
			Pallet::<T>::online_credit_reward() * 2u32.into();
		let reputation_for_validator = Pallet::<T>::reputation(&validator_id);
		assert_eq!(
			reputation_for_validator.online_credits,
			expected_credits_after_two_heartbeat_intervals
		);
	}

	on_initialize {
		let x in 1 .. 1_000;
		let max_block = x as u64;

		let mut eighty_percent_of_validators : u32 = (VALIDATOR_COUNT as u32 * 8) / 10;
		let mut dead_validators: Vec<T::ValidatorId> = vec![];
		let accounts = initialise_pallet::<T>(eighty_percent_of_validators);

		// Give the validators reputation, this is equivalent to a heartbeat interval having passed
		for validator_id in &as_validators::<T>(&accounts) {
			<Pallet<T> as Store>::Reputations::insert(&validator_id, Reputation {
				online_credits: Pallet::<T>::online_credit_reward(),
				reputation_points: 1,
			});
		}
	}: {
		for c in 0..max_block {
			Pallet::<T>::on_initialize((c as u32).into());
		}
	}

	update_accrual_ratio {
		let accounts = initialise_pallet::<T>(VALIDATOR_COUNT);
		let b in 200 .. 1_000;
	} : _(RawOrigin::Root, 2, b.into())

	// check_liveness {
	// 	let accounts = initialise_pallet::<T>(VALIDATOR_COUNT);
	// 	// All of the VALIDATOR_COUNT will be penalised, this wouldn't occur but for benchmarking
	// 	// here it is
	// } : {
	// 	Pallet::<T>::check_liveness();
	// }
	// verify {
	// 	for validator_id in &as_validators::<T>(&accounts) {
	// 		let reputation_for_validator = Pallet::<T>::reputation(&validator_id);
	// 		assert_eq!(
	// 			reputation_for_validator.reputation_points,
	// 			0 - expected_reputation_penalty::<T>()
	// 		);
	// 	}
	// }

	// check_liveness_at_eighty_percent {
	// 	let accounts = initialise_pallet::<T>(VALIDATOR_COUNT);

	// 	let mut eighty_percent_of_validators : i32 = (VALIDATOR_COUNT as i32 * 8) / 10;
	// 	let mut dead_validators: Vec<T::ValidatorId> = vec![];
	// 	<Pallet<T> as Store>::AwaitingHeartbeats::translate(|validator_id, _awaiting: bool| {
	// 		eighty_percent_of_validators -= 1;
	// 		if eighty_percent_of_validators < 0 {
	// 			dead_validators.push(validator_id);
	// 		}
	// 		Some(eighty_percent_of_validators < 0)
	// 	});

	// } : {
	// 	Pallet::<T>::check_liveness();
	// }
	// verify {
	// 	for validator_id in dead_validators {
	// 		let reputation_for_validator = Pallet::<T>::reputation(&validator_id);
	// 		assert_eq!(
	// 			reputation_for_validator.reputation_points,
	// 			0 - expected_reputation_penalty::<T>()
	// 		);
	// 	}
	// }
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::Test;
	use frame_support::assert_ok;
	use sp_io::TestExternalities;

	pub fn new_test_ext() -> TestExternalities {
		let t = frame_system::GenesisConfig::default()
			.build_storage::<Test>()
			.unwrap();
		TestExternalities::new(t)
	}

	#[test]
	fn bench_heartbeat() {
		new_test_ext().execute_with(|| {
			assert_ok!(test_benchmark_heartbeat::<Test>());
			assert_ok!(test_benchmark_next_heartbeat::<Test>());
		});
	}

	#[test]
	fn bench_check_liveness() {
		new_test_ext().execute_with(|| {
			assert_ok!(test_benchmark_check_liveness::<Test>());
		});
	}
}
impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
