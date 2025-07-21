use core::ops::RangeInclusive;

use frame_system::pallet_prelude::BlockNumberFor;
use sol_prim::consts::const_address;
use sp_core::H160;
use sp_std::{vec, vec::Vec};

use pallet_cf_elections::{
	electoral_systems::{
		oracle_price::{
			chainlink::{ChainlinkAssetPair, ChainlinkPrice},
			price::PriceAsset,
			state_machine::OPTypes,
		},
		state_machine::core::*,
	},
	generic_tools::*,
};

use crate::{chainflip::elections::TypesFor, Runtime, Timestamp};
use cf_chains::sol::SolAddress;
use cf_primitives::Price;
use cf_traits::Chainflip;
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
			price::{price_with_unit_to_statechain_price, Fraction, PriceUnit},
			primitives::*,
			state_machine::*,
		},
		state_machine::{
			common_imports::*,
			core::{def_derive, Hook},
			state_machine_es::{StatemachineElectoralSystem, StatemachineElectoralSystemTypes},
		},
	},
	generic_tools::*,
	vote_storage, CorruptStorageError, ElectionIdentifierOf, InitialState, InitialStateOf,
	RunnerStorageAccess,
};

def_derive! {
	#[derive(TypeInfo)]
	pub struct ChainlinkOraclePriceSettings<Container: Functor = VectorContainer> {
		pub sol_oracle_program_id: SolAddress,
		pub sol_oracle_feeds: Container::Of<SolAddress>,
		pub sol_oracle_query_helper: SolAddress,
		pub eth_contract_address: H160,
		pub eth_oracle_feeds: Container::Of<H160>
	}
}

impl<F: Functor> ChainlinkOraclePriceSettings<F> {
	pub fn convert<G: Functor>(
		self,
		t: impl Transformation<F, G>,
	) -> ChainlinkOraclePriceSettings<G> {
		let ChainlinkOraclePriceSettings {
			sol_oracle_program_id,
			sol_oracle_feeds,
			sol_oracle_query_helper,
			eth_contract_address,
			eth_oracle_feeds,
		} = self;
		ChainlinkOraclePriceSettings {
			sol_oracle_program_id,
			sol_oracle_feeds: t.at(sol_oracle_feeds),
			sol_oracle_query_helper,
			eth_contract_address,
			eth_oracle_feeds: t.at(eth_oracle_feeds),
		}
	}
}

pub struct Chainlink;

impls! {
	for TypesFor<Chainlink>:

	OPTypes {
		type Price = ChainlinkPrice;
		type GetTime = Self;
		type AssetPair = ChainlinkAssetPair;
		type Aggregation = AggregatedF;

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

pub fn initial_state(
	chainlink_oracle_price_settings: ChainlinkOraclePriceSettings,
) -> InitialStateOf<Runtime, ()> {
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
		settings: (chainlink_oracle_price_settings,),
		shared_data_reference_lifetime: 8,
	}
}
