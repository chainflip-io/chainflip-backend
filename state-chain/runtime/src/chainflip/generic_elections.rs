use cf_primitives::Price;
use cf_runtime_utilities::log_or_panic;
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::H160;
use sp_std::vec::Vec;

use pallet_cf_elections::{
	electoral_system::ElectoralReadAccess,
	electoral_systems::oracle_price::{
		chainlink::{
			get_latest_price_with_statechain_encoding, ChainlinkAssetpair, ChainlinkPrice,
		},
		state_machine::OPTypes,
	},
	generic_tools::*,
};

use crate::{chainflip::elections::TypesFor, Runtime, Timestamp};
use cf_chains::sol::SolAddress;
use cf_traits::Chainflip;
use pallet_cf_elections::{
	electoral_system::ElectoralSystem,
	electoral_systems::{
		block_witnesser::state_machine::HookTypeFor,
		composite::{
			tuple_1_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
		oracle_price::{consensus::OraclePriceConsensus, primitives::*, state_machine::*},
		state_machine::{
			core::{def_derive, Hook},
			state_machine_es::{StatemachineElectoralSystem, StatemachineElectoralSystemTypes},
		},
	},
	vote_storage, CorruptStorageError, ElectionIdentifierOf, InitialState, InitialStateOf,
	RunnerStorageAccess,
};

//--------------- api provided to other pallets -------------

pub struct OraclePrice {
	/// Statechain encoded price, fixed-point value with 128 bits for fractional part, ie.
	/// denominator is 2^128.
	pub price: Price,

	/// Whether the price is stale according to the oracle price ES settings.
	pub stale: bool,
}

pub fn decode_and_get_latest_oracle_price<T: OPTypes>(
	asset: ChainlinkAssetpair,
) -> Option<OraclePrice> {
	use PriceStatus::*;

	let state = DerivedElectoralAccess::<
			_,
			ChainlinkOraclePriceES,
			RunnerStorageAccess<Runtime, ()>,
		>::unsynchronised_state()
		.inspect_err(|_| log_or_panic!("Failed to get election state for the ChainlinkOraclePrice ES due to corrupted storage")).ok()?;

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

def_derive! {
	#[derive(TypeInfo)]
	pub struct ChainlinkOraclePriceSettings<C: Container = VectorContainer> {
		pub sol_oracle_program_id: SolAddress,
		pub sol_oracle_feeds: C::Of<SolAddress>,
		pub sol_oracle_query_helper: SolAddress,
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
			sol_oracle_program_id,
			sol_oracle_feeds,
			sol_oracle_query_helper,
			eth_address_checker,
			eth_oracle_feeds,
		} = self;
		ChainlinkOraclePriceSettings {
			sol_oracle_program_id,
			sol_oracle_feeds: t.at(sol_oracle_feeds),
			sol_oracle_query_helper,
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
		type Price = ChainlinkPrice;
		type GetTime = Self;
		type AssetPair = ChainlinkAssetpair;
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

pub type ChainlinkOraclePriceES = StatemachineElectoralSystem<TypesFor<Chainlink>>;

//--------------- all generic ESs -------------

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

impl pallet_cf_elections::GovernanceElectionHook for GenericElectionHooks {
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
