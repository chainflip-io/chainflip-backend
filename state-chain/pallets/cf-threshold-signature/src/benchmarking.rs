//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_runtime_benchmark_utilities::BenchmarkDefault;
use frame_benchmarking::{account, benchmarks_instance_pallet, impl_benchmark_test_suite};
use frame_system::RawOrigin;
use pallet_cf_validator::{ValidatorLookup, Validators};
use sp_std::convert::TryInto;

const SEED: u32 = 0;

type SignatureFor<T, I> = <<T as Config<I>>::TargetChain as ChainCrypto>::ThresholdSignature;

benchmarks_instance_pallet! {
	where_clause {
		where
			T: frame_system::Config
			+ pallet_cf_validator::Config
			+ pallet_session::Config<ValidatorId = <T as frame_system::Config>::AccountId>,
	}

	signature_success {
		let all_accounts = (0..150).map(|i| account::<T::AccountId>("signers", i, SEED));

		Validators::<T>::put(all_accounts.clone().collect::<Vec<_>>());
		for account in all_accounts.clone() {
			ValidatorLookup::<T>::insert(account, ());
		}

		let ceremony_id = Pallet::<T, I>::request_signature(<T::SigningContext as BenchmarkDefault>::benchmark_default());
		let signature = <SignatureFor<T, I> as BenchmarkDefault>::benchmark_default();
	} : _(RawOrigin::None, ceremony_id, signature)
	report_signature_failed {
		let a in 1 .. 100;
		let mut all_accounts = (0..150).map(|i| account::<T::AccountId>("signers", i, SEED));

		Validators::<T>::put(all_accounts.clone().collect::<Vec<_>>());
		for account in all_accounts.clone() {
			ValidatorLookup::<T>::insert(account, ());
		}
		let all_validator_ids = all_accounts.clone().map(<T as Chainflip>::ValidatorId::from);
		let offenders = BTreeSet::from_iter(all_validator_ids.take(a as usize))
			.try_into()
			.expect("Benchmark threshold should not exceed BTreeSet bounds");
		let signer = all_accounts.nth(a as usize).unwrap();

		let ceremony_id = Pallet::<T, I>::request_signature(<T::SigningContext as BenchmarkDefault>::benchmark_default());
	} : _(RawOrigin::Signed(signer), ceremony_id, offenders)
	on_initialize {} : {}
	determine_offenders {
		let a in 1 .. 200;

		// Worst case: 1/2 of participants failed.
		let blame_counts = (0..a / 2)
			.map(|i| account::<<T as Chainflip>::ValidatorId>("signers", i, SEED))
			.map(|id| (id, a))
			.collect();

		let completed_response_context = RequestContext::<T, I> {
			attempt: 0,
			retry_scheduled: true,
			remaining_respondents: Default::default(),
			blame_counts,
			participant_count: a,
			chain_signing_context: T::SigningContext::benchmark_default(),
		};
	} : {
		let _ = completed_response_context.offenders();
	}
}

// impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
