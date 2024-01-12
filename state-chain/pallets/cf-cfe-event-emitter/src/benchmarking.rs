#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::benchmarks;

benchmarks! {

	clear_events {
		let event = CfeEvent::<T>::EthKeygenRequest(KeygenRequest::<T> {ceremony_id: 0, epoch_index: 0, participants: Default::default()});

		CfeEvents::<T>::append(event.clone());
		CfeEvents::<T>::append(event.clone());
		CfeEvents::<T>::append(event);

	}: {
		CfeEvents::<T>::kill();
	}
	verify {
		assert!(CfeEvents::<T>::get().is_empty());
	}
}
