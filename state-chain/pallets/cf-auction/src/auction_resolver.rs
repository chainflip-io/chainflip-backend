use crate::*;

/// Defines a method for resolving auctions.
pub trait AuctionResolver<T: Chainflip> {
	type AuctionParameters;
	type Error: Into<DispatchError>;

	fn resolve_auction(
		auction_parameters: &Self::AuctionParameters,
		auction_candidates: Vec<(T::ValidatorId, T::Amount)>,
	) -> Result<AuctionOutcome<T>, Self::Error>;
}

/// A simple auction resolver that takes as many participants as possible withing some min/max range
/// of set sizes.
pub struct ResolverV1<T: Config>(PhantomData<T>);

impl<T: Config> AuctionResolver<T> for ResolverV1<T> {
	type AuctionParameters = ActiveValidatorRange;
	type Error = Error<T>;

	fn resolve_auction(
		auction_parameters: &Self::AuctionParameters,
		mut auction_candidates: Vec<(T::ValidatorId, T::Amount)>,
	) -> Result<AuctionOutcome<T>, Self::Error> {
		let (min_number_of_validators, max_number_of_validators) = *auction_parameters;
		let number_of_bidders = auction_candidates.len() as u32;

		ensure!(number_of_bidders >= min_number_of_validators, {
			log::error!(
				"[cf-auction] insufficient bidders to proceed. {} < {}",
				number_of_bidders,
				min_number_of_validators
			);
			Error::<T>::NotEnoughBidders
		});

		auction_candidates.sort_unstable_by_key(|k| k.1);
		auction_candidates.reverse();

		let mut target_validator_group_size =
			min(max_number_of_validators, number_of_bidders) as usize;
		let mut next_validator_group: Vec<_> =
			auction_candidates.iter().take(target_validator_group_size as usize).collect();

		if T::EmergencyRotation::emergency_rotation_in_progress() {
			// We are interested in only have `PercentageOfBackupValidatorsInEmergency`
			// of existing BVs in the validating set.  We ensure this by using the last
			// MAB to understand who were BVs and ensure we only maintain the required
			// amount under this level to avoid a superminority of low collateralised
			// nodes.
			if let Some(new_target_validator_group_size) = next_validator_group
				.iter()
				.position(|(_, amount)| amount < &T::EpochInfo::bond())
			{
				let number_of_existing_backup_validators = (target_validator_group_size -
					new_target_validator_group_size) as u32 *
					(T::ActiveToBackupValidatorRatio::get() - 1) /
					T::ActiveToBackupValidatorRatio::get();

				let number_of_backup_validators_to_be_included =
					(number_of_existing_backup_validators as u32)
						.saturating_mul(T::PercentageOfBackupValidatorsInEmergency::get()) /
						100;

				target_validator_group_size = new_target_validator_group_size +
					number_of_backup_validators_to_be_included as usize;

				next_validator_group.truncate(target_validator_group_size);
			}
		}
		// let backup_group_size =
		// 	target_validator_group_size as u32 / T::ActiveToBackupValidatorRatio::get();

		let winners: Vec<_> = next_validator_group
			.iter()
			.map(|(validator_id, _)| validator_id.clone())
			.collect();

		let losers: Vec<_> = auction_candidates
			.iter()
			.skip(target_validator_group_size as usize)
			.cloned()
			.collect();

		let bond = next_validator_group.last().map(|(_, bid)| *bid).unwrap_or_default();

		Ok(AuctionOutcome { winners, losers, bond })
	}
}
