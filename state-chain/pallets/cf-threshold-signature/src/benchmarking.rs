#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::{benchmarking_value::BenchmarkValue, ChainCrypto};
use cf_primitives::GENESIS_EPOCH;
use cf_runtime_utilities::StorageDecodeVariant;
use cf_traits::{
	AccountRoleRegistry, Chainflip, CurrentEpochIndex, KeyRotationStatusOuter, ThresholdSigner,
};
use frame_benchmarking::{account, v2::*, whitelist_account, whitelisted_caller};
use frame_support::{
	assert_ok,
	traits::{IsType, OnInitialize, OnNewAccount, UnfilteredDispatchable},
};
use frame_system::RawOrigin;
use pallet_cf_validator::CurrentAuthorities;

const SEED: u32 = 0;
const CEREMONY_ID: u64 = 1;

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

/// Generate an authority set
fn generate_authority_set<T: Config<I>, I: 'static>(
	set_size: u32,
	caller: T::ValidatorId,
) -> BTreeSet<T::ValidatorId> {
	let mut authority_set: BTreeSet<T::ValidatorId> = BTreeSet::new();
	// make room for the caller
	for i in 0..set_size.checked_sub(1).expect("set size should be at least 1") {
		let validator_id = account("doogle", i, 0);
		authority_set.insert(validator_id);
	}
	authority_set.insert(caller);
	authority_set
}

#[instance_benchmarks( where
	T: frame_system::Config
	+ pallet_cf_validator::Config
	+ pallet_cf_reputation::Config
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn signature_success() {
		// Note: this benchmark does not include the cost of the dispatched extrinsic.
		let all_accounts =
			(0..150).map(|i| account::<<T as Chainflip>::ValidatorId>("signers", i, SEED));

		add_authorities::<T, _>(all_accounts);

		let request_id = <Pallet<T, I> as ThresholdSigner<_>>::request_signature(
			PayloadFor::<T, I>::benchmark_value(),
		);
		let ceremony_id = 1;
		let signature = SignatureFor::<T, I>::benchmark_value();

		#[extrinsic_call]
		signature_success(RawOrigin::None, ceremony_id, signature);

		let last_event = frame_system::Pallet::<T>::events().pop().unwrap().event;
		let expected: <T as crate::Config<I>>::RuntimeEvent =
			Event::<T, I>::ThresholdSignatureSuccess { request_id, ceremony_id }.into();
		assert_eq!(last_event, *expected.into_ref());
	}

	#[benchmark]
	fn report_signature_failed(a: Linear<1, 100>) {
		let all_accounts =
			(0..150).map(|i| account::<<T as Chainflip>::ValidatorId>("signers", i, SEED));

		add_authorities::<T, _>(all_accounts);

		let _request_id = <Pallet<T, I> as ThresholdSigner<_>>::request_signature(
			PayloadFor::<T, I>::benchmark_value(),
		);
		let ceremony_id = 1;

		let mut threshold_set = PendingCeremonies::<T, I>::get(ceremony_id)
			.unwrap()
			.remaining_respondents
			.into_iter();

		let reporter = threshold_set.next().unwrap();
		let account: T::AccountId = reporter.clone().into();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&account);
		assert_ok!(<T as Chainflip>::AccountRoleRegistry::register_as_validator(&account));
		let offenders = BTreeSet::from_iter(threshold_set.take(a as usize));

		#[extrinsic_call]
		report_signature_failed(RawOrigin::Signed(reporter.into()), ceremony_id, offenders);
	}

	#[benchmark]
	fn set_threshold_signature_timeout() {
		let old_timeout: BlockNumberFor<T> = 5u32.into();
		ThresholdSignatureResponseTimeout::<T, I>::put(old_timeout);
		let new_timeout: BlockNumberFor<T> = old_timeout + 1u32.into();
		let call = Call::<T, I>::set_threshold_signature_timeout { new_timeout };

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(
				<T as Chainflip>::EnsureGovernance::try_successful_origin().unwrap()
			));
		}

		assert_eq!(ThresholdSignatureResponseTimeout::<T, I>::get(), new_timeout);
	}

	#[benchmark]
	fn on_initialize_keygen_failure_no_pending_sig_ceremonies(b: Linear<1, 100>) {
		let current_block: BlockNumberFor<T> = 0u32.into();
		KeygenResolutionPendingSince::<T, I>::put(current_block);
		let caller: T::AccountId = whitelisted_caller();
		let keygen_participants = generate_authority_set::<T, I>(150, caller.clone().into());
		let blamed = generate_authority_set::<T, I>(b, caller.into());
		let mut keygen_response_status =
			KeygenResponseStatus::<T, I>::new(keygen_participants.clone());

		for validator_id in &keygen_participants {
			keygen_response_status.add_failure_vote(validator_id, blamed.clone());
		}

		PendingKeyRotation::<T, I>::put(KeyRotationStatus::<T, I>::AwaitingKeygen {
			ceremony_id: CEREMONY_ID,
			keygen_participants: keygen_participants.into_iter().collect(),
			response_status: keygen_response_status,
			new_epoch_index: GENESIS_EPOCH,
		});
		let block_number: BlockNumberFor<T> = 5u32.into();
		let empty_vec: Vec<CeremonyId> = vec![];
		CeremonyRetryQueues::<T, I>::insert(block_number, empty_vec);
		#[block]
		{
			Pallet::<T, I>::on_initialize(5u32.into());
		}

		assert!(matches!(
			<Pallet::<T, I> as KeyRotator>::status(),
			AsyncResult::Ready(KeyRotationStatusOuter::Failed(..))
		));
	}

	#[benchmark]
	fn on_initialize_keygen_success_no_pending_sig_ceremonies() {
		let current_block: BlockNumberFor<T> = 0u32.into();
		KeygenResolutionPendingSince::<T, I>::put(current_block);
		let caller: T::AccountId = whitelisted_caller();
		let keygen_participants = generate_authority_set::<T, I>(150, caller.into());
		let mut keygen_response_status =
			KeygenResponseStatus::<T, I>::new(keygen_participants.clone());

		for validator_id in &keygen_participants {
			keygen_response_status
				.add_success_vote(validator_id, AggKeyFor::<T, I>::benchmark_value());
		}

		PendingKeyRotation::<T, I>::put(KeyRotationStatus::<T, I>::AwaitingKeygen {
			ceremony_id: CEREMONY_ID,
			keygen_participants: keygen_participants.into_iter().collect(),
			response_status: keygen_response_status,
			new_epoch_index: GENESIS_EPOCH,
		});
		let block_number: BlockNumberFor<T> = 5u32.into();
		let empty_vec: Vec<CeremonyId> = vec![];
		CeremonyRetryQueues::<T, I>::insert(block_number, empty_vec);
		#[block]
		{
			Pallet::<T, I>::on_initialize(5u32.into());
		}

		assert_eq!(
			PendingKeyRotation::<T, I>::decode_variant(),
			Some(KeyRotationStatusVariant::AwaitingKeygenVerification),
		);
	}

	#[benchmark]
	fn on_initialize_no_keygen(a: Linear<10, 150>, r: Linear<0, 50>) {
		// a: number of authorities, r: number of retries
		let key = <T::TargetChainCrypto as ChainCrypto>::AggKey::benchmark_value();
		let current_epoch = CurrentEpochIndex::<T>::get();
		<Pallet<T, I> as KeyProvider<T::TargetChainCrypto>>::set_key(key, current_epoch);
		CurrentAuthorities::<T>::put(BTreeSet::<<T as Chainflip>::ValidatorId>::new());

		// These attempts will fail because there are no authorities to do the signing.
		for _ in 0..r {
			Pallet::<T, I>::new_ceremony_attempt(RequestInstruction::new(
				1,
				1,
				PayloadFor::<T, I>::benchmark_value(),
				RequestType::SpecificKey(key, current_epoch),
			));
		}

		assert_eq!(
			CeremonyRetryQueues::<T, I>::decode_len(
				ThresholdSignatureResponseTimeout::<T, I>::get()
			)
			.unwrap_or_default(),
			r as usize,
		);

		// Now we add the authorities
		add_authorities::<T, _>(
			(0..a).map(|i| account::<<T as Chainflip>::ValidatorId>("signers", i, SEED)),
		);

		#[block]
		{
			Pallet::<T, I>::on_initialize(ThresholdSignatureResponseTimeout::<T, I>::get());
		}

		assert_eq!(
			CeremonyRetryQueues::<T, I>::decode_len(
				ThresholdSignatureResponseTimeout::<T, I>::get()
			)
			.unwrap_or_default(),
			0_usize,
		);
	}

	// The above benchmark results in retries without any blamed parties. This benchmark allows us
	// to account for blame reports.
	#[benchmark]
	fn report_offenders(o: Linear<1, 100>) {
		let offenders = (0..o)
			.map(|i| account::<<T as Chainflip>::ValidatorId>("offender", i, SEED))
			.collect::<Vec<_>>();
		#[block]
		{
			<T as Config<I>>::OffenceReporter::report_many(
				PalletOffence::ParticipateSigningFailed,
				offenders,
			);
		}
	}

	#[benchmark]
	fn report_keygen_outcome() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		T::AccountRoleRegistry::register_as_validator(&caller).unwrap();

		let keygen_participants = generate_authority_set::<T, I>(150, caller.clone().into());
		PendingKeyRotation::<T, I>::put(KeyRotationStatus::<T, I>::AwaitingKeygen {
			ceremony_id: CEREMONY_ID,
			keygen_participants: keygen_participants.clone().into_iter().collect(),
			response_status: KeygenResponseStatus::<T, I>::new(keygen_participants),
			new_epoch_index: GENESIS_EPOCH,
		});

		// Submit a key that doesn't verify the signature. This is approximately the same cost as
		// success at time of writing. But is much easier to write, and we might add slashing, which
		// would increase the cost of the failure. Making this test the more expensive of the two
		// paths, therefore ensuring we have a more conservative benchmark
		#[extrinsic_call]
		report_keygen_outcome(
			RawOrigin::Signed(caller),
			CEREMONY_ID,
			KeygenOutcomeFor::<T, I>::Ok(AggKeyFor::<T, I>::benchmark_value()),
		);

		assert!(matches!(
			PendingKeyRotation::<T, I>::get().unwrap(),
			KeyRotationStatus::AwaitingKeygen { response_status, .. }
				if response_status.remaining_candidate_count() == 149
		))
	}

	#[benchmark]
	fn on_keygen_verification_result() {
		let caller: T::AccountId = whitelisted_caller();
		let agg_key = AggKeyFor::<T, I>::benchmark_value();
		let keygen_participants = generate_authority_set::<T, I>(150, caller.into());
		let request_id = Pallet::<T, I>::trigger_keygen_verification(
			CEREMONY_ID,
			agg_key,
			keygen_participants.into_iter().collect(),
			2,
		);
		<Pallet<T, I> as ThresholdSigner<T::TargetChainCrypto>>::insert_signature(
			request_id,
			SignatureFor::<T, I>::benchmark_value(),
		);
		let call = Call::<T, I>::on_keygen_verification_result {
			keygen_ceremony_id: CEREMONY_ID,
			threshold_request_id: request_id,
			new_public_key: agg_key,
		};
		let origin = EnsureThresholdSigned::<T, I>::try_successful_origin().unwrap();
		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin.into()));
		}

		assert!(matches!(
			PendingKeyRotation::<T, I>::get().unwrap(),
			KeyRotationStatus::KeygenVerificationComplete { new_public_key }
				if new_public_key == agg_key
		))
	}

	#[benchmark]
	fn set_keygen_response_timeout() {
		let old_timeout: BlockNumberFor<T> = 5u32.into();
		KeygenResponseTimeout::<T, I>::put(old_timeout);
		let new_timeout: BlockNumberFor<T> = old_timeout + 1u32.into();
		// ensure it's a different value for most expensive path.
		let call = Call::<T, I>::set_keygen_response_timeout { new_timeout };
		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		assert_eq!(KeygenResponseTimeout::<T, I>::get(), new_timeout);
	}
	// NOTE: Test suite not included because of dependency mismatch between benchmarks and mocks.
}
