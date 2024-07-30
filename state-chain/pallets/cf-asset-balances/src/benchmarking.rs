#![cfg(feature = "runtime-benchmarks")]

use crate::{Config, Pallet};
use frame_benchmarking::v2::*;

// Keep this to avoid CI warnings about no benchmarks in the crate.
#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn noop() {
		#[block]
		{
			// Do nothing.
		}
	}
}
