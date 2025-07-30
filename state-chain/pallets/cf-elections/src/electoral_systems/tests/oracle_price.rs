// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use core::{
	cmp::max,
	iter::{self, repeat},
	ops::RangeInclusive,
};
use std::collections::{BTreeMap, BTreeSet};

use enum_iterator::all;

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
		state_machine::state_machine_es::{
			StatemachineElectoralSystem, StatemachineElectoralSystemTypes,
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
const TIME_STEP: Seconds = Seconds(15);

register_checks! {
	OraclePriceES {
		election_for_chain_ongoing_with_asset_status(_pre, post, arg: (ExternalPriceChain, Option<BTreeMap<ChainlinkAssetpair, PriceStatus>>)) {

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
							0 => PriceStatus::MaybeStale,
							1 => PriceStatus::Stale,
							2 => PriceStatus::UpToDate,
							_ => panic!("unexpected number of voting conditions!")
						};
						(*asset, price_status)
					})
					.collect()
			),
			"Wrong asset statuses: expected: {asset_statuses:?}, post state: {:?}", post.unsynchronised_state
		)
		},

		electoral_price_api_returns(_pre, post, prices: BTreeMap<ChainlinkAssetpair, (ChainlinkPrice, PriceStatus)>) {

			let prices : BTreeMap<_,_> = prices
				.clone()
				.into_iter()
				.map(|(asset, (price, staleness))| {
					(asset.to_price_unit().base_asset, (price, staleness))
				})
				.collect();

			let valid_assets = get_all_latest_prices_with_statechain_encoding(&post.unsynchronised_state)
				.into_iter()
				.filter_map(|(asset, (price, status))| {
					let price: StatechainPrice = Fraction::from_raw(price);
					let converted_price = convert_unit(
						price,
						PriceUnit { base_asset: PriceAsset::Fine, quote_asset: PriceAsset::Fine },
						PriceUnit { base_asset: asset, quote_asset: PriceAsset::Usdc }
					)?.convert()?;

					let required_price = &prices.get(&asset)?.0;
					let required_status = &prices.get(&asset)?.1;

					// we allow the round-tripped price to differ by the smallest unit
					assert!(
						required_price - converted_price.clone() <= Fraction(1u32.into()),
						"asset: {asset:?}, price actual: {converted_price:?}, expected: {required_price:?}, latest prices are {:?}, post state is: {:?}",
						get_all_latest_prices_with_statechain_encoding(&post.unsynchronised_state),
						post.unsynchronised_state
					);
					assert_eq!(status, *required_status);
					Some(asset)
				})
				.collect::<BTreeSet<_>>();

			assert_eq!(valid_assets, prices.into_keys().collect::<BTreeSet<_>>());
		}
	}
}

use ChainlinkAssetpair::*;

#[test]
fn election_lifecycle() {
	let default_prices: [(ChainlinkAssetpair, ChainlinkPrice); _] = [
		(BtcUsd, ChainlinkPrice::integer(120_000)),
		(EthUsd, ChainlinkPrice::integer(3_500)),
		(SolUsd, ChainlinkPrice::integer(170)),
		(UsdcUsd, ChainlinkPrice::integer(1)),
		(UsdtUsd, ChainlinkPrice::integer(1)),
	];
	let prices1: BTreeMap<ChainlinkAssetpair, ChainlinkPrice> = default_prices.clone().into();
	let prices2: BTreeMap<ChainlinkAssetpair, ChainlinkPrice> = default_prices
		.iter()
		.cloned()
		.map(|(asset, price)| (asset, (price * BasisPoints(10005).to_fraction()).unwrap()))
		.collect();
	let prices3: BTreeMap<ChainlinkAssetpair, ChainlinkPrice> = default_prices
		.iter()
		.cloned()
		.map(|(asset, price)| (asset, (price * BasisPoints(8500).to_fraction()).unwrap()))
		.collect();

	let election_for_chain_with_all_assets = |chain, status: Option<_>| {
		Check::<OraclePriceES>::election_for_chain_ongoing_with_asset_status((
			chain,
			status.map(|status| all::<ChainlinkAssetpair>().zip(repeat(status)).collect()),
		))
	};
	let current_prices_are = |prices: &BTreeMap<_, _>, staleness| {
		Check::<OraclePriceES>::electoral_price_api_returns(
			prices
				.clone()
				.into_iter()
				.map(|(asset, price)| (asset, (price, staleness)))
				.collect(),
		)
	};

	use ExternalPriceChain::*;
	use PriceStatus::*;
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
				election_for_chain_with_all_assets(Solana, Some(MaybeStale)),
				election_for_chain_with_all_assets(Ethereum, Some(MaybeStale)),
			],
		)
		//  - For solana: no consensus
		//  - For ethereum: consensus where all 20 voters vote for the exact same prices +
		//    timestamps
		.expect_consensus_multi(vec![
			// Data for the Solana election, no votes
			(no_votes(20), None),
			// Data for the Ethereum election, all 20 voters vote for the default price
			(
				generate_votes(
					(0..20).collect(),
					Default::default(),
					default_prices.clone().into(),
					Default::default(),
					START_TIME,
				),
				Some(generate_asset_response(START_TIME, &prices1)),
			),
		])
		// since we got prices for the secondary chain (Ethereum), the elections are:
		//  - Solana: MaybeStale (since we didn't get Solana prices yet)
		//  - Ethereum: UpToDate
		.test_on_finalize(
			&vec![()],
			|_| {},
			vec![
				election_for_chain_with_all_assets(Solana, Some(MaybeStale)),
				election_for_chain_with_all_assets(Ethereum, Some(UpToDate)),
				current_prices_are(&prices1, UpToDate),
			],
		)
		.mutate_unsynchronized_state(|state| {
			println!("stepping two timestep (30 seconds) forward");
			state.get_time.state.state.seconds += 2 * TIME_STEP.0;
		})
		// We get consensus:
		// - Solana: newest prices
		.expect_consensus_multi(vec![
			// Data for the Solana election, all 20 voters vote for the default price
			(
				generate_votes(
					(0..20).collect(),
					Default::default(),
					prices2.clone(),
					Default::default(),
					START_TIME + TIME_STEP + TIME_STEP,
				),
				Some(generate_asset_response(START_TIME + TIME_STEP * 2, &prices2)),
			),
			// Data for the Ethereum election, no votes
			(no_votes(20), None),
		])
		// since we now got prices for the primary chain (Solana):
		//  - there should be no ethereum election
		//  - the Solana election should have all price statuses be `UpToDate`
		//  - the stored prices should reflect the newer `prices2` values
		.test_on_finalize(
			&vec![()],
			|_| {},
			vec![
				election_for_chain_with_all_assets(Solana, Some(UpToDate)),
				election_for_chain_with_all_assets(Ethereum, None),
				current_prices_are(&prices2, UpToDate),
			],
		)
		.mutate_unsynchronized_state(|state| {
			println!("stepping 3 timesteps (45 seconds) forward");
			state.get_time.state.state.seconds += 3 * TIME_STEP.0;
		})
		// The prices on eth should now be MaybeStale, but since the ones on Solana are up to date,
		// we don't have Eth elections. Also the prices returned by the api are still up to date
		.test_on_finalize(
			&vec![()],
			|_| {},
			vec![
				election_for_chain_with_all_assets(Solana, Some(UpToDate)),
				election_for_chain_with_all_assets(Ethereum, None),
				current_prices_are(&prices2, UpToDate),
			],
		)
		.mutate_unsynchronized_state(|state| {
			println!("stepping 2 timesteps (30 seconds) forward");
			state.get_time.state.state.seconds += 2 * TIME_STEP.0;
		})
		// - the prices on solana are now maybe stale (sol elections going on)
		// - the prices on Eth are fully stale (eth elections going on)
		// - the prices returned by the API are still the same and maybe stale
		.test_on_finalize(
			&vec![()],
			|_| {},
			vec![
				election_for_chain_with_all_assets(Solana, Some(MaybeStale)),
				election_for_chain_with_all_assets(Ethereum, Some(Stale)),
				current_prices_are(&prices2, MaybeStale),
			],
		)
		.mutate_unsynchronized_state(|state| {
			println!("stepping 2 timesteps (30 seconds) forward");
			state.get_time.state.state.seconds += 2 * TIME_STEP.0;
		})
		// - the prices on both chains are now stale
		// - the prices returned by the API are stale
		.test_on_finalize(
			&vec![()],
			|_| {},
			vec![
				election_for_chain_with_all_assets(Solana, Some(Stale)),
				election_for_chain_with_all_assets(Ethereum, Some(Stale)),
				current_prices_are(&prices2, Stale),
			],
		)
		.mutate_unsynchronized_state(|state| {
			println!("setting time to 100 timesteps after start");
			state.get_time.state.state.seconds = START_TIME.seconds + 100 * TIME_STEP.0;
		})
		// We get consensus:
		// - Solana: newest prices
		.expect_consensus_multi(vec![
			// Data for the Ethereum election, no votes
			(no_votes(20), None),
			// Data for the Solana election, all 20 voters vote for price3
			(
				generate_votes(
					(0..20).collect(),
					Default::default(),
					prices3.clone(),
					Default::default(),
					START_TIME + TIME_STEP * 100,
				),
				Some(generate_asset_response(START_TIME + TIME_STEP * 100, &prices3)),
			),
		])
		// - the prices in the api should be updated to the newest value (prices3)
		// - there should be a Solana (UpToDate) election going on
		// - there should be no Ethereum election
		.test_on_finalize(
			&vec![()],
			|_| {},
			vec![
				election_for_chain_with_all_assets(Solana, Some(UpToDate)),
				election_for_chain_with_all_assets(Ethereum, None),
				current_prices_are(&prices3, UpToDate),
			],
		);
}

#[test]
fn election_lifecycles_handles_missing_assets_and_disparate_timestamps() {
	use ExternalPriceChain::*;
	use PriceStatus::*;

	let prices4assets = [
		(BtcUsd, ChainlinkPrice::integer(120_000)),
		(SolUsd, ChainlinkPrice::integer(170)),
		(UsdcUsd, ChainlinkPrice::integer(1)),
		(UsdtUsd, ChainlinkPrice::integer(1)),
	];

	let prices2assets =
		[(EthUsd, ChainlinkPrice::integer(3000)), (SolUsd, ChainlinkPrice::integer(160))];

	TestSetup::<OraclePriceES>::default()
		.with_unsynchronised_settings(SETTINGS)
		.build()
		.mutate_unsynchronized_state(|state| state.get_time.state.state = START_TIME)
		.test_on_finalize(&vec![()], |_| {}, vec![])
		// get consensus on 4 assets on Solana (Eth is missing)
		.expect_consensus_multi(vec![
			(
				generate_votes(
					(0..22).collect(),
					Default::default(),
					prices4assets.clone().into(),
					Default::default(),
					START_TIME,
				),
				Some(generate_asset_response(START_TIME, &prices4assets.clone().into())),
			),
			(no_votes(20), None),
		])
		// this means that we're still querying on both ethereum and solana
		.test_on_finalize(
			&vec![()],
			|_| {},
			vec![
				Check::<OraclePriceES>::election_for_chain_ongoing_with_asset_status((
					Solana,
					Some(
						all::<ChainlinkAssetpair>()
							.zip(repeat(UpToDate))
							.chain(iter::once((EthUsd, MaybeStale)))
							.collect(),
					),
				)),
				Check::<OraclePriceES>::election_for_chain_ongoing_with_asset_status((
					Ethereum,
					Some(all::<ChainlinkAssetpair>().zip(repeat(MaybeStale)).collect()),
				)),
				Check::<OraclePriceES>::electoral_price_api_returns(
					prices4assets
						.clone()
						.into_iter()
						.map(|(asset, price)| (asset, (price, UpToDate)))
						.collect(),
				),
			],
		)
		.mutate_unsynchronized_state(|state| {
			println!("stepping 2 timesteps (30 seconds) forward");
			state.get_time.state.state.seconds += TIME_STEP.0 * 2;
		})
		// get consensus on 2 assets on Solana (Eth & sol)
		.expect_consensus_multi(vec![
			(no_votes(20), None),
			(
				generate_votes(
					(0..20).collect(),
					Default::default(),
					prices2assets.clone().into(),
					Default::default(),
					START_TIME + TIME_STEP * 2,
				),
				Some(generate_asset_response(
					START_TIME + TIME_STEP * 2,
					&prices2assets.clone().into(),
				)),
			),
		])
		// this means that we're now querying only solana and all prices are up to date
		.test_on_finalize(
			&vec![()],
			|_| {},
			vec![
				Check::<OraclePriceES>::election_for_chain_ongoing_with_asset_status((
					Solana,
					Some(all::<ChainlinkAssetpair>().zip(repeat(UpToDate)).collect()),
				)),
				Check::<OraclePriceES>::election_for_chain_ongoing_with_asset_status((
					Ethereum, None,
				)),
				Check::<OraclePriceES>::electoral_price_api_returns(
					prices4assets
						.clone()
						.into_iter()
						.chain(prices2assets.clone()) // we override the SolUsd price with the new value & we now have an EthUsd
						// price
						.map(|(asset, price)| (asset, (price, UpToDate)))
						.collect(),
				),
			],
		)
		.mutate_unsynchronized_state(|state| {
			println!("stepping 3 timesteps (45 seconds) forward");
			state.get_time.state.state.seconds += TIME_STEP.0 * 3;
		})
		// since we're now in total 5 timesteps (1min 15s) after we got the "prices4assets",
		// they're all MaybeStale - except Sol and Eth that got updated more recently.
		// this also means we have an election ongoing for all assets for ethereum
		.test_on_finalize(
			&vec![()],
			|_| {},
			vec![
				Check::<OraclePriceES>::election_for_chain_ongoing_with_asset_status((
					Solana,
					Some(
						all::<ChainlinkAssetpair>()
							.zip(repeat(MaybeStale))
							.chain([(EthUsd, UpToDate), (SolUsd, UpToDate)])
							.collect(),
					),
				)),
				Check::<OraclePriceES>::election_for_chain_ongoing_with_asset_status((
					Ethereum,
					Some(all::<ChainlinkAssetpair>().zip(repeat(MaybeStale)).collect()),
				)),
				Check::<OraclePriceES>::electoral_price_api_returns(
					prices4assets
						.clone()
						.into_iter()
						.map(|(asset, price)| (asset, (price, MaybeStale)))
						.chain(
							prices2assets
								.clone()
								.into_iter()
								.map(|(asset, price)| (asset, (price, UpToDate))),
						) // we override the SolUsd & EthUsd price + status
						.collect(),
				),
			],
		);
}

#[test]
fn consensus_computes_correct_median_and_iqr() {
	// Note, this test is a bit brittle: the generation of a distribution of votes such that the
	// given iq_range is the consensus is sometimes off by one voter. These values were
	// experimentally chosen to make the computed and expected iq_range match up exactly.
	let prices = [
		(
			BtcUsd,
			AssetResponse {
				timestamp: Aggregated { median: START_TIME, iq_range: START_TIME..=START_TIME },
				price: Aggregated {
					median: Fraction::integer(120000),
					iq_range: Fraction::integer(110000)..=Fraction::integer(150000),
				},
			},
		),
		(
			EthUsd,
			AssetResponse {
				timestamp: Aggregated { median: START_TIME, iq_range: START_TIME..=START_TIME },
				price: Aggregated {
					median: Fraction::integer(3500),
					iq_range: Fraction::integer(2100)..=Fraction::integer(3800),
				},
			},
		),
	];

	TestSetup::<OraclePriceES>::default()
		.with_unsynchronised_settings(SETTINGS)
		.build()
		.mutate_unsynchronized_state(|state| state.get_time.state.state = START_TIME)
		.test_on_finalize(&vec![()], |_| {}, vec![])
		.expect_consensus_multi(vec![
			(
				generate_votes(
					(0..21).collect(),
					Default::default(),
					prices
						.iter()
						.map(
							|(asset, response): &(ChainlinkAssetpair, AssetResponse<MockTypes>)| {
								(*asset, response.price.median.clone())
							},
						)
						.collect(),
					prices
						.iter()
						.map(
							|(asset, response): &(ChainlinkAssetpair, AssetResponse<MockTypes>)| {
								(*asset, response.price.iq_range.clone())
							},
						)
						.collect(),
					START_TIME,
				),
				Some(prices.into()),
			),
			(no_votes(20), None),
		]);
}

//----------------------- utilities ------------------------

fn generate_asset_response(
	time: UnixTime,
	prices: &BTreeMap<ChainlinkAssetpair, ChainlinkPrice>,
) -> BTreeMap<ChainlinkAssetpair, AssetResponse<MockTypes>> {
	prices
		.clone()
		.into_iter()
		.map(|(asset, price)| {
			(
				asset,
				AssetResponse::<MockTypes> {
					timestamp: Aggregated::from_single_value(time),
					price: Aggregated::from_single_value(price),
				},
			)
		})
		.collect::<BTreeMap<_, _>>()
}

fn no_votes(authorities: u16) -> ConsensusVotes<OraclePriceES> {
	ConsensusVotes {
		votes: (0..authorities)
			.map(|id| ConsensusVote { vote: None, validator_id: id })
			.collect(),
	}
}

fn generate_votes(
	voters: BTreeSet<ValidatorId>,
	did_not_vote: BTreeSet<ValidatorId>,
	prices: BTreeMap<ChainlinkAssetpair, ChainlinkPrice>,
	price_ranges: BTreeMap<ChainlinkAssetpair, RangeInclusive<ChainlinkPrice>>,
	current_time: UnixTime,
) -> ConsensusVotes<OraclePriceES> {
	println!("Generate votes called");

	let half = max(1, voters.len() / 2);

	let lower_quartile = (half / 2) as u32;
	let upper_quartile = (voters.len() - half + 1) as u32 / 2;

	// we want to generate a distribution of prices such that we will get exactly the median price +
	// iqr that we input we distribute the votes linearly below and above the given price
	let votes: BTreeMap<ChainlinkAssetpair, BTreeMap<ValidatorId, ChainlinkPrice>> = prices
		.into_iter()
		.map(|(asset, price)| {
			let (step_below, step_above) = price_ranges
				.get(&asset)
				.map(|price_range| {
					(
						(&price - price_range.start().clone()) / lower_quartile,
						(price_range.end() - price.clone()) / upper_quartile,
					)
				})
				.unwrap_or_default();

			let votes = voters
				.iter()
				.enumerate()
				.map(|(index, voter)| {
					let vote = if index < half {
						&price - (&step_below * ChainlinkPrice::integer(half - index)).unwrap()
					} else {
						&price + (&step_above * ChainlinkPrice::integer(index - half)).unwrap()
					};
					(*voter, vote)
				})
				.collect();

			(asset, votes)
		})
		.collect();

	println!("generated votes: {votes:?}");

	let mut by_voter: BTreeMap<
		ValidatorId,
		BTreeMap<ChainlinkAssetpair, (UnixTime, ChainlinkPrice)>,
	> = Default::default();
	for (validator, (asset, data)) in votes.into_iter().flat_map(|(asset, prices)| {
		prices
			.into_iter()
			.map(move |(validator, price)| (validator, (asset, (current_time, price))))
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
