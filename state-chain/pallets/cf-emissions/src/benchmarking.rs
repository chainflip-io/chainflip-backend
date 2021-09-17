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
	}: {
		for current_block in 1..BLOCK_NUMBER {
			Emissions::<T>::on_initialize((current_block).into());
		}
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
