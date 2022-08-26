use cf_traits::{EpochInfo, SignerNomination};
use pallet_cf_validator::{
	CurrentAuthorities, CurrentEpoch, EpochAuthorityCount, HistoricalAuthorities,
};
use sp_runtime::AccountId32;
use state_chain_runtime::{chainflip::RandomSignerNomination, Runtime, Validator};

#[test]
fn signer_nomination_respects_epoch() {
	super::genesis::default().build().execute_with(|| {
		let genesis_authorities = Validator::current_authorities();
		let genesis_epoch = Validator::epoch_index();

		assert_eq!(genesis_authorities, HistoricalAuthorities::<Runtime>::get(genesis_epoch));
		assert_eq!(
			genesis_authorities.len() as u32,
			EpochAuthorityCount::<Runtime>::get(genesis_epoch).unwrap()
		);

		assert!(RandomSignerNomination::threshold_nomination_with_seed((), genesis_epoch)
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
		let new_nominees = RandomSignerNomination::threshold_nomination_with_seed((), next_epoch)
			.expect("Non empty set, no one banned");
		assert!(new_nominees.iter().all(|n| !genesis_authorities.contains(n)));
		assert!(new_nominees.iter().all(|n| new_authorities.contains(n)));

		// asking to sign at old epoch still works
		let old_nominees =
			RandomSignerNomination::threshold_nomination_with_seed((), genesis_epoch)
				.expect("Non empty, no one banned");
		assert!(old_nominees.iter().all(|n| genesis_authorities.contains(n)));

		// double the number of authorities should mean we have a higher threshold
		assert!(new_nominees.len() > old_nominees.len());
	})
}
