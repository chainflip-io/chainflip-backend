#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn clear_events() {
		let event = CfeEvent::<T>::EthKeygenRequest(KeygenRequest::<T> {
			ceremony_id: 0,
			epoch_index: 0,
			participants: Default::default(),
		});

		CfeEvents::<T>::append(event.clone());
		CfeEvents::<T>::append(event.clone());
		CfeEvents::<T>::append(event);

		#[block]
		{
			CfeEvents::<T>::kill();
		}

		assert!(CfeEvents::<T>::get().is_empty());
	}
}
