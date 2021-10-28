// //! Benchmarking setup for pallet-template
// #![cfg(feature = "runtime-benchmarks")]

// use super::*;

// use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, whitelisted_caller};
// use frame_system::RawOrigin;
// use sp_std::{boxed::Box, vec, vec::Vec};

// #[allow(unused)]
// use crate::Pallet as Auction;

// benchmarks! {
// 	// TODO: implement benchmark
// 	on_initialize {} : {}
// 	// TODO: implement benchmark
// 	update_validator_emission_inflation {} : {}
// 	// TODO: implement benchmark
// 	update_backup_validator_emission_inflation {} : {}
// }

// impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, benchmarks_instance_pallet, impl_benchmark_test_suite};
use frame_support::traits::OnInitialize;
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec, vec::Vec};

const BLOCK_NUMBER: u32 = 100;
const MINT_INTERVAL: u32 = 100;

#[allow(unused)]
use crate::Pallet as Emissions;

benchmarks! {
	update_backup_validator_emission_inflation {
		let b in 1 .. 1_000;
	}: _(RawOrigin::Root, b.into())
	update_validator_emission_inflation {
		let b in 1 .. 1_000;
	}: _(RawOrigin::Root, b.into())
	zero_reward {
		let x in 1 .. 1_000;
		let balance: T::FlipBalance = T::FlipBalance::from(0 as u32);
		ValidatorEmissionPerBlock::<T>::set(balance);
	} : {
		for b in 1..x {
			if b % MINT_INTERVAL == 0 {
				Emissions::<T>::on_initialize((b as u32).into());
			}
		}
	}
	no_rewards_minted {

	} : {
		Emissions::<T>::on_initialize((5 as u32).into());
	}
	rewards_minted {
		let x in 1 .. 1_000;
	}: {
		for b in 1..x {
			if b % MINT_INTERVAL == 0 {
				Emissions::<T>::on_initialize((b as u32).into());
			}
		}
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
