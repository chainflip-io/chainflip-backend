#![cfg(feature = "runtime-benchmarks")]

use frame_benchmarking::benchmarks;
use pallet_session::*;

pub struct Pallet<T: Config>(pallet_session::Pallet<T>);
pub trait Config: pallet_session::Config + pallet_session::historical::Config {}

benchmarks! {
	set_keys {
	}: {}
	purge_keys {
	}: {}
	#[extra]
	check_membership_proof_current_session {
	}: {
	}
	#[extra]
	check_membership_proof_historical_session {
	}: {
	}
}
