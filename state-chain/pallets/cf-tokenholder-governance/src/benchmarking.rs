//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, whitelisted_caller};

benchmarks! {
    on_initialize {}: {}
    submit_proposal {}: {}
    back_proposal {}: {}
}