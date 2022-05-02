use crate::*;

/// Defines a method for resolving auctions.
pub trait AuctionResolver<T: Chainflip> {
	type AuctionParameters;
	type AuctionContext;
	type Error: Into<DispatchError>;

	fn resolve_auction(
		auction_parameters: Self::AuctionParameters,
		auction_context: Self::AuctionContext,
		auction_candidates: Vec<(T::ValidatorId, T::Amount)>,
	) -> Result<AuctionOutcome<T>, Self::Error>;
}

/// A simple auction resolver that takes as many participants as possible withing some min/max range
/// of set sizes.
pub struct ResolverV1<T: Config>(PhantomData<T>);

#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode)]
pub struct AuctionParametersV1 {
	pub min_size: u32,
	pub max_size: u32,
	pub authority_to_backup_ratio: u32,
	pub percentage_of_backup_nodes_in_emergency: u32,
}

pub struct AuctionContextV1 {
	pub is_emergency: bool,
}

impl<T: Config> AuctionResolver<T> for ResolverV1<T> {
	type AuctionParameters = AuctionParametersV1;
	type AuctionContext = AuctionContextV1;
	type Error = Error<T>;

	fn resolve_auction(
		auction_parameters: Self::AuctionParameters,
		auction_context: Self::AuctionContext,
		mut auction_candidates: Vec<(T::ValidatorId, T::Amount)>,
	) -> Result<AuctionOutcome<T>, Self::Error> {
		let (min_number_of_authorities, max_number_of_authorities) =
			(auction_parameters.min_size, auction_parameters.max_size);
		let number_of_bidders = auction_candidates.len() as u32;

		ensure!(number_of_bidders >= min_number_of_authorities, {
			log::error!(
				"[cf-auction] insufficient bidders to proceed. {} < {}",
				number_of_bidders,
				min_number_of_authorities
			);
			Error::<T>::NotEnoughBidders
		});

		auction_candidates.sort_unstable_by_key(|&(_, amount)| Reverse(amount));

		let mut target_authority_set_size =
			min(max_number_of_authorities, number_of_bidders) as usize;
		let mut next_authority_set: Vec<_> =
			auction_candidates.iter().take(target_authority_set_size as usize).collect();

		if auction_context.is_emergency {
			// We are interested in only have `PercentageOfBackupValidatorsInEmergency`
			// of existing BVs in the validating set.  We ensure this by using the last
			// MAB to understand who were BVs and ensure we only maintain the required
			// amount under this level to avoid a superminority of low collateralised
			// nodes.

			// NOTE DAN: Leaving this here even though I'm pretty sure it doesn't work as intended.
			// Instead of including at most some percentage of the existing BVs, it might include
			// more, or less, depending on a bunch of factors. Having said that, it's not critical
			// and will soon be superceded by a new method aka dynamic set sizes.

			// NOTE DAN: This is the size of the group if we cut off at the previous bond.
			if let Some(new_target_authority_set_size) =
				next_authority_set.iter().position(|(_, amount)| amount < &T::EpochInfo::bond())
			{
				// NOTE DAN: This is wrong since (a) the new_target_authority_set_size already
				// might contain some of the previous backup validators if their stake was above the
				// previous bond. Also (b) there are likely some backup validators that are not
				// included in the next_authority_set and therefore unaccounted-for in this
				// calculation.
				let number_of_existing_backup_nodes = (target_authority_set_size -
					new_target_authority_set_size) as u32 *
					(auction_parameters.authority_to_backup_ratio - 1) /
					auction_parameters.authority_to_backup_ratio;

				let number_of_backup_nodes_to_be_included = (number_of_existing_backup_nodes
					as u32)
					.saturating_mul(auction_parameters.percentage_of_backup_nodes_in_emergency) /
					100;

				target_authority_set_size =
					new_target_authority_set_size + number_of_backup_nodes_to_be_included as usize;

				next_authority_set.truncate(target_authority_set_size);
			}
		}

		let winners: Vec<_> = next_authority_set
			.iter()
			.map(|(validator_id, _)| validator_id.clone())
			.collect();

		let losers: Vec<_> = auction_candidates
			.iter()
			.skip(target_authority_set_size as usize)
			.cloned()
			.collect();

		let bond = next_authority_set.last().map(|(_, bid)| *bid).unwrap_or_default();

		Ok(AuctionOutcome { winners, losers, bond })
	}
}
