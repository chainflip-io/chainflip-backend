//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::benchmarking_value::BenchmarkValue;
use frame_benchmarking::{account, benchmarks_instance_pallet, whitelist_account};
use frame_support::{dispatch::UnfilteredDispatchable, traits::IsType};
use frame_system::RawOrigin;
use pallet_cf_online::Call as OnlineCall;
use pallet_cf_validator::CurrentAuthorities;
use sp_std::convert::TryInto;

const SEED: u32 = 0;

type SignatureFor<T, I> = <<T as Config<I>>::TargetChain as ChainCrypto>::ThresholdSignature;

fn add_authorities<T, I>(authorities: I)
where
	T: frame_system::Config + pallet_cf_validator::Config + pallet_cf_online::Config,
	I: Clone + Iterator<Item = <T as Chainflip>::ValidatorId>,
{
	CurrentAuthorities::<T>::put(authorities.clone().collect::<Vec<_>>());
	for validator_id in authorities {
		let account_id = validator_id.into_ref();
		whitelist_account!(account_id);
		OnlineCall::<T>::heartbeat {}
			.dispatch_bypass_filter(RawOrigin::Signed(account_id.clone()).into())
			.unwrap();
	}
}

benchmarks_instance_pallet! {
	where_clause {
		where
			T: frame_system::Config
			+ pallet_cf_validator::Config
			+ pallet_cf_online::Config
	}

	signature_success {
		let all_accounts = (0..150).map(|i| account::<<T as Chainflip>::ValidatorId>("signers", i, SEED));

		add_authorities::<T, _>(all_accounts);

		let (_, ceremony_id) = Pallet::<T, I>::request_signature(PayloadFor::<T, I>::benchmark_value());
		let signature = SignatureFor::<T, I>::benchmark_value();
	} : _(RawOrigin::None, ceremony_id, signature)
	verify {
		let last_event = frame_system::Pallet::<T>::events().pop().unwrap().event;
		let expected: <T as crate::Config<I>>::Event = Event::<T, I>::ThresholdDispatchComplete(ceremony_id, Ok(())).into();
		assert_eq!(last_event, *expected.into_ref());
	}
	report_signature_failed {
		let a in 1 .. 100;
		let all_accounts = (0..150).map(|i| account::<<T as Chainflip>::ValidatorId>("signers", i, SEED));

		add_authorities::<T, _>(all_accounts);

		let (_, ceremony_id) = Pallet::<T, I>::request_signature(PayloadFor::<T, I>::benchmark_value());

		let mut threshold_set = PendingCeremonies::<T, I>::get(ceremony_id).unwrap().remaining_respondents.into_iter();

		let reporter = threshold_set.next().unwrap();
		let offenders = BTreeSet::from_iter(threshold_set.take(a as usize))
			.try_into()
			.expect("Benchmark threshold should not exceed BTreeSet bounds");
	} : _(RawOrigin::Signed(reporter.into()), ceremony_id, offenders)
	on_initialize {} : {}
	determine_offenders {
		let a in 1 .. 200;

		// Worst case: 1/2 of participants failed.
		let blame_counts = (0..a / 2)
			.map(|i| account::<<T as Chainflip>::ValidatorId>("signers", i, SEED))
			.map(|id| (id, a))
			.collect();

		let completed_response_context = CeremonyContext::<T, I> {
			remaining_respondents:Default::default(),
			blame_counts,
			participant_count:a,
			_phantom: Default::default()
		};
	} : {
		let _ = completed_response_context.offenders();
	}
	set_threshold_signature_timeout {
		let old_timeout: T::BlockNumber = 5u32.into();
		ThresholdSignatureResponseTimeout::<T, I>::put(old_timeout);
		let new_timeout: T::BlockNumber = old_timeout + 1u32.into();
		let call = Call::<T, I>::set_threshold_signature_timeout {
			new_timeout
		};
	} : { call.dispatch_bypass_filter(<T as Config<I>>::EnsureGovernance::successful_origin())? }
	verify {
		assert_eq!(ThresholdSignatureResponseTimeout::<T, I>::get(), new_timeout);
	}
}

// NOTE: Test suite not included because of dependency mismatch between benchmarks and mocks.
