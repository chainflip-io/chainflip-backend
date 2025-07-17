use core::ops::RangeInclusive;

use cf_chains::sol::SolAddress;
use cf_primitives::Price;
use cf_traits::Chainflip;
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_elections::{
	electoral_system::ElectoralSystem,
	electoral_systems::{
		block_witnesser::state_machine::HookTypeFor,
		composite::{
			tuple_1_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
		oracle_price::{
			consensus::OraclePriceConsensus,
			price::{
				price_with_unit_to_statechain_price, ChainlinkAssetPair, FractionImpl, PriceAsset,
				PriceUnit,
			},
			primitives::*,
			state_machine::*,
		},
		state_machine::{
			common_imports::*,
			core::{def_derive, Hook},
			state_machine_es::{StatemachineElectoralSystem, StatemachineElectoralSystemTypes},
		},
	},
	vote_storage, CorruptStorageError, ElectionIdentifierOf, InitialState, InitialStateOf,
	RunnerStorageAccess,
};
use sol_prim::consts::const_address;
use sp_core::H160;

use crate::{chainflip::elections::TypesFor, Runtime, Timestamp};
use sp_std::{vec, vec::Vec};

pub fn get_current_chainlink_prices(
	state: &OraclePriceTracker<TypesFor<Chainlink>>,
) -> BTreeMap<PriceAsset, (Price, PriceStatus)> {
	//
	// WARNING: We are currently assuming that USD == USDC!
	//

	state
		.chain_states
		.get_latest_prices()
		.into_iter()
		.map(|(chainlink_assetpair, (price, status))| {
			let base_asset = chainlink_assetpair.base_asset();
			(
				base_asset,
				(
					price_with_unit_to_statechain_price(
						price,
						PriceUnit { base_asset, quote_asset: PriceAsset::Usdc },
					)
					.into(),
					status,
				),
			)
		})
		.collect()
}

def_derive! {
	#[derive(TypeInfo)]
	pub struct ChainlinkOraclePriceSettings {
		pub sol_oracle_program_id: SolAddress,
		pub sol_oracle_feeds: Vec<SolAddress>,
		pub sol_oracle_query_helper: SolAddress,
		pub eth_contract_address: H160,
		pub eth_oracle_feeds: Vec<H160>
	}
}

pub type ChainlinkPrice = FractionImpl<99_999_999>;

pub struct Chainlink;

impls! {
	for TypesFor<Chainlink>:

	OPTypes {
		type Price = ChainlinkPrice;
		type GetTime = Self;
		type Asset = ChainlinkAssetPair;
		type Aggregation = AggregatedF;

		fn price_range(price: &Self::Price, range: BasisPoints) -> RangeInclusive<Self::Price> {
			let delta = price * range.to_fraction();
			price + delta.clone() ..= price - delta
		}
	}

	Hook<HookTypeFor<Self, GetTimeHook>> {
		fn run(&mut self, _: ()) -> UnixTime {
			// in our configuration the timestamp pallet measures time in millis since the unix epoch
			UnixTime { seconds: Timestamp::get() / 1000 }
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

pub type OraclePriceES = StatemachineElectoralSystem<TypesFor<Chainlink>>;

pub struct GenericElectionHooks;

impl Hooks<OraclePriceES> for GenericElectionHooks {
	fn on_finalize(
		(oracle_price_election_identifiers,): (Vec<ElectionIdentifierOf<OraclePriceES>>,),
	) -> Result<(), CorruptStorageError> {
		OraclePriceES::on_finalize::<
			DerivedElectoralAccess<_, OraclePriceES, RunnerStorageAccess<Runtime, ()>>,
		>(oracle_price_election_identifiers, &Vec::from([()]))?;
		Ok(())
	}
}

impl pallet_cf_elections::GovernanceElectionHook for GenericElectionHooks {
	type Properties = ();

	fn start(_properties: Self::Properties) {}
}

pub type GenericElectoralSystemRunner = CompositeRunner<
	(OraclePriceES,),
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
	RunnerStorageAccess<Runtime, ()>,
	GenericElectionHooks,
>;

pub fn initial_state() -> InitialStateOf<Runtime, ()> {
	InitialState {
		unsynchronised_state: (OraclePriceTracker {
			chain_states: ExternalChainStates {
				solana: ExternalChainState { price: Default::default() },
				ethereum: ExternalChainState { price: Default::default() },
			},
			get_time: Default::default(),
		},),
		unsynchronised_settings: (OraclePriceSettings {
			solana: ExternalChainSettings {
				up_to_date_timeout: Seconds(10),
				maybe_stale_timeout: Seconds(10),
				minimal_price_deviation: BasisPoints(10),
			},
			ethereum: ExternalChainSettings {
				up_to_date_timeout: Seconds(10),
				maybe_stale_timeout: Seconds(10),
				minimal_price_deviation: BasisPoints(10),
			},
		},),
		// TODO change these to mainnet values, currently localnet
		settings: (ChainlinkOraclePriceSettings {
			sol_oracle_program_id: const_address("DfYdrym1zoNgc6aANieNqj9GotPj2Br88rPRLUmpre7X"),
			sol_oracle_feeds: vec![const_address("HDSV2wFxmsrmCwwY34QzaVkvmJpG7VF8S9fX2iThynjG")],
			sol_oracle_query_helper: const_address("GXn7uzbdNgozXuS8fEbqHER1eGpD9yho7FHTeuthWU8z"),
			eth_contract_address: H160(hex_literal::hex!(
				"e7f1725E7734CE288F8367e1Bb143E90bb3F0512"
			)),
			eth_oracle_feeds: vec![H160(hex_literal::hex!(
				"322813Fd9A801c5507c9de605d63CEA4f2CE6c44"
			))],
		},),
		shared_data_reference_lifetime: 8,
	}
}
