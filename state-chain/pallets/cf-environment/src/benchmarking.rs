//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::benchmarks;
use frame_support::{dispatch::UnfilteredDispatchable, traits::EnsureOrigin};

#[allow(unused)]
use crate::Pallet;

benchmarks! {}
