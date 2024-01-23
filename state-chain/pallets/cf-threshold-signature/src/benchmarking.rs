//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::{benchmarking_value::BenchmarkValue, ChainCrypto};
use cf_traits::{AccountRoleRegistry, Chainflip, CurrentEpochIndex, ThresholdSigner};
use frame_benchmarking::{account, benchmarks_instance_pallet, whitelist_account};
use frame_support::{
	assert_ok,
	traits::{IsType, OnInitialize, OnNewAccount, UnfilteredDispatchable},
};
use frame_system::RawOrigin;
use pallet_cf_validator::CurrentAuthorities;

const SEED: u32 = 0;

type SignatureFor<T, I> = <<T as Config<I>>::TargetChainCrypto as ChainCrypto>::ThresholdSignature;

fn add_authorities<T, I>(authorities: I)
where
	T: frame_system::Config + pallet_cf_validator::Config + pallet_cf_reputation::Config,
	I: Clone + Iterator<Item = <T as Chainflip>::ValidatorId>,
{
	CurrentAuthorities::<T>::put(authorities.clone().collect::<BTreeSet<_>>());
	for validator_id in authorities {
		<T as frame_system::Config>::OnNewAccount::on_new_account(validator_id.into_ref());
		assert_ok!(<T as Chainflip>::AccountRoleRegistry::register_as_validator(
			&validator_id.clone().into()
		));
		let account_id = validator_id.into_ref();
		whitelist_account!(account_id);
		assert_ok!(pallet_cf_reputation::Pallet::<T>::heartbeat(
			RawOrigin::Signed(account_id.clone()).into()
		));
	}
}

benchmarks_instance_pallet! {
	where_clause {
		where
			T: frame_system::Config
			+ pallet_cf_validator::Config
			+ pallet_cf_reputation::Config,
	}

	// Note: this benchmark does not include the cost of the dispatched extrinsic.
	signature_success {
		let all_accounts = (0..150).map(|i| account::<<T as Chainflip>::ValidatorId>("signers", i, SEED));

		add_authorities::<T, _>(all_accounts);

		let request_id = <Pallet::<T, I> as ThresholdSigner<_>>::request_signature(PayloadFor::<T, I>::benchmark_value());
		let ceremony_id = 1;
		let signature = SignatureFor::<T, I>::benchmark_value();
	} : _(RawOrigin::None, ceremony_id, signature)
	verify {
		let last_event = frame_system::Pallet::<T>::events().pop().unwrap().event;
		let expected: <T as crate::Config<I>>::RuntimeEvent = Event::<T, I>::ThresholdSignatureSuccess{request_id, ceremony_id}.into();
		assert_eq!(last_event, *expected.into_ref());
	}
	report_signature_failed {
		let a in 1 .. 100;
		let all_accounts = (0..150).map(|i| account::<<T as Chainflip>::ValidatorId>("signers", i, SEED));

		add_authorities::<T, _>(all_accounts);

		let request_id = <Pallet::<T, I> as ThresholdSigner<_>>::request_signature(PayloadFor::<T, I>::benchmark_value());
		let ceremony_id = 1;

		let mut threshold_set = PendingCeremonies::<T, I>::get(ceremony_id).unwrap().remaining_respondents.into_iter();

		let reporter = threshold_set.next().unwrap();
		let account: T::AccountId = reporter.clone().into();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&account);
		assert_ok!(<T as Chainflip>::AccountRoleRegistry::register_as_validator(&account));
		let offenders = BTreeSet::from_iter(threshold_set.take(a as usize));
	} : _(RawOrigin::Signed(reporter.into()), ceremony_id, offenders)
	set_threshold_signature_timeout {
		let old_timeout: BlockNumberFor<T> = 5u32.into();
		ThresholdSignatureResponseTimeout::<T, I>::put(old_timeout);
		let new_timeout: BlockNumberFor<T> = old_timeout + 1u32.into();
		let call = Call::<T, I>::set_threshold_signature_timeout {
			new_timeout
		};
	} : { call.dispatch_bypass_filter(<T as Chainflip>::EnsureGovernance::try_successful_origin().unwrap())? }
	verify {
		assert_eq!(ThresholdSignatureResponseTimeout::<T, I>::get(), new_timeout);
	}

	on_initialize {
		// a: number of authorities
		let a in 10..150;
		// r: number of retries
		let r in 0..50;
		let key = <T::TargetChainCrypto as ChainCrypto>::AggKey::benchmark_value();
		let current_epoch = CurrentEpochIndex::<T>::get();
		T::KeyProvider::set_key(key, current_epoch);
		CurrentAuthorities::<T>::put(BTreeSet::<<T as Chainflip>::ValidatorId>::new());

		// These attempts will fail because there are no authorities to do the signing.
		for _ in 0..r {
			Pallet::<T, I>::new_ceremony_attempt(RequestInstruction::new(1, 1, PayloadFor::<T, I>::benchmark_value(), RequestType::SpecificKey(key, current_epoch)));
		}

		assert_eq!(
			CeremonyRetryQueues::<T, I>::decode_len(ThresholdSignatureResponseTimeout::<T, I>::get()).unwrap_or_default(),
			r as usize,
		);

		// Now we add the authorities
		add_authorities::<T, _>((0..a).map(|i| account::<<T as Chainflip>::ValidatorId>("signers", i, SEED)));

	}: {
		Pallet::<T, I>::on_initialize(ThresholdSignatureResponseTimeout::<T, I>::get())
	}
	verify {
		assert_eq!(
			CeremonyRetryQueues::<T, I>::decode_len(ThresholdSignatureResponseTimeout::<T, I>::get()).unwrap_or_default(),
			0_usize,
		);
	}
	// The above benchmark results in retries without any blamed parties. This benchmark allows us to account for
	// blame reports.
	report_offenders {
		let o in 1 .. 100;
		let offenders = (0..o)
			.map(|i| account::<<T as Chainflip>::ValidatorId>("offender", i, SEED))
			.collect::<Vec<_>>();
	}: {
		<T as Config<I>>::OffenceReporter::report_many(
			PalletOffence::ParticipateSigningFailed,
			offenders,
		);
	}
}

// NOTE: Test suite not included because of dependency mismatch between benchmarks and mocks.
