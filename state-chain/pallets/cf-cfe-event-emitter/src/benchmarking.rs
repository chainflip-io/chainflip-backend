#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::benchmarks;
use sp_std::{vec, vec::Vec};

benchmarks! {

	remove_events_for_block {

		let event = CfeEvent::<T>::EthKeygenRequest(KeygenRequest::<T> {ceremony_id: 0, epoch_index: 0, participants: Default::default()});

		CfeEvents::<T>::insert::<BlockNumberFor<T>, Vec<CfeEvent<T>>>(0u32.into(), vec![
			event.clone(),
			event.clone(),
			event.clone(),
		]);
		CfeEvents::<T>::insert::<BlockNumberFor<T>, Vec<CfeEvent<T>>>(1u32.into(), vec![
			event.clone(),
			event.clone(),
			event.clone(),
		]);
		CfeEvents::<T>::insert::<BlockNumberFor<T>, Vec<CfeEvent<T>>>(2u32.into(), vec![
			event.clone(),
			event.clone(),
			event.clone(),
		]);
	}: {
		CfeEvents::<T>::remove::<BlockNumberFor<T>>(0u32.into());
	}
	verify {
		assert_eq!(CfeEvents::<T>::get::<BlockNumberFor<T>>(0u32.into()), None);
	}
}
