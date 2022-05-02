//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_runtime_benchmark_utilities::BenchmarkDefault;
use frame_benchmarking::{account, benchmarks_instance_pallet, whitelist_account};
use frame_support::{dispatch::UnfilteredDispatchable, traits::IsType};
use frame_system::RawOrigin;
use pallet_cf_online::Call as OnlineCall;
use pallet_cf_validator::CurrentAuthorities;
use sp_std::convert::TryInto;

const SEED: u32 = 0;

type SignatureFor<T, I> = <<T as Config<I>>::TargetChain as ChainCrypto>::ThresholdSignature;

fn add_online_validators<T, I>(validators: I)
where
	T: frame_system::Config + pallet_cf_validator::Config + pallet_cf_online::Config,
	I: Clone + Iterator<Item = <T as Chainflip>::ValidatorId>,
{
	CurrentAuthorities::<T>::put(validators.clone().collect::<Vec<_>>());
	for validator_id in validators {
		let account_id = validator_id.into_ref();
		whitelist_account!(account_id);
		OnlineCall::<T>::heartbeat()
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

		add_online_validators::<T, _>(all_accounts);

		let (_, ceremony_id) = Pallet::<T, I>::request_signature(PayloadFor::<T, I>::benchmark_default());
		let signature = SignatureFor::<T, I>::benchmark_default();
	} : _(RawOrigin::None, ceremony_id, signature)
	verify {
		let last_event = frame_system::Pallet::<T>::events().pop().unwrap().event;
		let expected: <T as crate::Config<I>>::Event = Event::<T, I>::ThresholdDispatchComplete(ceremony_id, Ok(())).into();
		assert_eq!(last_event, *expected.into_ref());
	}
	report_signature_failed {
		let a in 1 .. 100;
		let all_accounts = (0..150).map(|i| account::<<T as Chainflip>::ValidatorId>("signers", i, SEED));

		add_online_validators::<T, _>(all_accounts);

		let (_, ceremony_id) = Pallet::<T, I>::request_signature(PayloadFor::<T, I>::benchmark_default());

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
}

// NOTE: Test suite not included because of dependency mismatch between benchmarks and mocks.
