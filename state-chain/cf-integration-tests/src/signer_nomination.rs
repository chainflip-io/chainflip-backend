use cf_traits::{
	offence_reporting::OffenceReporter, EpochInfo, SingleSignerNomination,
	ThresholdSignerNomination,
};
use pallet_cf_threshold_signature::PalletOffence;
use pallet_cf_validator::{
	CurrentAuthorities, CurrentEpoch, EpochAuthorityCount, HistoricalAuthorities,
};
use sp_runtime::AccountId32;
use state_chain_runtime::{EthereumInstance, Reputation, Runtime, Validator};

type RuntimeThresholdSignerNomination =
	<Runtime as pallet_cf_threshold_signature::Config<EthereumInstance>>::ThresholdSignerNomination;

type RuntimeBroadcastsignerNomination =
	<Runtime as pallet_cf_broadcast::Config<EthereumInstance>>::BroadcastSignerNomination;

#[test]
fn threshold_signer_nomination_respects_epoch() {
	super::genesis::default().build().execute_with(|| {
		let genesis_authorities = Validator::current_authorities();
		let genesis_epoch = Validator::epoch_index();

		assert_eq!(genesis_authorities, HistoricalAuthorities::<Runtime>::get(genesis_epoch));
		assert_eq!(
			genesis_authorities.len() as u32,
			EpochAuthorityCount::<Runtime>::get(genesis_epoch).unwrap()
		);

		assert!(RuntimeThresholdSignerNomination::threshold_nomination_with_seed(
			(),
			genesis_epoch
		)
		.expect("Non empty set, no one is banned")
		.into_iter()
		.all(|n| genesis_authorities.contains(&n)));

		// simulate transition to next epoch
		let next_epoch = genesis_epoch + 1;
		CurrentEpoch::<Runtime>::put(next_epoch);

		// double the number of authorities, so we also have a different threshold size
		let new_authorities: Vec<_> = (0u8..(2 * genesis_authorities.len() as u8))
			.into_iter()
			.map(|i| AccountId32::from([i; 32]))
			.collect();
		CurrentAuthorities::<Runtime>::put(&new_authorities);
		HistoricalAuthorities::<Runtime>::insert(next_epoch, &new_authorities);
		EpochAuthorityCount::<Runtime>::insert(next_epoch, new_authorities.len() as u32);
		assert!(Validator::current_authorities()
			.into_iter()
			.all(|n| !genesis_authorities.contains(&n)));

		// asking to sign at new epoch works
		let new_nominees =
			RuntimeThresholdSignerNomination::threshold_nomination_with_seed((), next_epoch)
				.expect("Non empty set, no one banned");
		assert!(new_nominees.iter().all(|n| !genesis_authorities.contains(n)));
		assert!(new_nominees.iter().all(|n| new_authorities.contains(n)));

		// asking to sign at old epoch still works
		let old_nominees =
			RuntimeThresholdSignerNomination::threshold_nomination_with_seed((), genesis_epoch)
				.expect("Non empty, no one banned");
		assert!(old_nominees.iter().all(|n| genesis_authorities.contains(n)));

		// double the number of authorities should mean we have a higher threshold
		assert!(new_nominees.len() > old_nominees.len());
	})
}

fn test_not_nominated_for_offence<F: Fn(crate::AccountId)>(penalise: F) {
	let genesis_epoch = Validator::epoch_index();

	let node1 = Validator::current_authorities().first().unwrap().clone();

	penalise(node1.clone());

	for seed in 0..20 {
		assert!(!RuntimeThresholdSignerNomination::threshold_nomination_with_seed(
			seed,
			genesis_epoch,
		)
		.unwrap()
		.contains(&node1));
	}
}

#[test]
fn offline_nodes_are_not_nominated_for_threshold_signing() {
	super::genesis::default().build().execute_with(|| {
		test_not_nominated_for_offence(|node_id| {
			Reputation::penalise_offline_authorities(vec![node_id])
		});
	});
}

#[test]
fn nodes_who_failed_to_sign_excluded_from_threshold_nomination() {
	super::genesis::default().build().execute_with(|| {
		test_not_nominated_for_offence(|node_id| {
			Reputation::report_many(PalletOffence::ParticipateSigningFailed, &[node_id])
		});
	});
}

#[test]
fn offline_nodes_are_not_nominated_transaction_signing() {
	super::genesis::default().build().execute_with(|| {
		let node1 = Validator::current_authorities().first().unwrap().clone();

		Reputation::penalise_offline_authorities(vec![node1.clone()]);

		for seed in 0..20 {
			// no extra ids, excluded.
			assert_ne!(
				RuntimeBroadcastsignerNomination::nomination_with_seed(seed, &[]).unwrap(),
				node1
			);
		}
	});
}
