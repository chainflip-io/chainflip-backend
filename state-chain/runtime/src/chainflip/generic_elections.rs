use cf_primitives::Asset;
use cf_traits::Chainflip;
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_elections::{
	electoral_system::ElectoralSystem,
	electoral_systems::{
		block_witnesser::state_machine::HookTypeFor,
		composite::{
			tags::A,
			tuple_1_impls::{DerivedElectoralAccess, Hooks},
			CompositeRunner,
		},
		oracle_price::{
			consensus::OraclePriceConsensus,
			primitives::{Aggregated, AggregatedF, Seconds, UnixTime},
			state_machine::{
				BasisPoints, ExternalChainBlockQueried, ExternalChainSettings, ExternalChainState,
				ExternalChainStateVote, ExternalChainStates, GetTimeHook, OPTypes,
				OraclePriceSettings, OraclePriceTracker,
			},
		},
		state_machine::{
			core::Hook,
			state_machine_es::{StatemachineElectoralSystem, StatemachineElectoralSystemTypes},
		},
	},
	vote_storage, CorruptStorageError, ElectionIdentifierOf, InitialState, InitialStateOf,
	RunnerStorageAccess,
};

use crate::{chainflip::elections::TypesFor, Runtime, Timestamp};
use sp_std::vec::Vec;

pub struct Chainlink;

impls! {
	for TypesFor<Chainlink>:

	OPTypes {
		type Price = Vec<u16>;
		type GetTime = Self;
		type Asset = Asset;
		type Aggregation = AggregatedF;

		fn all_assets() -> impl Iterator<Item = Self::Asset> {
			cf_chains::assets::any::Asset::all()
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

	fn start(properties: Self::Properties) {}
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
				solana: ExternalChainState {
					block: ExternalChainBlockQueried::Solana(0),
					timestamp: Aggregated {
						median: UnixTime { seconds: 0 },
						iq_range: UnixTime { seconds: 0 }..=UnixTime { seconds: 0 },
					},
					price: Default::default(),
				},
				ethereum: ExternalChainState {
					block: ExternalChainBlockQueried::Ethereum(0),
					timestamp: Aggregated {
						median: UnixTime { seconds: 0 },
						iq_range: UnixTime { seconds: 0 }..=UnixTime { seconds: 0 },
					},
					price: Default::default(),
				},
			},
			get_time: Default::default(),
			queries: Default::default(),
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
		settings: ((),),
		shared_data_reference_lifetime: 8,
	}
}
