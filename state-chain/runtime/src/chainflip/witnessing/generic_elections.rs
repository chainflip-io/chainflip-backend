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

use cf_amm::math::Price;
use cf_primitives::chains::assets::any;
use cf_runtime_utilities::log_or_panic;
use cf_utilities::{impls, macros::*};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::{Get, RuntimeDebug, H160};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

use pallet_cf_elections::{
	electoral_system::ElectoralReadAccess,
	electoral_systems::{
		block_witnesser::primitives::SafeModeStatus,
		oracle_price::{
			chainlink::{
				chainlink_price_to_statechain_price, get_latest_price_with_statechain_encoding,
				ChainlinkAssetpair, ChainlinkPrice,
			},
			price::PriceAsset,
			primitives::{Seconds, UnixTime},
			state_machine::OPTypes,
		},
	},
	generic_tools::*,
};

use crate::{chainflip::witnessing::elections::TypesFor, Runtime, Timestamp};
use cf_traits::{impl_pallet_safe_mode, Chainflip, Hook, OraclePrice};
use pallet_cf_elections::{
	electoral_system::ElectoralSystem,
	electoral_systems::{
		block_witnesser::state_machine::HookTypeFor,
		composite::{
			tuple_1_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
		oracle_price::{consensus::OraclePriceConsensus, primitives::*, state_machine::*},
		state_machine::state_machine_es::{
			StatemachineElectoralSystem, StatemachineElectoralSystemTypes,
		},
	},
	vote_storage, CorruptStorageError, ElectionIdentifierOf, InitialState, InitialStateOf,
	RunnerStorageAccess,
};

//--------------- api provided to other pallets -------------

pub fn decode_and_get_latest_oracle_price<T: OPTypes>(asset: any::Asset) -> Option<OraclePrice> {
	use ChainlinkAssetpair::*;
	use PriceStatus::*;

	let state = DerivedElectoralAccess::<
			_,
			ChainlinkOraclePriceES,
			RunnerStorageAccess<Runtime, ()>,
		>::unsynchronised_state()
		.inspect_err(|_| log_or_panic!("Failed to get election state for the ChainlinkOraclePrice ES due to corrupted storage")).ok()?;

	let asset = match asset {
		any::Asset::Eth => Some(EthUsd),
		any::Asset::Flip => None,
		any::Asset::Usdc => Some(UsdcUsd),
		any::Asset::Usdt => Some(UsdtUsd),
		any::Asset::Wbtc => Some(BtcUsd),
		any::Asset::Dot => None,
		any::Asset::Btc => Some(BtcUsd),
		any::Asset::ArbEth => Some(EthUsd),
		any::Asset::ArbUsdc => Some(UsdcUsd),
		any::Asset::ArbUsdt => Some(UsdtUsd),
		any::Asset::Sol => Some(SolUsd),
		any::Asset::SolUsdc => Some(UsdcUsd),
		any::Asset::SolUsdt => Some(UsdtUsd),
		any::Asset::HubDot => None,
		any::Asset::HubUsdt => Some(UsdtUsd),
		any::Asset::HubUsdc => Some(UsdcUsd),
		any::Asset::Trx => None,
		any::Asset::TronUsdt => Some(UsdtUsd),
	}?;

	get_latest_price_with_statechain_encoding(&state, asset).map(|(price, staleness)| OraclePrice {
		price,
		stale: match staleness {
			UpToDate => false,
			MaybeStale => false,
			Stale => true,
		},
	})
}

//--------------- voter settings -------------

derive_common_traits! {
	#[derive(TypeInfo)]
	pub struct ChainlinkOraclePriceSettings<C: Container = VectorContainer> {
		pub arb_address_checker: H160,
		pub arb_oracle_feeds: C::Of<H160>,
		pub eth_address_checker: H160,
		pub eth_oracle_feeds: C::Of<H160>
	}
}

impl<F: Container> ChainlinkOraclePriceSettings<F> {
	pub fn convert<G: Container>(
		self,
		t: impl Transformation<F, G>,
	) -> ChainlinkOraclePriceSettings<G> {
		let ChainlinkOraclePriceSettings {
			arb_address_checker,
			arb_oracle_feeds,
			eth_address_checker,
			eth_oracle_feeds,
		} = self;
		ChainlinkOraclePriceSettings {
			arb_address_checker,
			arb_oracle_feeds: t.at(arb_oracle_feeds),
			eth_address_checker,
			eth_oracle_feeds: t.at(eth_oracle_feeds),
		}
	}
}

//--------------- instantiation of Chainlink ES -------------

pub struct Chainlink;

impls! {
	for TypesFor<Chainlink>:

	OPTypes {
		type StateChainBlockNumber = BlockNumberFor<Runtime>;
		type Price = ChainlinkPrice;
		type GetTime = Self;
		type GetStateChainBlockHeight = Self;
		type AssetPair = ChainlinkAssetpair;
		type SafeModeEnabledHook = Self;
		type EmitPricesUpdatedEventHook = Self;
	}

	Hook<HookTypeFor<Self, GetTimeHook>> {
		fn run(&mut self, _: ()) -> UnixTime {
			// in our configuration the timestamp pallet measures time in millis since the unix epoch
			UnixTime { seconds: Timestamp::get() / 1000 }
		}
	}

	Hook<HookTypeFor<Self, GetStateChainBlockHeight>> {
		fn run(&mut self, _: ()) -> BlockNumberFor<Runtime> {
			crate::System::block_number()
		}
	}

	Hook<HookTypeFor<Self, EmitPricesUpdatedEvent>> {
		fn run(&mut self, prices: Vec<(ChainlinkAssetpair, UnixTime, ChainlinkPrice)>) {
			pallet_cf_elections::Pallet::<Runtime>::deposit_event(
				pallet_cf_elections::Event::ElectoralEvent(GenericElectoralEvents::OraclePricesUpdated {
					prices: prices.into_iter()
						.filter_map(|(assetpair, timestamp, price)| {
							let price_unit = assetpair.to_price_unit();
							Some(OraclePriceUpdate {
								price: chainlink_price_to_statechain_price(&price, assetpair)?.into(),
								base_asset: price_unit.base_asset,
								quote_asset: price_unit.quote_asset,
								updated_at_oracle_timestamp: timestamp.seconds
							})
						}
						)
					.collect()
				})
			);
		}
	}

	Hook<HookTypeFor<Self, SafeModeEnabledHook>> {
		fn run(&mut self, _input: ()) -> SafeModeStatus {
			if <<Runtime as pallet_cf_elections::Config>::SafeMode as Get<GenericElectionsSafeMode>>::get()
			.oracle_price_elections
			{
				SafeModeStatus::Disabled
			} else {
				SafeModeStatus::Enabled
			}
		}
	}

	StatemachineElectoralSystemTypes {
		type ConsensusMechanism = OraclePriceConsensus<Self>;
		type OnFinalizeReturnItem = ();
		type StateChainBlockNumber = BlockNumberFor<Runtime>;
		type Statemachine = OraclePriceTracker<Self>;
		type ValidatorId = <Runtime as Chainflip>::ValidatorId;
		type VoteStorage = vote_storage::bitmap::Bitmap<ExternalChainStateVote<Self>>;
		type ElectoralSettings = ChainlinkOraclePriceSettings;
	}
}

pub type ChainlinkOraclePriceES = StatemachineElectoralSystem<TypesFor<Chainlink>>;

/// data for events
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct OraclePriceUpdate {
	/// internal price with 128 bits fractional part
	price: Price,

	/// base asset, here interpreted as "fine" asset
	base_asset: PriceAsset,

	/// quote asset, here interpreted as "fine" asset
	quote_asset: PriceAsset,

	/// seconds since Unix epoch
	updated_at_oracle_timestamp: u64,
}

//--------------- all generic ESs -------------

#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum GenericElectoralEvents {
	OraclePricesUpdated { prices: Vec<OraclePriceUpdate> },
}

impl_pallet_safe_mode! {
	GenericElectionsSafeMode;
	oracle_price_elections
}

pub struct GenericElectionHooks;

impl Hooks<ChainlinkOraclePriceES> for GenericElectionHooks {
	fn on_finalize(
		(oracle_price_election_identifiers,): (Vec<ElectionIdentifierOf<ChainlinkOraclePriceES>>,),
	) -> Result<(), CorruptStorageError> {
		ChainlinkOraclePriceES::on_finalize::<
			DerivedElectoralAccess<_, ChainlinkOraclePriceES, RunnerStorageAccess<Runtime, ()>>,
		>(oracle_price_election_identifiers, &Vec::from([()]))?;
		Ok(())
	}
}

impl pallet_cf_elections::ElectoralSystemConfiguration for GenericElectionHooks {
	type ElectoralEvents = GenericElectoralEvents;

	type SafeMode = GenericElectionsSafeMode;

	type Properties = ();

	fn start(_properties: Self::Properties) {}
}

pub type GenericElectoralSystemRunner = CompositeRunner<
	(ChainlinkOraclePriceES,),
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
	RunnerStorageAccess<Runtime, ()>,
	GenericElectionHooks,
>;

pub fn initial_state(
	chainlink_oracle_price_settings: ChainlinkOraclePriceSettings,
) -> InitialStateOf<Runtime, ()> {
	// The prices for usdc and usdt are considered up-to-date
	// if they have been updated at least once every 25 hours.
	let up_to_date_timeout_overrides: BTreeMap<_, _> = [
		(ChainlinkAssetpair::UsdcUsd, Seconds(60 * 60 * 25)),
		(ChainlinkAssetpair::UsdtUsd, Seconds(60 * 60 * 25)),
	]
	.into();

	// There is an additionaly 5 minute window during which we
	// ask the engines to submit any latest price information that
	// they have. Once this is over, the price is marked as stale.
	let maybe_stale_timeout_overrides: BTreeMap<_, _> = [
		(ChainlinkAssetpair::UsdcUsd, Seconds(60 * 5)),
		(ChainlinkAssetpair::UsdtUsd, Seconds(60 * 5)),
	]
	.into();

	InitialState {
		unsynchronised_state: (OraclePriceTracker {
			chain_states: ExternalChainStates {
				arbitrum: ExternalChainState { price: Default::default() },
				ethereum: ExternalChainState { price: Default::default() },
			},
			get_time: Default::default(),
			safe_mode_enabled: Default::default(),
			get_statechain_block_height: Default::default(),
			emit_oracle_price_event: Default::default(),
		},),
		unsynchronised_settings: (OraclePriceSettings {
			arbitrum: ExternalChainSettings {
				up_to_date_timeout: Seconds(60),
				maybe_stale_timeout: Seconds(30),
				minimal_price_deviation: BasisPoints(10),
				up_to_date_timeout_overrides: up_to_date_timeout_overrides.clone(),
				maybe_stale_timeout_overrides: maybe_stale_timeout_overrides.clone(),
			},
			ethereum: ExternalChainSettings {
				up_to_date_timeout: Seconds(60),
				maybe_stale_timeout: Seconds(30),
				minimal_price_deviation: BasisPoints(10),
				up_to_date_timeout_overrides: up_to_date_timeout_overrides.clone(),
				maybe_stale_timeout_overrides: maybe_stale_timeout_overrides.clone(),
			},
		},),
		settings: (chainlink_oracle_price_settings,),
		shared_data_reference_lifetime: 8,
	}
}

#[cfg(any(feature = "runtime-benchmarks", feature = "runtime-integration-tests"))]
pub fn set_price(asset: any::Asset, price: Price) {
	use pallet_cf_elections::electoral_systems::oracle_price::chainlink::statechain_price_to_chainlink_price;

	let asset_pair = match asset {
		any::Asset::Eth => ChainlinkAssetpair::EthUsd,
		any::Asset::Usdc => ChainlinkAssetpair::UsdcUsd,
		any::Asset::Usdt => ChainlinkAssetpair::UsdtUsd,
		any::Asset::Btc => ChainlinkAssetpair::BtcUsd,
		any::Asset::Sol => ChainlinkAssetpair::SolUsd,
		_ => panic!("Asset not supported for price oracle"),
	};

	let price = statechain_price_to_chainlink_price(&price.into(), asset_pair).unwrap();

	let mut state: OraclePriceTracker<TypesFor<Chainlink>> =
		pallet_cf_elections::ElectoralUnsynchronisedState::<Runtime>::get().unwrap().0;
	state.chain_states.arbitrum.price.insert(
		asset_pair,
		AssetState {
			price: Aggregated { median: price.clone(), iq_range: price.clone()..=price },
			price_status: PriceStatus::UpToDate,
			..Default::default()
		},
	);

	pallet_cf_elections::ElectoralUnsynchronisedState::<Runtime>::set(Some((state,)));
}
