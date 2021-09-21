//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, benchmarks_instance_pallet, impl_benchmark_test_suite};
use frame_support::traits::OnInitialize;
use frame_system::RawOrigin;
use sp_std::{boxed::Box, vec, vec::Vec};

const BLOCK_NUMBER: u32 = 100;

#[allow(unused)]
use crate::Pallet as Emissions;

benchmarks! {
	on_initialize {
		let x in 1 .. 1_000;
		let leaves = x as u64;
	}: {
		for b in 0..leaves {
			Emissions::<T>::on_initialize((b as u32).into());
		}
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
