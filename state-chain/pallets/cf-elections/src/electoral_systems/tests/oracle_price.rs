use core::{cmp::min, default, iter::repeat, ops::RangeInclusive};
use std::collections::{BTreeMap, BTreeSet};

use enum_iterator::all;
use frame_system::BlockHash;

use crate::{
	electoral_systems::{
		mocks::{Check, TestSetup},
		oracle_price::{
			chainlink::*,
			consensus::OraclePriceConsensus,
			price::*,
			primitives::*,
			state_machine::{tests::MockTypes, *},
		},
		state_machine::{
			core::TypesFor,
			state_machine_es::{StatemachineElectoralSystem, StatemachineElectoralSystemTypes},
		},
	},
	register_checks, vote_storage, ConsensusVote, ConsensusVotes,
};

type ValidatorId = u16;

impl StatemachineElectoralSystemTypes for MockTypes {
	type ValidatorId = ValidatorId;
	type StateChainBlockNumber = u64;
	type OnFinalizeReturnItem = ();
	type VoteStorage = vote_storage::bitmap::Bitmap<ExternalChainStateVote<Self>>;
	type Statemachine = OraclePriceTracker<Self>;
	type ConsensusMechanism = OraclePriceConsensus<Self>;
	type ElectoralSettings = ();
}

type OraclePriceES = StatemachineElectoralSystem<MockTypes>;

const UP_TO_DATE_TIMEOUT: Seconds = Seconds(60);
const MAYBE_STALE_TIMEOUT: Seconds = Seconds(30);
const MINIMAL_PRICE_DEVIATION: BasisPoints = BasisPoints(100);
const SETTINGS: OraclePriceSettings = OraclePriceSettings {
	solana: ExternalChainSettings {
		up_to_date_timeout: UP_TO_DATE_TIMEOUT,
		maybe_stale_timeout: MAYBE_STALE_TIMEOUT,
		minimal_price_deviation: MINIMAL_PRICE_DEVIATION,
	},
	ethereum: ExternalChainSettings {
		up_to_date_timeout: UP_TO_DATE_TIMEOUT,
		maybe_stale_timeout: MAYBE_STALE_TIMEOUT,
		minimal_price_deviation: MINIMAL_PRICE_DEVIATION,
	},
};
const START_TIME: UnixTime = UnixTime { seconds: 1000 };

register_checks! {
	OraclePriceES {
		election_for_chain_ongoing_with_asset_status(pre, post, arg: (ExternalPriceChain, Option<BTreeMap<ChainlinkAssetPair, PriceStaleness>>)) {

			println!("state is now {:?}", post.unsynchronised_state);

			let (chain, asset_statuses) = arg;
			assert_eq!(
				asset_statuses,
				post.election_properties
					.values()
					.find(|query| query.chain == chain)
					.map(|query| query.assets.iter().map(|(asset, voting_conditions)| {
						// Note, this is very specific to how the voting conditions are set up currently,
						// if that's changed this code here has to be updated to account for it most likely.
						let price_status = match voting_conditions.len() {
							0 => PriceStaleness::MaybeStale,
							1 => PriceStaleness::Stale,
							2 => PriceStaleness::UpToDate,
							_ => panic!("unexpected number of voting conditions!")
						};
						(asset.clone(), price_status)
					})
					.collect()
			))
		},

		electoral_price_api_returns(pre, post, arg: BTreeMap<PriceAsset, Option<StatechainPrice>>) {
			todo!()
		}
	}
}

use ChainlinkAssetPair::*;

#[test]
fn es_creates_elections_based_on_staleness() {
	let default_prices: [(ChainlinkAssetPair, ChainlinkPrice); _] = [
		(BtcUsd, ChainlinkPrice::integer(120_000)),
		(EthUsd, ChainlinkPrice::integer(3_500)),
		(SolUsd, ChainlinkPrice::integer(170)),
		(UsdcUsd, ChainlinkPrice::integer(1)),
		(UsdtUsd, ChainlinkPrice::integer(1)),
	];

	use PriceStaleness::*;
	TestSetup::<OraclePriceES>::default()
		.with_unsynchronised_settings(SETTINGS)
		.build()
		.mutate_unsynchronized_state(|state| state.get_time.state.state = START_TIME)
		// on startup all assets on both solana and eth are in `MaybeStale` (we query for the latest
		// data the engines have) state
		.test_on_finalize(
			&vec![()],
			|_| {},
			vec![
				Check::<OraclePriceES>::election_for_chain_ongoing_with_asset_status((
					ExternalPriceChain::Solana,
					Some(all::<ChainlinkAssetPair>().zip(repeat(MaybeStale)).collect()),
				)),
				Check::<OraclePriceES>::election_for_chain_ongoing_with_asset_status((
					ExternalPriceChain::Ethereum,
					Some(all::<ChainlinkAssetPair>().zip(repeat(MaybeStale)).collect()),
				)),
			],
		)
		//  - For solana: no consensus
		//  - For ethereum: consensus where all 20 voters vote for the exact same prices +
		//    timestamps
		.expect_consensus_multi(vec![
			// Data for the Solana election, no votes
			(
				generate_votes(
					Default::default(),
					(0..20).collect(),
					default_prices.clone().into(),
					Default::default(),
					START_TIME,
				),
				None,
			),
			// Data for the Ethereum election, all 20 voters vote for the default price
			(
				generate_votes(
					(0..20).collect(),
					Default::default(),
					default_prices.clone().into(),
					Default::default(),
					START_TIME,
				),
				Some(
					default_prices
						.iter()
						.cloned()
						.map(|(asset, price)| {
							(
								asset,
								AssetResponse {
									timestamp: Aggregated::from_single_value(START_TIME),
									price: Aggregated::from_single_value(price),
								},
							)
						})
						.collect(),
				),
			),
		])
		// since we got prices for the secondary chain (Ethereum), the elections are:
		//  - Solana: MaybeStale (since we didn't get Solana prices yet)
		//  - Ethereum: UpToDate
		.test_on_finalize(
			&vec![()],
			|_| {},
			vec![
				Check::<OraclePriceES>::election_for_chain_ongoing_with_asset_status((
					ExternalPriceChain::Solana,
					Some(all::<ChainlinkAssetPair>().zip(repeat(MaybeStale)).collect()),
				)),
				Check::<OraclePriceES>::election_for_chain_ongoing_with_asset_status((
					ExternalPriceChain::Ethereum,
					Some(all::<ChainlinkAssetPair>().zip(repeat(UpToDate)).collect()),
				)),
			],
		)
		// We get consensus:
		// - Solana: newest prices
		.expect_consensus_multi(vec![
			// Data for the Solana election, no votes
			(
				generate_votes(
					Default::default(),
					(0..20).collect(),
					default_prices.clone().into(),
					Default::default(),
					START_TIME,
				),
				None,
			),
			// Data for the Ethereum election, all 20 voters vote for the default price
			(
				generate_votes(
					(0..20).collect(),
					Default::default(),
					default_prices.clone().into(),
					Default::default(),
					START_TIME,
				),
				Some(
					default_prices
						.iter()
						.cloned()
						.map(|(asset, price)| {
							(
								asset,
								AssetResponse {
									timestamp: Aggregated::from_single_value(START_TIME),
									price: Aggregated::from_single_value(price),
								},
							)
						})
						.collect(),
				),
			),
		]);

	// since we got prices for the secondary chain (Ethereum), the election
	// .test_on_finalize(
	// 	&vec![()],
	// 	|_| {},
	// 	vec![
	// 		Check::<OraclePriceES>::election_for_chain_ongoing_with_asset_status((
	// 			ExternalPriceChain::Solana,
	// 			Some(all::<ChainlinkAssetPair>().zip(repeat(UpToDate)).collect()),
	// 		)),
	// 		Check::<OraclePriceES>::election_for_chain_ongoing_with_asset_status((
	// 			ExternalPriceChain::Ethereum,
	// 			None,
	// 		)),
	// 	],
	// );
	// .expect_consensus(
	// 	generate_votes(
	// 		(0..20).collect(),
	// 		Default::default(),
	// 		Default::default(),
	// 		None,
	// 	),
	// 	Some((vec![TX_RECEIVED], None)),
	// );
}

//----------------------- utilities ------------------------
fn generate_votes(
	voters: BTreeSet<ValidatorId>,
	did_not_vote: BTreeSet<ValidatorId>,
	prices: BTreeMap<ChainlinkAssetPair, ChainlinkPrice>,
	price_ranges: BTreeMap<ChainlinkAssetPair, RangeInclusive<ChainlinkPrice>>,
	current_time: UnixTime,
) -> ConsensusVotes<OraclePriceES> {
	println!("Generate votes called");

	let quartile = min(1, voters.len() / 4) as u32;
	let half = min(1, voters.len() / 2);

	// we want to generate a distribution of prices such that we will get exactly the median price +
	// iqr that we input we distribute the votes linearly below and above the given price
	let votes: BTreeMap<ChainlinkAssetPair, BTreeMap<ValidatorId, ChainlinkPrice>> = prices
		.into_iter()
		.map(|(asset, price)| {
			let (step_below, step_above) = price_ranges
				.get(&asset)
				.map(|price_range| {
					(
						(&price - price_range.start().clone()) / quartile,
						(price_range.end() - price.clone()) / quartile,
					)
				})
				.unwrap_or(Default::default());

			let votes = voters
				.iter()
				.enumerate()
				.map(|(index, voter)| {
					let vote = if index < half {
						&price - (&step_below * ChainlinkPrice::integer(half - index))
					} else {
						&price + (&step_above * ChainlinkPrice::integer(index - half))
					};
					(voter.clone(), vote)
				})
				.collect();

			(asset, votes)
		})
		.collect();

	let mut by_voter: BTreeMap<
		ValidatorId,
		BTreeMap<ChainlinkAssetPair, (UnixTime, ChainlinkPrice)>,
	> = Default::default();
	for (validator, (asset, data)) in votes.into_iter().flat_map(|(asset, prices)| {
		prices
			.into_iter()
			.map(move |(validator, price)| (validator, (asset.clone(), (current_time, price))))
	}) {
		by_voter.entry(validator).or_default().insert(asset, data);
	}

	ConsensusVotes {
		votes: by_voter
			.into_iter()
			.map(|(voter, vote)| ConsensusVote {
				vote: Some(((), ExternalChainStateVote { price: vote })),
				validator_id: voter,
			})
			.chain(
				did_not_vote
					.clone()
					.into_iter()
					.map(|v| ConsensusVote { vote: None, validator_id: v }),
			)
			.collect(),
	}
}
