//! This pallet is intended to provide a highly flexible model on which to implement algorithms for
//! deciding external state such as deposits. We primarily need this as in Solana's case we cannot
//! rely on authorities seeing ingresses exactly the same way, and therefore we need a more
//! elaborate method to decide consensus rather than just checking if everyone voted for exactly the
//! same thing.
//!
//! The pallet's configuration is entirely done via the `ElectoralSystem` trait, which an
//! implementation of must be provided as part of the pallet's substrate "Config". Implementations
//! of this trait must provide a set of callbacks the pallet will call (via the trait).
//!
//! The pallet is based around the idea of "elections". At any point in time, the pallet is
//! "running" a possibly empty set of elections which each authority can provide up to one vote for.
//! If authorities vote repeatedly their previous vote is overwritten.
//!
//! While there was other parts of the `ElectoralSystem` trait the most important parts is the
//! `Vote` associated type, the `check_consensus` function, and the `on_finalize` function
//! - `Vote` allows the `ElectoralSystem` to set what information is contained in a vote.
//! - `check_consensus` allows the `ElectoralSystem` to specify how to decide if an election has
//!   consensus, based on the current set of votes.
//! - `on_finalize`, which is called each block during the pallet's our `on_finalize` hook, allows
//!   the `ElectoralSystem` to create and delete elections, and to check their consensus and than
//!   optionally take action based on that consensus or lack there of.
//!
//! --------------------------------------Vote Storage--------------------------------------
//!
//! While it would be nice and simple to store authority votes in a StorageDoubleMap from
//! validator_id and election_id to vote, this is very costly in terms of storage. Consider an
//! authority set of 150 authorities and a single election in which everyone has voted, just storing
//! the keys of that map for that election would consume more than 4KB. To avoid this problem where
//! possible the pallet provides a method to configure the Vote storage method via the `VoteStorage`
//! trait.
//!
//! Also having 150 authorities needing to exchange 150 vote extrinsics for every election is
//! potentially expensive, so this pallet provides the ability for validators to both batch multiple
//! votes into a single extrinsic, but also to vote using only the hash of the vote information
//! (`PartialVote`), as long as alteast one other validator does provide the matching full vote
//! data. Note the `PartialVote` is not restricted to only being the hash of the full vote.
//!
//! This diagram shows how an authority's vote is formulated, and split up so it can be stored:
//! ```text
//!     ┌─────────────────────────────────────────────────────────────────┐
//!     │   Key:                                                          │
//!     │                                                                 │
//!     │   A ──(N)───► B : A contains/is constructed from <N> B values   │
//!     │                                                                 │
//!     │   A ─ (N)─ ─► B : A references <N> B values by hash             │
//!     │                                                                 │
//!     └─────────────────────────────────────────────────────────────────┘
//!
//!      ┌──────────────────────────────────────────────────────┐
//!      │ The formats in which an authority may provide a vote.│
//!      └─┬──────────────────┬─────────────────────────────────┘
//!        │┼────────────────┼│
//!        ││                ││
//!        ││   Vote─────────┼┼─────────────────────────────┐
//!        ││   │            ││                             │
//!        ││  (1)           ││                             │
//!        ││   │            ││                             │
//!        ││   ▼            ││                             │
//!        ││   PartialVote  ││                             │
//!        ││   │            ││                             │
//!        │┼───┼────────────┼│                             │
//!        └────┼─────────────┘                             │
//!             │                                           │
//!             ▼                                           │
//!             Components──────────────┐                   │
//!             │                       │                   │
//!             │                       │                   │
//!          (0 or 1)                (0 or 1)              (0+)
//!             │                       │                   │
//!             │                       │                   │
//!         ┌───┼───────────────────────┼───────────────────┼─────────────────────────┐
//!         │┼──┼───────────────────────┼───────────────────┼────────────────────────┼│
//!         ││  │                       │                   │                        ││
//!         ││  ▼                       ▼                   ▼                        ││
//!         ││  IndividualComponent     BitmapComponent     SharedData               ││
//!         ││  │                       │                   ▲   ▲                    ││
//!         ││                                                                       ││
//!         ││  │                       └ ─ ─ ─ ─(0+) ─ ─ ─ ┘   │                    ││
//!         ││                                                                       ││
//!         ││  └ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─(0+) ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘                    ││
//!         ││                                                                       ││
//!         │┼───────────────────────────────────────────────────────────────────────┼│
//!         └───────────┼────────────────────────────┼────────────────────────────────┘
//!                     │How the pallet stores votes.│
//!                     └────────────────────────────┘
//! ```
//! - "SharedData" is shared between authority votes, so if 150 different validator votes when
//!   "split up" contain the same SharedData, only one copy of that SharedData will be stored. A
//!   vote when split up, may be constructed from any number of SharedData values, including zero.
//! - "IndividualComponent" is stored in a map from election id and validator id to
//!   IndividualCompoment. This will consume a lot of storage as described above. A vote when split
//!   up, can only contain upto a single "IndividualComponent" (This is enforced by the interfaces,
//!   and cannot be "messed up" by bad VoteStorage or ElectoralSystem impls).
//! - "BitmapComponent" is stored once similiar to SharedData, but with a bitmap to indicate which
//!   authorities votes used that "BitmapCompoment" value. A vote when split up, can only contain
//!   upto a single "BitmapCompoment" (This is enforced by the interfaces, and cannot be "messed up"
//!   by bad VoteStorage or ElectoralSystem impls).
//!
//! Note that all these types "Vote", "PartialVote", "SharedData", "IndividualComponent", and
//! "BitmapComponent" are set via the VoteStorage trait, and how an "AuthorityVote" is split up into
//! or reconstructed from the others is also configured via that trait.

#![feature(try_find)]
#![cfg_attr(test, feature(closure_track_caller))]
#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

pub mod electoral_system;
pub mod electoral_system_runner;
pub mod electoral_systems;
mod mock;
mod tests;
pub mod vote_storage;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;
pub use weights::WeightInfo;

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

pub use pallet::*;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(5);

pub use pallet::UniqueMonotonicIdentifier;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[cfg(feature = "runtime-benchmarks")]
	use cf_chains::benchmarking_value::BenchmarkValue;
	use cf_primitives::{AuthorityCount, EpochIndex};
	use cf_traits::{AccountRoleRegistry, Chainflip, EpochInfo};

	use crate::electoral_system::ConsensusStatus;
	pub use access_impls::RunnerStorageAccess;

	use crate::electoral_system_runner::RunnerStorageAccessTrait;
	use bitmap_components::ElectionBitmapComponents;
	pub use electoral_system::{
		AuthorityVoteOf, ConsensusVote, ConsensusVotes, ElectionIdentifierOf, ElectoralSystemTypes,
		IndividualComponentOf, PartialVoteOf, VoteOf, VotePropertiesOf, VoteStorageOf,
	};
	pub use electoral_system_runner::ElectoralSystemRunner;

	use frame_support::{
		sp_runtime::traits::BlockNumberProvider, storage::bounded_btree_map::BoundedBTreeMap,
		Deserialize, Serialize, StorageDoubleMap as _,
	};
	use itertools::Itertools;
	use sp_std::{
		collections::{btree_map::BTreeMap, btree_set::BTreeSet},
		vec::Vec,
	};
	use vote_storage::{AuthorityVote, VoteComponents, VoteStorage};

	pub const MAXIMUM_VOTES_PER_EXTRINSIC: u32 = 16;
	const BLOCKS_BETWEEN_CLEANUP: u64 = 128;

	#[derive(Clone, Debug, Encode, Decode, TypeInfo)]
	pub struct AuthorityElectionData<Settings, Properties, AuthorityVote> {
		pub settings: Settings,
		pub properties: Properties,
		pub is_vote_desired: bool,
		/// Important we use `AuthorityVote` as when validator wants to delete invalid/possibly bad
		/// votes, it needs to delete `PartialVote`s as other validators may submit `SharedData`
		/// later that would cause those `PartialVote`s to be reconstructed to full `Vote`s.
		pub option_existing_vote: Option<AuthorityVote>,
	}

	#[derive(Clone, Debug, Encode, Decode, TypeInfo)]
	pub struct ElectoralData<ElectionIdentifier, Settings, Properties, AuthorityVote, BlockNumber> {
		pub current_elections: BTreeMap<
			ElectionIdentifier,
			AuthorityElectionData<Settings, Properties, AuthorityVote>,
		>,
		pub unprovided_shared_data_hashes: BTreeMap<SharedDataHash, ReferenceDetails<BlockNumber>>,
		pub contributing: bool,
		pub authority_count: u32,
	}

	/// This is the information exposed via RPC to the engine each block so it can decide how and
	/// when to vote.
	#[allow(type_alias_bounds)]
	pub type ElectoralDataFor<T: Config<I>, I: 'static> = ElectoralData<
		ElectionIdentifierOf<T::ElectoralSystemRunner>,
		<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralSettings,
		<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionProperties,
		AuthorityVoteOf<T::ElectoralSystemRunner>,
		BlockNumberFor<T>,
	>;

	/// A unique identifier for an election.
	#[derive(
		PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Encode, Decode, TypeInfo, Default,
	)]
	pub struct UniqueMonotonicIdentifier(u64);

	impl UniqueMonotonicIdentifier {
		#[cfg(test)]
		pub(crate) fn next_identifier(&self) -> Option<Self> {
			self.0.checked_add(1).map(Self)
		}

		#[cfg(any(feature = "runtime-benchmarks", test))]
		pub const fn from_u64(value: u64) -> Self {
			Self(value)
		}
	}

	/// A unique identifier for an election with extra details used by the ElectoralSystem
	/// implementation. These extra details are currently used in composite electoral systems to
	/// identify which type of election an identifier refers to, without having to read additional
	/// storage.
	#[derive(
		PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug, Encode, Decode, TypeInfo, Default,
	)]
	pub struct ElectionIdentifier<Extra>(UniqueMonotonicIdentifier, Extra);
	impl<Extra> ElectionIdentifier<Extra> {
		pub(crate) fn new(unique_monotonic: UniqueMonotonicIdentifier, extra: Extra) -> Self {
			Self(unique_monotonic, extra)
		}

		pub fn with_extra<OtherExtra>(
			&self,
			other_extra: OtherExtra,
		) -> ElectionIdentifier<OtherExtra> {
			ElectionIdentifier::new(*self.unique_monotonic(), other_extra)
		}

		pub fn unique_monotonic(&self) -> &UniqueMonotonicIdentifier {
			&self.0
		}

		pub fn extra(&self) -> &Extra {
			&self.1
		}
	}

	/// The hash of the original `SharedData` information, used retrieve the original information.
	#[derive(
		Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode, TypeInfo,
	)]
	pub struct SharedDataHash(sp_core::H256);
	impl SharedDataHash {
		pub fn of<Vote: frame_support::Hashable>(vote: &Vote) -> Self {
			Self(vote.blake2_256().into())
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl BenchmarkValue for SharedDataHash {
		fn benchmark_value() -> Self {
			Self(Default::default())
		}
	}

	// Private mod forces creation of error via the new function
	mod corrupt_storage_error {
		/// This error is used indicate that the pallet's Storage is corrupt. If it is returned by
		/// an ElectoralSystem then the whole pallet will stop all actions, and output an error
		/// Event every block. This error should not be handled or interpreted, and instead should
		/// always be passed out. Note there are a small number of cases we do handle these errors,
		/// specifically in Solana's chain/fee tracking trait impls as those traits do not allow
		/// errors to be returned, this is ok, but should be avoided in future.
		#[derive(Debug, PartialEq, Eq)]
		pub struct CorruptStorageError {}
		impl CorruptStorageError {
			/// We use this function to create this error type (and make the struct impossible to
			/// create without it) so it is easier to find all locations we create the error, and so
			/// every location will log.
			#[track_caller]
			#[allow(clippy::new_without_default)]
			pub fn new() -> Self {
				log::error!(
					"Election pallet CorruptStorageError at '{}'.",
					core::panic::Location::caller()
				);
				Self {}
			}
		}
	}
	pub use corrupt_storage_error::CorruptStorageError;

	#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo)]
	pub enum ElectionPalletStatus {
		Paused { detected_corrupt_storage: bool },
		Running,
	}

	#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo, Serialize, Deserialize)]
	pub struct InitialState<
		ElectoralUnsynchronisedState,
		ElectoralUnsynchronisedSettings,
		ElectoralSettings,
	> {
		pub unsynchronised_state: ElectoralUnsynchronisedState,
		pub unsynchronised_settings: ElectoralUnsynchronisedSettings,
		pub settings: ElectoralSettings,
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl<A: BenchmarkValue, B: BenchmarkValue, C: BenchmarkValue> BenchmarkValue
		for InitialState<A, B, C>
	{
		fn benchmark_value() -> Self {
			InitialState::<A, B, C> {
				unsynchronised_state: A::benchmark_value(),
				unsynchronised_settings: B::benchmark_value(),
				settings: C::benchmark_value(),
			}
		}
	}

	#[allow(type_alias_bounds)]
	pub type InitialStateOf<T: Config<I>, I: 'static> = InitialState<
		<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
		<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralSettings,
	>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		#[allow(clippy::type_complexity)]
		pub option_initial_state: Option<InitialStateOf<T, I>>,
	}

	impl<T: Config<I>, I: 'static> Default for GenesisConfig<T, I> {
		fn default() -> Self {
			Self { option_initial_state: Default::default() }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			if let Some(initial_state) = self.option_initial_state.clone() {
				Pallet::<T, I>::internally_initialize(initial_state)
					.expect("Pallet could not be already initialized at genesis.");
			}
		}
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type ElectoralSystemRunner: ElectoralSystemRunner<
			ValidatorId = <Self as Chainflip>::ValidatorId,
		>;

		/// The weights for the pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// Corrupted storage was detected, and so all elections and voting has been paused.
		CorruptStorage,
		/// A request was made, but the pallet hasn't been initialized.
		Uninitialized,
		/// All vote data was cleared.
		AllVotesCleared,
		/// Not all vote data was cleared. *You should continue clearing votes until you receive
		/// the AllVotesCleared event*.
		AllVotesNotCleared,
		/// Received vote for an unknown election
		UnknownElection(ElectionIdentifierOf<T::ElectoralSystemRunner>),
	}

	#[derive(CloneNoBound, PartialEqNoBound, EqNoBound)]
	#[pallet::error]
	pub enum Error<T, I = ()> {
		Uninitialized,
		AlreadyInitialized,
		UnknownElection,
		Unauthorised,
		Paused,
		NotPaused,
		UnreferencedSharedData,
		CorruptStorage,
		VotesNotCleared,
		NotContributing,
		NoVotesSpecified,
	}

	// ---------------------------------------------------------------------------------------- //

	/// Stores the number of blocks after a piece of shared data is first referenced without being
	/// "provided" before expiring. Expiring will cause all votes that include references to be
	/// invalidated. This should be set as low as possible, I'd suggest using 8 blocks, which
	/// equates to 48 seconds.
	#[pallet::storage]
	pub(crate) type SharedDataReferenceLifetime<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// Stores the number of references to a shared vote. We also store the block number at which
	/// the first reference to a given SharedDataHash was added. If the associated SharedData has
	/// not been added, as this block number becomes older the probability a validator will submit
	/// the associated SharedData increases. After a number of blocks without the SharedData being
	/// added the reference will be removed which will invalidate any votes that reference it,
	/// forcing validators who referenced it to revote.
	#[pallet::storage]
	pub type SharedDataReferenceCount<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Identity,
		SharedDataHash,
		Twox64Concat,
		UniqueMonotonicIdentifier,
		ReferenceDetails<BlockNumberFor<T>>,
		OptionQuery,
	>;

	#[derive(PartialEq, Eq, Clone, Debug, Encode, Decode, TypeInfo, Default)]
	pub struct ReferenceDetails<BlockNumber> {
		pub count: u32,
		/// The block at which the first reference was introduced.
		pub created: BlockNumber,
		/// The block at which this reference will become invalid. This will be `self.created +
		/// SharedDataReferenceLifetime::get()`.
		pub expires: BlockNumber,
	}

	/// Stores the *shared* parts of validator votes. Any duplicates will only be stored once,
	/// thereby decreasing the storage costs of validator votes as generally most validator's votes
	/// will be duplicates. A validator can choose to only provide the hashes of these pieces of
	/// data instead of the full data, any validator who has the associated data will randomly
	/// choose to submit it, where the probability increases over time.
	#[pallet::storage]
	pub type SharedData<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Identity,
		SharedDataHash,
		<<T::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as VoteStorage>::SharedData,
		OptionQuery,
	>;

	/// A mapping from election id and validator id to shared vote hash that uses bitmaps to
	/// decrease space requirements assuming most validators submit the same hashes.
	#[pallet::storage]
	pub type BitmapComponents<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		UniqueMonotonicIdentifier,
		ElectionBitmapComponents<T, I>,
		OptionQuery,
	>;

	/// A mapping from election id and validator id to individual vote components.
	#[pallet::storage]
	pub type IndividualComponents<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Twox64Concat,
		UniqueMonotonicIdentifier,
		Identity,
		T::ValidatorId,
		(
			VotePropertiesOf<T::ElectoralSystemRunner>,
			IndividualComponentOf<T::ElectoralSystemRunner>,
		),
		OptionQuery,
	>;

	// TODO: rename this storage item to be specific to umi.
	// election identifier is used elsewhere to mean umi + extra.
	/// Stores the next valid election identifier.
	#[pallet::storage]
	pub(crate) type NextElectionIdentifier<T: Config<I>, I: 'static = ()> =
		StorageValue<_, UniqueMonotonicIdentifier, ValueQuery>;

	/// Stores governance-controlled settings regarding the electoral system. These settings can be
	/// changed by governance at anytime.
	#[pallet::storage]
	pub type ElectoralUnsynchronisedSettings<T: Config<I>, I: 'static = ()> = StorageValue<
		_,
		<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
		OptionQuery,
	>;

	/// Stores persistent state the electoral system needs.
	#[pallet::storage]
	pub type ElectoralUnsynchronisedState<T: Config<I>, I: 'static = ()> = StorageValue<
		_,
		<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
		OptionQuery,
	>;

	/// Stores persistent state the electoral system needs.
	#[pallet::storage]
	pub type ElectoralUnsynchronisedStateMap<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapKey,
		<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapValue,
		OptionQuery,
	>;

	/// Stores governance-controlled settings regarding the elections. These settings can be changed
	/// at anytime, but that change will only affect newly created elections.
	#[pallet::storage]
	pub type ElectoralSettings<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		UniqueMonotonicIdentifier,
		<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralSettings,
		OptionQuery,
	>;

	/// Stores the properties of each election. These settings are fixed and are set on creation of
	/// the election by the electoral system.
	#[pallet::storage]
	pub type ElectionProperties<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		ElectionIdentifierOf<T::ElectoralSystemRunner>,
		<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionProperties,
		OptionQuery,
	>;

	/// Stores mutable per-election state that the electoral system needs.
	#[pallet::storage]
	pub type ElectionState<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		UniqueMonotonicIdentifier,
		<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionState,
		OptionQuery,
	>;

	/// Stores the most recent consensus, i.e. the most recent result of
	/// `ElectoralSystemRunner::check_consensus` that returned `Some(...)`, and whether it is
	/// `current` / has not been `lost` since.
	#[pallet::storage]
	pub type ElectionConsensusHistory<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		UniqueMonotonicIdentifier,
		ConsensusHistory<<T::ElectoralSystemRunner as ElectoralSystemTypes>::Consensus>,
		OptionQuery,
	>;

	#[derive(PartialEq, Eq, Clone, Debug, Encode, Decode, TypeInfo, Default)]
	pub struct ConsensusHistory<T> {
		/// The most recent consensus the election had.
		pub most_recent: T,
		/// Indicates if consensus was lost after the `most_recent` consensus was gained. I.e. that
		/// we currently do not have consensus.
		///
		/// Note that `lost_since` is only based on when `check_consensus` is called, and so it is
		/// possible consensus was "lost" and regained, but as `check_consensus` was not called
		/// while the consensus was "lost", this member could still be `false`.
		pub lost_since: bool,
	}

	/// Stores the elections whose consensus doesn't need to be rechecked, and the epoch when they
	/// were last checked.
	#[pallet::storage]
	pub type ElectionConsensusHistoryUpToDate<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, UniqueMonotonicIdentifier, EpochIndex, OptionQuery>;

	/// Stores the set of authorities whose votes can contribute to consensus. Whether an authority
	/// is included is controlled solely by them. This serves as a method for validators to quickly
	/// remove all their votes from consensus, without having to know which votes should be removed
	/// and without deleting votes that are still valid. This storage item is not consistent with
	/// the current authority set, and so it may include authorities that are not in the current
	/// authority set or exclude authorities that are in the current authority set.
	#[pallet::storage]
	pub(crate) type ContributingAuthorities<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Identity, T::ValidatorId, (), OptionQuery>;

	/// Stores the status of the ElectoralSystem, i.e. if it is initialized, paused, or running. If
	/// this is None, the pallet is considered uninitialized.
	#[pallet::storage]
	pub type Status<T: Config<I>, I: 'static = ()> =
		StorageValue<_, ElectionPalletStatus, OptionQuery>;

	// ---------------------------------------------------------------------------------------- //

	pub mod access_impls {
		use electoral_system_runner::RunnerStorageAccessTrait;

		use super::*;

		// Provides access to the storage layer in a controlled, and consistent way.
		pub struct RunnerStorageAccess<T: Config<I>, I: 'static> {
			_phantom: core::marker::PhantomData<(T, I)>,
		}

		impl<T: Config<I>, I: 'static> RunnerStorageAccessTrait for RunnerStorageAccess<T, I> {
			type ElectoralSystemRunner = T::ElectoralSystemRunner;

			fn new_election(
				extra: <T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra,
				properties: <T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionProperties,
				state: <T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionState,
			) -> Result<ElectionIdentifierOf<Self::ElectoralSystemRunner>, CorruptStorageError> {
				let unique_monotonic_identifier = NextElectionIdentifier::<T, I>::get();
				let election_identifier = ElectionIdentifier(unique_monotonic_identifier, extra);
				NextElectionIdentifier::<T, I>::set(UniqueMonotonicIdentifier(
					unique_monotonic_identifier
						.0
						.checked_add(1)
						.ok_or_else(CorruptStorageError::new)?,
				));
				ElectionProperties::<T, I>::insert(election_identifier, properties);
				ElectionState::<T, I>::insert(unique_monotonic_identifier, state);
				Ok(election_identifier)
			}

			fn electoral_settings_for_election(
				unique_monotonic_identifier: UniqueMonotonicIdentifier,
			) -> Result<
				<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralSettings,
				CorruptStorageError,
			> {
				let mut settings_boundaries =
					ElectoralSettings::<T, I>::iter_keys().collect::<Vec<_>>();
				settings_boundaries.sort();
				let settings_boundary = settings_boundaries
					.iter()
					.rev()
					.find(|settings_boundary| **settings_boundary <= unique_monotonic_identifier)
					.ok_or_else(CorruptStorageError::new)?;
				ElectoralSettings::<T, I>::get(settings_boundary)
					.ok_or_else(CorruptStorageError::new)
			}
			fn election_properties(
				election_identifier: ElectionIdentifierOf<T::ElectoralSystemRunner>,
			) -> Result<
				<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionProperties,
				CorruptStorageError,
			> {
				ElectionProperties::<T, I>::get(election_identifier)
					.ok_or_else(CorruptStorageError::new)
			}
			fn election_state(
				unique_monotonic_identifier: UniqueMonotonicIdentifier,
			) -> Result<
				<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionState,
				CorruptStorageError,
			> {
				ElectionState::<T, I>::get(unique_monotonic_identifier)
					.ok_or_else(CorruptStorageError::new)
			}

			fn set_election_state(
				unique_monotonic_identifier: UniqueMonotonicIdentifier,
				state: <T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionState,
			) -> Result<(), CorruptStorageError> {
				if Self::election_state(unique_monotonic_identifier)? != state {
					ElectionState::<T, I>::insert(unique_monotonic_identifier, state);
					ElectionConsensusHistoryUpToDate::<T, I>::remove(unique_monotonic_identifier);
				}

				Ok(())
			}
			fn clear_election_votes(unique_monotonic_identifier: UniqueMonotonicIdentifier) {
				ElectionBitmapComponents::<T, I>::clear(unique_monotonic_identifier);
				for (_, (_, individual_component)) in
					IndividualComponents::<T, I>::drain_prefix(unique_monotonic_identifier)
				{
					<<T::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as
				VoteStorage>::visit_shared_data_references_in_individual_component(&
				individual_component, |shared_data_hash| { 		Pallet::<T,
				I>::remove_shared_data_reference(shared_data_hash, unique_monotonic_identifier);
					});
				}
				ElectionConsensusHistoryUpToDate::<T, I>::remove(unique_monotonic_identifier);
			}
			fn delete_election(
				composite_election_identifier: ElectionIdentifierOf<Self::ElectoralSystemRunner>,
			) {
				let unique_monotonic_identifier = composite_election_identifier.unique_monotonic();
				Self::clear_election_votes(*unique_monotonic_identifier);
				ElectionProperties::<T, I>::remove(composite_election_identifier);
				ElectionState::<T, I>::remove(unique_monotonic_identifier);
				ElectionConsensusHistory::<T, I>::remove(unique_monotonic_identifier);
			}

			fn refresh_election(
				election_identifier: ElectionIdentifierOf<Self::ElectoralSystemRunner>,
				new_extra: <T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra,
				properties: <T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectionProperties,
			) -> Result<(), CorruptStorageError> {
				if new_extra <= *election_identifier.extra() {
					Err(CorruptStorageError::new())
				} else {
					ElectionProperties::<T, I>::remove(election_identifier);
					let new_election_identifier =
						ElectionIdentifierOf::<Self::ElectoralSystemRunner>::new(
							*election_identifier.unique_monotonic(),
							new_extra,
						);
					ElectionProperties::<T, I>::insert(new_election_identifier, properties);
					Ok(())
				}
			}

			fn check_election_consensus(
				election_identifier: ElectionIdentifierOf<Self::ElectoralSystemRunner>,
			) -> Result<
				ConsensusStatus<<T::ElectoralSystemRunner as ElectoralSystemTypes>::Consensus>,
				CorruptStorageError,
			> {
				let epoch_index = T::EpochInfo::epoch_index();
				let unique_monotonic_identifier = election_identifier.unique_monotonic();
				let option_consensus_history =
					ElectionConsensusHistory::<T, I>::get(unique_monotonic_identifier);
				Ok(
					if ElectionConsensusHistoryUpToDate::<T, I>::get(unique_monotonic_identifier) ==
						Some(epoch_index)
					{
						match option_consensus_history {
							Some(ConsensusHistory { most_recent, lost_since }) if !lost_since =>
								ConsensusStatus::Unchanged { current: most_recent },
							_ => ConsensusStatus::None,
						}
					} else {
						let current_authorities = T::EpochInfo::current_authorities();
						let current_authorities_count: AuthorityCount = current_authorities
							.len()
							.try_into()
							.map_err(|_| CorruptStorageError::new())?;

						let bitmap_components = ElectionBitmapComponents::<T, I>::with(
							epoch_index,
							*unique_monotonic_identifier,
							|election_bitmap_components| {
								election_bitmap_components.get_all(&current_authorities)
							},
						)?;
						let mut individual_components =
							IndividualComponents::<T, I>::iter_prefix(unique_monotonic_identifier)
								.collect::<BTreeMap<_, _>>();

						let votes = current_authorities
							.into_iter()
							.map(|validator_id| {
								(
									VoteComponents {
										bitmap_component: bitmap_components
											.get(&validator_id)
											.cloned(),
										individual_component: individual_components
											.remove(&validator_id),
									},
									validator_id,
								)
							})
							.map(|(vote_components, validator_id)| {
								if ContributingAuthorities::<T, I>::contains_key(&validator_id) {
									match <<T::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as
								VoteStorage>::components_into_authority_vote(vote_components, |shared_data_hash|
							{
								 	// We don't bother to check if the reference has expired, as if we have the
									// data we may as well use it, even if it was provided after the shared data
									// reference expired (But before the reference was cleaned up `on_finalize`).
									Ok(SharedData::<T, I>::get(shared_data_hash))
								}) {
									// Only a full vote can count towards consensus.
									Ok(Some((properties, AuthorityVote::Vote(vote)))) => Ok(Some((properties, vote))),
									Ok(Some((_properties, AuthorityVote::PartialVote(_)))) => Ok(None),
									Ok(None) => Ok(None),
									Err(e) => Err(e),
								}
								} else {
									Ok(None)
								}
								.map(|props_and_vote| ConsensusVote {
									vote: props_and_vote,
									validator_id,
								})
							})
							.collect::<Result<Vec<_>, _>>()?;

						debug_assert!(votes.len() == current_authorities_count as usize);

						// Remove individual components from non-authorities
						for (validator_id, (_, individual_component)) in individual_components {
							<<T::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as
				VoteStorage>::visit_shared_data_references_in_individual_component(
								&individual_component,
								|shared_data_hash| {
									Pallet::<T, I>::remove_shared_data_reference(shared_data_hash,
				*unique_monotonic_identifier); 				},
							);
							IndividualComponents::<T, I>::remove(
								unique_monotonic_identifier,
								&validator_id,
							);
						}

						let option_new_consensus =
							<T::ElectoralSystemRunner as ElectoralSystemRunner>::check_consensus(
								election_identifier,
								option_consensus_history.as_ref().and_then(|consensus_history| {
									if consensus_history.lost_since {
										None
									} else {
										Some(&consensus_history.most_recent)
									}
								}),
								ConsensusVotes { votes },
							)?;

						ElectionConsensusHistory::<T, I>::set(
							unique_monotonic_identifier,
							match &option_new_consensus {
								Some(new) => Some(ConsensusHistory {
									most_recent: new.clone(),
									lost_since: false,
								}),
								None =>
									option_consensus_history.as_ref().map(|consensus_history| {
										ConsensusHistory {
											most_recent: consensus_history.most_recent.clone(),
											lost_since: true,
										}
									}),
							},
						);
						ElectionConsensusHistoryUpToDate::<T, I>::insert(
							unique_monotonic_identifier,
							epoch_index,
						);

						if let Some(new_consensus) = option_new_consensus {
							if let Some(consensus_history) = option_consensus_history {
								if consensus_history.lost_since {
									ConsensusStatus::Gained {
										most_recent: Some(consensus_history.most_recent),
										new: new_consensus,
									}
								} else if consensus_history.most_recent != new_consensus {
									ConsensusStatus::Changed {
										previous: consensus_history.most_recent,
										new: new_consensus,
									}
								} else {
									ConsensusStatus::Unchanged { current: new_consensus }
								}
							} else {
								ConsensusStatus::Gained { most_recent: None, new: new_consensus }
							}
						} else if let Some(consensus_history) = option_consensus_history
							.filter(|consensus_history| !consensus_history.lost_since)
						{
							ConsensusStatus::Lost { previous: consensus_history.most_recent }
						} else {
							ConsensusStatus::None
						}
					},
				)
			}

			fn unsynchronised_settings() -> Result<
				<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
				CorruptStorageError,
			> {
				ElectoralUnsynchronisedSettings::<T, I>::get().ok_or_else(CorruptStorageError::new)
			}

			fn unsynchronised_state() -> Result<
				<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
				CorruptStorageError,
			> {
				ElectoralUnsynchronisedState::<T, I>::get().ok_or_else(CorruptStorageError::new)
			}

			fn unsynchronised_state_map(
				key: &<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapKey,
			) ->
				Option<
					<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapValue,
			>{
				ElectoralUnsynchronisedStateMap::<T, I>::get(key)
			}

			fn set_unsynchronised_state(
				unsynchronised_state: <T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedState,
			) {
				ElectoralUnsynchronisedState::<T, I>::put(unsynchronised_state);
			}

			fn set_unsynchronised_state_map(
				key: <T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapKey,
				value: Option<
					<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedStateMapValue,
				>,
			) {
				ElectoralUnsynchronisedStateMap::<T, I>::set(key, value);
			}
		}
	}

	// ---------------------------------------------------------------------------------------- //

	pub mod bitmap_components {
		use super::{
			BitmapComponents, Config, CorruptStorageError, Pallet, UniqueMonotonicIdentifier,
		};
		use crate::{
			electoral_system::{BitmapComponentOf, ElectoralSystemTypes},
			vote_storage::VoteStorage,
		};
		use bitvec::prelude::*;
		use cf_primitives::{AuthorityCount, EpochIndex};
		use cf_traits::EpochInfo;
		use codec::{Decode, Encode};
		use frame_system::pallet_prelude::BlockNumberFor;
		use scale_info::TypeInfo;
		use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

		#[derive(Encode, Decode, TypeInfo)]
		#[scale_info(skip_type_params(T, I))]
		pub struct ElectionBitmapComponents<T: Config<I>, I: 'static> {
			epoch: EpochIndex,
			#[allow(clippy::type_complexity)]
			bitmaps:
				Vec<(BitmapComponentOf<T::ElectoralSystemRunner>, BitVec<u8, bitvec::order::Lsb0>)>,
			#[codec(skip)]
			_phantom: core::marker::PhantomData<(T, I)>,
		}
		impl<T: Config<I>, I: 'static> ElectionBitmapComponents<T, I> {
			fn inner_with<
				const ALWAYS_STORE_AFTER_CLOSURE: bool,
				R,
				F: for<'a> FnOnce(&'a mut Self) -> Result<R, CorruptStorageError>,
			>(
				current_epoch: EpochIndex,
				unique_monotonic_identifier: UniqueMonotonicIdentifier,
				f: F,
			) -> Result<R, CorruptStorageError> {
				let (updated, mut this) =
					if let Some(mut this) =
						BitmapComponents::<T, I>::get(unique_monotonic_identifier)
					{
						let update = this.epoch != current_epoch;

						if update {
							if this.epoch.checked_add(1) == Some(current_epoch) {
								let old_authorities =
									T::EpochInfo::authorities_at_epoch(this.epoch);
								this.debug_assert_authorities_in_order_of_indices(&old_authorities);
								this.bitmaps = this
								.bitmaps
								.into_iter()
								.map(
									|(bitmap_component, bitmap)| -> Result<_, CorruptStorageError> {
										Ok((
											bitmap_component,
											bitmap.iter_ones().try_fold(
												{
													let mut new_bitmap =
														BitVec::<u8, bitvec::order::Lsb0>::default(
														);
													new_bitmap.resize(
														T::EpochInfo::current_authority_count()
															as usize,
														false,
													);
													new_bitmap
												},
												|mut new_bitmap,
												 authority_old_index|
												 -> Result<_, CorruptStorageError> {
													if let Some(authority_new_index) =
														T::EpochInfo::authority_index(
															current_epoch,
															old_authorities
																.get(authority_old_index)
																.ok_or_else(CorruptStorageError::new)?,
														) {
														let authority_new_index =
															authority_new_index as usize;
														debug_assert!(
															authority_new_index <= new_bitmap.len()
														);
														*new_bitmap
															.get_mut(authority_new_index)
															.ok_or_else(CorruptStorageError::new)? = true;
													}
													Ok(new_bitmap)
												},
											)?,
										))
									},
								)
								.collect::<Result<_, _>>()?;
							} else {
								// If we skipped multiple epochs then we should not transition any
								// votes, as only votes from validators who were consistently
								// authorites between this.epoch and current_epoch should have their
								// votes kept across epoch transitions to avoid unexpected
								// behaviours.
								//
								// Note this is *NOT* done for IndividualComponents, and so those
								// components/votes may be kept across periods when the voter wasn't
								// an authority.
								for (_bitmap_component, bitmap) in this.bitmaps.iter_mut() {
									bitmap.fill(false);
								}
							}

							this.epoch = current_epoch;
						}

						(update, this)
					} else {
						(
							false,
							Self {
								epoch: current_epoch,
								bitmaps: Default::default(),
								_phantom: Default::default(),
							},
						)
					};

				let r = f(&mut this)?;

				if ALWAYS_STORE_AFTER_CLOSURE || updated {
					// Remove references late to avoid deleting shared data that we will add
					// references to inside `f`.
					this.bitmaps.retain(|(bitmap_component, bitmap)| {
						let retain = bitmap.any();
						if !retain {
							<<T::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as VoteStorage>::visit_shared_data_references_in_bitmap_component(
								bitmap_component,
								|shared_data_hash| {
									Pallet::<T, I>::remove_shared_data_reference(
										shared_data_hash,
										unique_monotonic_identifier,
									);
								}
							);
						}
						retain
					});
					BitmapComponents::<T, I>::set(
						unique_monotonic_identifier,
						if this.bitmaps.is_empty() { None } else { Some(this) },
					);
				}

				Ok(r)
			}

			pub(crate) fn with<R, F: for<'a> FnOnce(&'a Self) -> Result<R, CorruptStorageError>>(
				current_epoch: EpochIndex,
				unique_monotonic_identifier: UniqueMonotonicIdentifier,
				f: F,
			) -> Result<R, CorruptStorageError> {
				Self::inner_with::<false, _, _>(
					current_epoch,
					unique_monotonic_identifier,
					|this| f(&*this),
				)
			}

			pub(super) fn with_mut<
				R,
				F: for<'a> FnOnce(&'a mut Self) -> Result<R, CorruptStorageError>,
			>(
				current_epoch: EpochIndex,
				unique_monotonic_identifier: UniqueMonotonicIdentifier,
				f: F,
			) -> Result<R, CorruptStorageError> {
				Self::inner_with::<true, _, _>(current_epoch, unique_monotonic_identifier, f)
			}

			pub(super) fn add(
				&mut self,
				authority_index: AuthorityCount,
				bitmap_component: BitmapComponentOf<T::ElectoralSystemRunner>,
				unique_monotonic_identifier: UniqueMonotonicIdentifier,
				block_number: BlockNumberFor<T>,
			) -> Result<(), CorruptStorageError> {
				// We don't need to delete existing because we remove the existing in `take` which
				// is called before this.

				let authority_index = authority_index as usize;
				if let Some((_existing_bitmap_component, existing_bitmap)) =
					self.bitmaps.iter_mut().find(|(existing_bitmap_component, _existing_bitmap)| {
						bitmap_component == *existing_bitmap_component
					}) {
					*existing_bitmap
						.get_mut(authority_index)
						.ok_or_else(CorruptStorageError::new)? = true;
				} else {
					self.bitmaps.push((bitmap_component.clone(), {
						let mut bitmap = BitVec::default();
						bitmap.resize(T::EpochInfo::current_authority_count() as usize, false);
						*bitmap.get_mut(authority_index).ok_or_else(CorruptStorageError::new)? =
							true;
						bitmap
					}));
					<<T::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as VoteStorage>::visit_shared_data_references_in_bitmap_component(
						&bitmap_component,
						|shared_data_hash| {
							Pallet::<T, I>::add_shared_data_reference(
								shared_data_hash,
								unique_monotonic_identifier,
								block_number,
							);
						}
					);
				}

				Ok(())
			}

			pub(super) fn take(
				&mut self,
				authority_index: AuthorityCount,
			) -> Result<Option<BitmapComponentOf<T::ElectoralSystemRunner>>, CorruptStorageError>
			{
				let authority_index = authority_index as usize;
				Ok(self
					.bitmaps
					.iter_mut()
					.try_find(|(_bitmap_component, bitmap)| -> Result<_, CorruptStorageError> {
						Ok(*bitmap.get(authority_index).ok_or_else(CorruptStorageError::new)?)
					})?
					.map(|(bitmap_component, bitmap)| {
						bitmap.set(authority_index, false);
						bitmap_component.clone()
					}))
			}

			pub(super) fn get(
				&self,
				authority_index: AuthorityCount,
			) -> Result<Option<BitmapComponentOf<T::ElectoralSystemRunner>>, CorruptStorageError>
			{
				Ok(self
					.bitmaps
					.iter()
					.try_find(|(_bitmap_component, bitmap)| -> Result<_, CorruptStorageError> {
						Ok(*bitmap
							.get(authority_index as usize)
							.ok_or_else(CorruptStorageError::new)?)
					})?
					.map(|(bitmap_component, _)| bitmap_component.clone()))
			}

			pub(super) fn get_all(
				&self,
				current_authorities: &[T::ValidatorId],
			) -> Result<
				BTreeMap<T::ValidatorId, BitmapComponentOf<T::ElectoralSystemRunner>>,
				CorruptStorageError,
			> {
				self.debug_assert_authorities_in_order_of_indices(current_authorities);
				self.bitmaps
					.iter()
					.flat_map(|(bitmap_component, bitmap)| {
						debug_assert_eq!(bitmap.len(), current_authorities.len());
						bitmap.iter_ones().map(|index| {
							current_authorities.get(index).ok_or_else(CorruptStorageError::new).map(
								|validator_id| (validator_id.clone(), bitmap_component.clone()),
							)
						})
					})
					.collect()
			}

			pub(super) fn clear(unique_monotonic_identifier: UniqueMonotonicIdentifier) {
				if let Some(this) = BitmapComponents::<T, I>::get(unique_monotonic_identifier) {
					for (bitmap_component, _) in this.bitmaps {
						<<T::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as VoteStorage>::visit_shared_data_references_in_bitmap_component(
							&bitmap_component,
							|shared_data_hash| {
								Pallet::<T, I>::remove_shared_data_reference(
									shared_data_hash,
									unique_monotonic_identifier,
								);
							}
						);
					}
					BitmapComponents::<T, I>::set(unique_monotonic_identifier, None);
				}
			}

			fn debug_assert_authorities_in_order_of_indices(&self, authorities: &[T::ValidatorId]) {
				debug_assert!(authorities.iter().enumerate().all(|(index, validator_id)| {
					Some(index) ==
						T::EpochInfo::authority_index(self.epoch, validator_id)
							.map(|authority_index| authority_index as usize)
				}));
			}
		}
	}

	// ---------------------------------------------------------------------------------------- //

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		#[pallet::call_index(0)]
		#[pallet::weight((T::WeightInfo::vote(authority_votes.len() as u32), DispatchClass::Operational))]
		pub fn vote(
			origin: OriginFor<T>,
			authority_votes: BoundedBTreeMap<
				ElectionIdentifierOf<T::ElectoralSystemRunner>,
				AuthorityVoteOf<T::ElectoralSystemRunner>,
				ConstU32<MAXIMUM_VOTES_PER_EXTRINSIC>,
			>,
		) -> DispatchResult {
			let (epoch_index, authority, authority_index) = Self::ensure_can_vote(origin)?;

			ensure!(!authority_votes.is_empty(), Error::<T, I>::NoVotesSpecified);
			ensure!(
				ContributingAuthorities::<T, I>::contains_key(&authority),
				Error::<T, I>::NotContributing
			);

			for (election_identifier, authority_vote) in authority_votes {
				// if an identifier refers to a non existent election, skip this vote,
				// but continue processing others.
				let unique_monotonic_identifier = if let Ok(unique_monotonic_identifier) =
					Self::ensure_election_exists(election_identifier)
				{
					unique_monotonic_identifier
				} else {
					Self::deposit_event(Event::UnknownElection(election_identifier));
					continue;
				};

				let (partial_vote, option_vote) = match authority_vote {
					AuthorityVote::PartialVote(partial_vote) => (partial_vote, None),
					AuthorityVote::Vote(vote) => (
						VoteStorageOf::<T::ElectoralSystemRunner>::vote_into_partial_vote(
							&vote,
							|shared_data| SharedDataHash::of(&shared_data),
						),
						Some(vote),
					),
				};

				Self::handle_corrupt_storage(Self::take_vote_and_then(
					epoch_index,
					unique_monotonic_identifier,
					&authority,
					authority_index,
					|option_existing_vote, election_bitmap_components| {
						let components = <<T::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as VoteStorage>::partial_vote_into_components(
							<T::ElectoralSystemRunner as ElectoralSystemRunner>::generate_vote_properties(
								election_identifier,
								option_existing_vote,
								&partial_vote,
							)?,
							partial_vote
						)?;

						let block_number = frame_system::Pallet::<T>::current_block_number();
						if let Some(bitmap_component) = components.bitmap_component {
							// Store bitmap component and update shared data reference counts
							election_bitmap_components.add(
								authority_index,
								bitmap_component,
								unique_monotonic_identifier,
								block_number,
							)?;
						}
						if let Some((properties, individual_component)) =
							components.individual_component
						{
							// Update shared data reference counts
							<<T::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as VoteStorage>::visit_shared_data_references_in_individual_component(
								&individual_component,
								|shared_data_hash| Self::add_shared_data_reference(shared_data_hash, unique_monotonic_identifier, block_number),
							);
							// Store individual component
							IndividualComponents::<T, I>::set(
								unique_monotonic_identifier,
								authority.clone(),
								Some((properties, individual_component)),
							);
						}

						Ok(())
					},
				))?;

				// Insert any `SharedData` provided as part of the `Vote`.
				if let Some(vote) = option_vote {
					<<T::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as VoteStorage>::visit_shared_data_in_vote(
						vote,
						|shared_data| Self::inner_provide_shared_data(shared_data),
					)
					.inspect_err(|error| {
						// Should be impossible for SharedData to be unreferenced
						// (`UnreferencedSharedData`) here, but with poor `VoteStorage` impls it
						// could happen. Particularly if the `VoteStorage` visit functions do not
						// consistently provide the same data/hashes, i.e. are non-deterministic, or
						// base their behaviour on mutable values not passed to them.
						debug_assert!(false, "{error:?}");
					})?;
				}
			}

			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight((T::WeightInfo::provide_shared_data(), DispatchClass::Operational))]
		pub fn provide_shared_data(
			origin: OriginFor<T>,
			shared_data: <<T::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as VoteStorage>::SharedData,
		) -> DispatchResult {
			Self::ensure_can_vote(origin)?;
			Self::inner_provide_shared_data(shared_data)?;
			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight((T::WeightInfo::ignore_my_votes(), DispatchClass::Operational))]
		pub fn ignore_my_votes(origin: OriginFor<T>) -> DispatchResult {
			let (epoch_index, authority, authority_index) = Self::ensure_can_vote(origin)?;

			if ContributingAuthorities::<T, I>::take(&authority).is_some() {
				Self::recheck_contributed_to_consensuses(epoch_index, &authority, authority_index)?;
			}

			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight((T::WeightInfo::stop_ignoring_my_votes(), DispatchClass::Operational))]
		pub fn stop_ignoring_my_votes(origin: OriginFor<T>) -> DispatchResult {
			let (epoch_index, authority, authority_index) = Self::ensure_can_vote(origin)?;

			if !ContributingAuthorities::<T, I>::contains_key(&authority) {
				Self::recheck_contributed_to_consensuses(epoch_index, &authority, authority_index)?;
			}
			ContributingAuthorities::<T, I>::insert(authority, ());

			Ok(())
		}

		#[pallet::call_index(4)]
		#[pallet::weight((T::WeightInfo::delete_vote(), DispatchClass::Operational))]
		pub fn delete_vote(
			origin: OriginFor<T>,
			election_identifier: ElectionIdentifierOf<T::ElectoralSystemRunner>,
		) -> DispatchResult {
			let (epoch_index, authority, authority_index) = Self::ensure_can_vote(origin)?;
			let unique_monotonic_identifier = Self::ensure_election_exists(election_identifier)?;

			Self::handle_corrupt_storage(Self::take_vote_and_then(
				epoch_index,
				unique_monotonic_identifier,
				&authority,
				authority_index,
				|_, _| Ok(()),
			))?;
			Ok(())
		}

		// ------------------------------------------------------------------------------------ //

		#[pallet::call_index(16)]
		#[pallet::weight(T::WeightInfo::initialize())]
		pub fn initialize(
			origin: OriginFor<T>,
			initial_state: InitialStateOf<T, I>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			Self::internally_initialize(initial_state)?;
			Ok(())
		}

		#[pallet::call_index(17)]
		#[pallet::weight(T::WeightInfo::update_settings())]
		pub fn update_settings(
			origin: OriginFor<T>,
			unsynchronised_settings: Option<
				<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralUnsynchronisedSettings,
			>,
			settings: Option<<T::ElectoralSystemRunner as ElectoralSystemTypes>::ElectoralSettings>,
			ignore_corrupt_storage: CorruptStorageAdherance,
		) -> DispatchResult {
			Self::ensure_governance(origin, ignore_corrupt_storage)?;
			if let Some(unsynchronised_settings) = unsynchronised_settings {
				ElectoralUnsynchronisedSettings::<T, I>::put(unsynchronised_settings);
			}
			if let Some(settings) = settings {
				// This cannot effect settings of any election as all elections have IDs strictly
				// lower than `NextElectionIdentifier`.
				ElectoralSettings::<T, I>::insert(NextElectionIdentifier::<T, I>::get(), settings);
			}
			Ok(())
		}

		#[pallet::call_index(18)]
		#[pallet::weight(T::WeightInfo::set_shared_data_reference_lifetime())]
		pub fn set_shared_data_reference_lifetime(
			origin: OriginFor<T>,
			blocks: BlockNumberFor<T>,
			ignore_corrupt_storage: CorruptStorageAdherance,
		) -> DispatchResult {
			Self::ensure_governance(origin, ignore_corrupt_storage)?;
			SharedDataReferenceLifetime::<T, I>::set(blocks);
			Ok(())
		}

		// ------------------------------------------------------------------------------------ //

		// These are governance extrinsics designed to help fix any potential issues that may arise,
		// but they should not be needed unless there is a bug.

		#[pallet::call_index(32)]
		#[pallet::weight(T::WeightInfo::clear_election_votes())]
		pub fn clear_election_votes(
			origin: OriginFor<T>,
			election_identifier: ElectionIdentifierOf<T::ElectoralSystemRunner>,
			ignore_corrupt_storage: CorruptStorageAdherance,
			check_election_exists: bool,
		) -> DispatchResult {
			Self::ensure_governance(origin, ignore_corrupt_storage)?;
			if check_election_exists {
				Self::ensure_election_exists(election_identifier)?;
			}

			RunnerStorageAccess::<T, I>::clear_election_votes(
				*election_identifier.unique_monotonic(),
			);

			Ok(())
		}

		#[pallet::call_index(33)]
		#[pallet::weight(T::WeightInfo::invalidate_election_consensus_cache())]
		pub fn invalidate_election_consensus_cache(
			origin: OriginFor<T>,
			election_identifier: ElectionIdentifierOf<T::ElectoralSystemRunner>,
			ignore_corrupt_storage: CorruptStorageAdherance,
			check_election_exists: bool,
		) -> DispatchResult {
			Self::ensure_governance(origin, ignore_corrupt_storage)?;
			let unique_monotonic_identifier = if check_election_exists {
				Self::ensure_election_exists(election_identifier)?
			} else {
				*election_identifier.unique_monotonic()
			};

			ElectionConsensusHistoryUpToDate::<T, I>::remove(unique_monotonic_identifier);

			Ok(())
		}

		#[pallet::call_index(34)]
		#[pallet::weight(T::WeightInfo::pause_elections())]
		pub fn pause_elections(origin: OriginFor<T>) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			match Status::<T, I>::get() {
				None => Err(Error::<T, I>::Uninitialized.into()),
				Some(ElectionPalletStatus::Paused { .. }) => Err(Error::<T, I>::Paused.into()),
				Some(_) => {
					Status::<T, I>::put(ElectionPalletStatus::Paused {
						detected_corrupt_storage: false,
					});
					Ok(())
				},
			}
		}

		#[pallet::call_index(35)]
		#[pallet::weight(T::WeightInfo::unpause_elections())]
		pub fn unpause_elections(
			origin: OriginFor<T>,
			require_votes_cleared: bool,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			match Status::<T, I>::get() {
				None => Err(Error::<T, I>::Uninitialized.into()),
				Some(ElectionPalletStatus::Paused { detected_corrupt_storage: true }) =>
					Err(Error::<T, I>::CorruptStorage.into()),
				Some(ElectionPalletStatus::Paused { .. }) => {
					ensure!(
						!require_votes_cleared ||
							(SharedDataReferenceCount::<T, I>::iter_keys().next().is_none() &&
								SharedData::<T, I>::iter_keys().next().is_none() &&
								BitmapComponents::<T, I>::iter_keys().next().is_none() &&
								IndividualComponents::<T, I>::iter_keys().next().is_none() &&
								ElectionConsensusHistoryUpToDate::<T, I>::iter_keys()
									.next()
									.is_none()),
						Error::<T, I>::VotesNotCleared
					);
					Status::<T, I>::put(ElectionPalletStatus::Running);
					Ok(())
				},
				Some(_) => Err(Error::<T, I>::NotPaused.into()),
			}
		}

		#[pallet::call_index(36)]
		#[pallet::weight(T::WeightInfo::clear_all_votes(*limit, *limit, *limit, *limit, *limit,))]
		pub fn clear_all_votes(
			origin: OriginFor<T>,
			limit: u32,
			ignore_corrupt_storage: CorruptStorageAdherance,
		) -> DispatchResult {
			Self::ensure_governance(origin, ignore_corrupt_storage)?;

			Self::deposit_event(
				// Note: non-short circuiting `&` is to ensure as much data as possible is deleted
				// from each storage item.
				if SharedDataReferenceCount::<T, I>::clear(limit, None).maybe_cursor.is_none() &
					SharedData::<T, I>::clear(limit, None).maybe_cursor.is_none() &
					BitmapComponents::<T, I>::clear(limit, None).maybe_cursor.is_none() &
					IndividualComponents::<T, I>::clear(limit, None).maybe_cursor.is_none() &
					ElectionConsensusHistoryUpToDate::<T, I>::clear(limit, None)
						.maybe_cursor
						.is_none()
				{
					Event::<T, I>::AllVotesCleared
				} else {
					// In this case Vote data will be invalid. For example
					// `SharedDataReferenceCount` entries will not be correct, but we make no
					// assumptions that are broken by arbitrarily removing elements from any of
					// these storage items.
					let _ = Self::handle_corrupt_storage(Err::<core::convert::Infallible, _>(
						CorruptStorageError::new(),
					));
					Event::<T, I>::AllVotesNotCleared
				},
			);

			Ok(())
		}

		// TODO Write list of things to check before calling
		#[pallet::call_index(37)]
		#[pallet::weight(T::WeightInfo::validate_storage())]
		pub fn validate_storage(origin: OriginFor<T>) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			match Status::<T, I>::get() {
				None => Err(Error::<T, I>::Uninitialized.into()),
				Some(ElectionPalletStatus::Paused { .. }) => {
					Status::<T, I>::put(ElectionPalletStatus::Paused {
						detected_corrupt_storage: false,
					});
					Ok(())
				},
				Some(_) => Err(Error::<T, I>::NotPaused.into()),
			}
		}
	}

	// ---------------------------------------------------------------------------------------- //

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		fn on_finalize(block_number: BlockNumberFor<T>) {
			if let Some(status) = Status::<T, I>::get() {
				match status {
					ElectionPalletStatus::Paused { detected_corrupt_storage } =>
						if detected_corrupt_storage {
							Self::deposit_event(Event::<T, I>::CorruptStorage);
						},
					ElectionPalletStatus::Running => {
						let _ = Self::with_election_identifiers(|election_identifiers| {
							if Into::<sp_core::U256>::into(block_number) % BLOCKS_BETWEEN_CLEANUP ==
								sp_core::U256::zero()
							{
								let minimum_election_identifiers = election_identifiers
									.iter()
									.copied()
									.map(|election_identifier| {
										*election_identifier.unique_monotonic()
									})
									.min()
									.unwrap_or_default();
								let mut settings_boundaries =
									ElectoralSettings::<T, I>::iter_keys().collect::<Vec<_>>();
								settings_boundaries.sort();
								for setting_boundary in &settings_boundaries
										[..settings_boundaries[..]
											.partition_point(|&setting_boundary| {
												setting_boundary <= minimum_election_identifiers
											})
											.saturating_sub(1) /*Keep the latest settings lower than the minimum election identifier, i.e. the settings referenced by the election with the minimum election identifier*/]
								{
									ElectoralSettings::<T, I>::remove(setting_boundary);
								}

								let current_authorities = T::EpochInfo::current_authorities();
								for validator in
									ContributingAuthorities::<T, I>::iter_keys().collect::<Vec<_>>()
								{
									if !current_authorities.contains(&validator) {
										ContributingAuthorities::<T, I>::remove(validator);
									}
								}
							}

							T::ElectoralSystemRunner::on_finalize(election_identifiers)?;

							Ok(())
						});
					},
				}
			}
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), DispatchError> {
			Self::do_try_state()
		}

		fn integrity_test() {
			let properties_keys = ElectionProperties::<T, I>::iter_keys()
				.map(|id| *id.unique_monotonic())
				.collect::<BTreeSet<_>>();
			let state_keys = ElectionState::<T, I>::iter_keys().collect::<BTreeSet<_>>();
			debug_assert_eq!(
				properties_keys,
				state_keys,
				"Expected election properties and state to have the same keys. In properties but not in state: {:?}. In state but not in properties: {:?}.",
				properties_keys.difference(&state_keys).collect::<Vec<_>>(),
				state_keys.difference(&properties_keys).collect::<Vec<_>>(),
			);
		}
	}

	// ---------------------------------------------------------------------------------------- //

	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum CorruptStorageAdherance {
		Ignore,
		Heed,
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// This function allows other pallets to initialize an Elections pallet, instead of needing
		/// to initialize it via a governance extrinsic or at genesis.
		pub fn internally_initialize(
			initial_state: InitialStateOf<T, I>,
		) -> Result<(), Error<T, I>> {
			ensure!(Status::<T, I>::get().is_none(), Error::<T, I>::AlreadyInitialized);
			ElectoralUnsynchronisedState::<T, I>::put(initial_state.unsynchronised_state);
			ElectoralUnsynchronisedSettings::<T, I>::put(initial_state.unsynchronised_settings);
			ElectoralSettings::<T, I>::insert(
				NextElectionIdentifier::<T, I>::get(),
				initial_state.settings,
			);
			Status::<T, I>::put(ElectionPalletStatus::Running);
			Ok(())
		}

		/// Provides access into the ElectoralSystem's current election
		/// identifiers.
		pub fn with_election_identifiers<
			R,
			F: FnOnce(
				Vec<ElectionIdentifierOf<T::ElectoralSystemRunner>>,
			) -> Result<R, CorruptStorageError>,
		>(
			f: F,
		) -> Result<R, DispatchError> {
			Self::with_status_check(|| {
				let mut election_identifiers =
					ElectionProperties::<T, I>::iter_keys().collect::<Vec<_>>();
				election_identifiers.sort();
				f(election_identifiers)
			})
		}

		pub fn with_status_check<R, F: FnOnce() -> Result<R, CorruptStorageError>>(
			f: F,
		) -> Result<R, DispatchError> {
			if Status::<T, I>::get().is_some() {
				Self::handle_corrupt_storage(f()).map_err(Into::into)
			} else {
				Self::deposit_event(Event::<T, I>::Uninitialized);
				Err(Error::<T, I>::Uninitialized.into())
			}
		}

		/// Returns all the current elections (with their details), the validators current votes and
		/// if the validator should vote in each election.
		#[allow(clippy::type_complexity)]
		pub fn electoral_data(validator_id: &T::ValidatorId) -> Option<ElectoralDataFor<T, I>> {
			use frame_support::traits::OriginTrait;

			Pallet::<T, I>::ensure_can_vote(OriginFor::<T>::signed(validator_id.clone().into()))
				.ok()
				.and_then(|(epoch_index, authority, authority_index)| {
					let block_number = frame_system::Pallet::<T>::current_block_number();

					Some(ElectoralData {
						current_elections: Self::with_election_identifiers(
							|election_identifiers| {
								election_identifiers
									.into_iter()
									.map(|election_identifier| {
										let unique_monotonic_identifier =
											*election_identifier.unique_monotonic();

										let mut contains_timed_out_shared_data_references = false;
										let option_current_authority_vote =
											Pallet::<T, I>::get_vote(
												epoch_index,
												unique_monotonic_identifier,
												&authority,
												authority_index,
												|unprovided_shared_data_hash| {
													let option_reference_details =
														SharedDataReferenceCount::<T, I>::get(
															unprovided_shared_data_hash,
															unique_monotonic_identifier,
														);
													if option_reference_details.is_none() ||
														option_reference_details.unwrap().expires <
															block_number
													{
														contains_timed_out_shared_data_references =
															true;
													}
												},
											)?;

										Ok((
												election_identifier,
												AuthorityElectionData {
													settings: RunnerStorageAccess::<T, I>::electoral_settings_for_election(*election_identifier.unique_monotonic())?,
													properties: RunnerStorageAccess::<T, I>::election_properties(election_identifier)?,
													// We report the vote to the engine even though it is timeouted so the engine
													// knows to delete it. As it may still later to reconstructed if the right
													// `SharedData` is provided, unless it is delete.
													option_existing_vote: option_current_authority_vote.as_ref().map(|(_, authority_vote)| {
														authority_vote.clone()
													}),
													is_vote_desired: <T::ElectoralSystemRunner as ElectoralSystemRunner>::is_vote_desired(
														election_identifier,
														option_current_authority_vote.filter(|_| !contains_timed_out_shared_data_references),
													)?,
												},
											))
									})
									.collect::<Result<BTreeMap<_, _>, _>>()
							},
						)
						.ok()?,
						unprovided_shared_data_hashes: {
							let mut unprovided_shared_data_hashes = BTreeMap::<
								SharedDataHash,
								ReferenceDetails<BlockNumberFor<T>>,
							>::new();

							for (shared_data_hash, _election_identifier, reference_details) in
								SharedDataReferenceCount::<T, I>::iter()
							{
								if SharedData::<T, I>::get(shared_data_hash).is_none() {
									// We use the first created unexpired reference
									if reference_details.created <= block_number &&
										block_number < reference_details.expires &&
										unprovided_shared_data_hashes
											.get(&shared_data_hash)
											.is_none_or(|previous_reference_details| {
												reference_details.created <
													previous_reference_details.created
											}) {
										unprovided_shared_data_hashes
											.insert(shared_data_hash, reference_details);
									}
								}
							}

							unprovided_shared_data_hashes
						},
						contributing: ContributingAuthorities::<T, I>::contains_key(&authority),
						authority_count: T::EpochInfo::current_authority_count(),
					})
				})
		}

		pub fn filter_votes(
			validator_id: &T::ValidatorId,
			proposed_votes: BTreeMap<
				ElectionIdentifierOf<T::ElectoralSystemRunner>,
				VoteOf<T::ElectoralSystemRunner>,
			>,
		) -> BTreeSet<ElectionIdentifierOf<T::ElectoralSystemRunner>> {
			use frame_support::traits::OriginTrait;

			if let Ok((epoch_index, authority, authority_index)) =
				Pallet::<T, I>::ensure_can_vote(OriginFor::<T>::signed(validator_id.clone().into()))
			{
				let block_number = frame_system::Pallet::<T>::current_block_number();

				Self::with_election_identifiers(|election_identifiers| {
					election_identifiers
						.into_iter()
						.map(|election_identifier| {
							Ok((
								election_identifier,
								if let Some(proposed_vote) =
									proposed_votes.get(&election_identifier)
								{
									let unique_monotonic_identifier =
										*election_identifier.unique_monotonic();

									let mut contains_timed_out_shared_data_references = false;
									let option_current_authority_vote = Pallet::<T, I>::get_vote(
										epoch_index,
										unique_monotonic_identifier,
										&authority,
										authority_index,
										|unprovided_shared_data_hash| {
											let option_reference_details =
												SharedDataReferenceCount::<T, I>::get(
													unprovided_shared_data_hash,
													unique_monotonic_identifier,
												);
											if option_reference_details.is_none() ||
												option_reference_details.unwrap().expires <
													block_number
											{
												contains_timed_out_shared_data_references = true;
											}
										},
									)?;

									if let Some((
										existing_vote_properties,
										existing_authority_vote,
									)) = option_current_authority_vote
										.filter(|_| !contains_timed_out_shared_data_references)
									{
										<T::ElectoralSystemRunner as ElectoralSystemRunner>::is_vote_needed(
												(
													existing_vote_properties,
													match &existing_authority_vote {
														AuthorityVote::Vote(existing_vote) => {
															VoteStorageOf::<T::ElectoralSystemRunner>::vote_into_partial_vote(
																existing_vote,
																|shared_data| SharedDataHash::of(&shared_data)
															)
														},
														AuthorityVote::PartialVote(existing_partial_vote) => existing_partial_vote.clone(),
													},
													existing_authority_vote,
												),
												(
													VoteStorageOf::<T::ElectoralSystemRunner>::vote_into_partial_vote(
														proposed_vote,
														|shared_data| SharedDataHash::of(&shared_data)
													),
													proposed_vote.clone(),
												),
											)
									} else {
										true
									}
								} else {
									false
								},
							))
						})
						.filter_ok(|(_election_identifier, needed)| *needed)
						.map_ok(|(election_identifier, _needed)| (election_identifier))
						.collect::<Result<BTreeSet<_>, _>>()
				})
				.unwrap_or_default()
			} else {
				Default::default()
			}
		}

		pub(crate) fn recheck_contributed_to_consensuses(
			epoch_index: EpochIndex,
			authority: &T::ValidatorId,
			authority_index: AuthorityCount,
		) -> Result<(), Error<T, I>> {
			for unique_monotonic_identifier in
				ElectionConsensusHistoryUpToDate::<T, I>::iter_keys().collect::<Vec<_>>()
			{
				if Self::handle_corrupt_storage(Self::get_vote(
					epoch_index,
					unique_monotonic_identifier,
					authority,
					authority_index,
					|_unprovided_shared_data_hash| (),
				))?
				.is_some_and(|(_, authority_vote)| matches!(authority_vote, AuthorityVote::Vote(_)))
				{
					ElectionConsensusHistoryUpToDate::<T, I>::remove(unique_monotonic_identifier);
				}
			}
			Ok(())
		}

		fn take_vote_and_then<
			R,
			F: for<'a> FnOnce(
				Option<(
					VotePropertiesOf<T::ElectoralSystemRunner>,
					AuthorityVoteOf<T::ElectoralSystemRunner>,
				)>,
				&'a mut ElectionBitmapComponents<T, I>,
			) -> Result<R, CorruptStorageError>,
		>(
			epoch_index: EpochIndex,
			unique_monotonic_identifier: UniqueMonotonicIdentifier,
			authority: &T::ValidatorId,
			authority_index: AuthorityCount,
			f: F,
		) -> Result<R, CorruptStorageError> {
			ElectionBitmapComponents::<T, I>::with_mut(
				epoch_index,
				unique_monotonic_identifier,
				|election_bitmap_components| {
					let individual_component =
						IndividualComponents::<T, I>::take(unique_monotonic_identifier, authority);

					let r = f(
						<<T::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as VoteStorage>::components_into_authority_vote(
							VoteComponents {
								bitmap_component: election_bitmap_components.take(authority_index)?,
								individual_component: individual_component.clone(),
							},
							|_| Ok(None),
						)?,
						election_bitmap_components,
					)?;

					// Remove references late to avoid deleting shared data that we will add
					// references to inside `f`.
					if let Some((_properties, individual_component)) = individual_component {
						<<T::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as VoteStorage>::visit_shared_data_references_in_individual_component(
							&individual_component,
							|shared_data_hash| Self::remove_shared_data_reference(shared_data_hash, unique_monotonic_identifier),
						);
					}

					if ContributingAuthorities::<T, I>::contains_key(authority) {
						ElectionConsensusHistoryUpToDate::<T, I>::remove(
							unique_monotonic_identifier,
						);
					}

					Ok(r)
				},
			)
		}

		#[allow(clippy::type_complexity)]
		fn get_vote<VisitUnprovidedSharedData: FnMut(SharedDataHash)>(
			epoch_index: EpochIndex,
			unique_monotonic_identifier: UniqueMonotonicIdentifier,
			authority: &T::ValidatorId,
			authority_index: AuthorityCount,
			mut visit_unprovided_shared_data: VisitUnprovidedSharedData,
		) -> Result<
			Option<(
				VotePropertiesOf<T::ElectoralSystemRunner>,
				AuthorityVoteOf<T::ElectoralSystemRunner>,
			)>,
			CorruptStorageError,
		> {
			<<T::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as VoteStorage>::components_into_authority_vote(
				VoteComponents {
					bitmap_component: ElectionBitmapComponents::<T, I>::with(
						epoch_index,
						unique_monotonic_identifier,
						|election_bitmap_components| {
							election_bitmap_components.get(authority_index)
						},
					)?,
					individual_component: IndividualComponents::<T, I>::get(
						unique_monotonic_identifier,
						authority,
					),
				},
				|shared_data_hash| {
					Ok(if let Some(shared_data) = SharedData::<T, I>::get(shared_data_hash) {
						Some(shared_data)
					} else {
						visit_unprovided_shared_data(shared_data_hash);
						None
					})
				},
			)
		}

		pub(crate) fn ensure_can_vote(
			origin: OriginFor<T>,
		) -> Result<(EpochIndex, T::ValidatorId, AuthorityCount), DispatchError> {
			let epoch_index = T::EpochInfo::epoch_index();
			let validator_id = T::AccountRoleRegistry::ensure_validator(origin)?.into();
			let authority_index = T::EpochInfo::authority_index(epoch_index, &validator_id);
			ensure!(authority_index.is_some(), Error::<T, I>::Unauthorised);
			ensure!(
				matches!(Status::<T, I>::get(), Some(ElectionPalletStatus::Running)),
				Error::<T, I>::Paused
			);
			Ok((epoch_index, validator_id, authority_index.unwrap()))
		}
		fn ensure_governance(
			origin: OriginFor<T>,
			ignore_corrupt_storage: CorruptStorageAdherance,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			if let Some(status) = Status::<T, I>::get() {
				ensure!(
					!matches!(
						status,
						ElectionPalletStatus::Paused { detected_corrupt_storage: true }
					) || matches!(ignore_corrupt_storage, CorruptStorageAdherance::Ignore),
					Error::<T, I>::CorruptStorage
				);
				Ok(())
			} else {
				Err(Error::<T, I>::Uninitialized.into())
			}
		}
		fn ensure_election_exists(
			election_identifier: ElectionIdentifierOf<T::ElectoralSystemRunner>,
		) -> Result<UniqueMonotonicIdentifier, Error<T, I>> {
			ensure!(
				ElectionProperties::<T, I>::contains_key(election_identifier),
				Error::<T, I>::UnknownElection
			);
			Ok(*election_identifier.unique_monotonic())
		}

		fn handle_corrupt_storage<Ok>(
			result: Result<Ok, CorruptStorageError>,
		) -> Result<Ok, Error<T, I>> {
			match result {
				Ok(ok) => Ok(ok),
				Err(_) => {
					Self::deposit_event(Event::<T, I>::CorruptStorage);
					Status::<T, I>::put(ElectionPalletStatus::Paused {
						detected_corrupt_storage: true,
					});
					Err(Error::<T, I>::CorruptStorage)
				},
			}
		}

		fn add_shared_data_reference(
			shared_data_hash: SharedDataHash,
			unique_monotonic_identifier: UniqueMonotonicIdentifier,
			block_number: BlockNumberFor<T>,
		) {
			let mut reference_details = SharedDataReferenceCount::<T, I>::get(
				shared_data_hash,
				unique_monotonic_identifier,
			)
			.unwrap_or_else(|| ReferenceDetails {
				count: 0,
				created: block_number,
				expires: block_number + SharedDataReferenceLifetime::<T, I>::get(),
			});

			reference_details.count = reference_details.count.saturating_add(1);

			SharedDataReferenceCount::<T, I>::insert(
				shared_data_hash,
				unique_monotonic_identifier,
				reference_details,
			);
		}
		fn remove_shared_data_reference(
			shared_data_hash: SharedDataHash,
			unique_monotonic_identifier: UniqueMonotonicIdentifier,
		) {
			if let Some(mut reference_details) =
				SharedDataReferenceCount::<T, I>::get(shared_data_hash, unique_monotonic_identifier)
			{
				reference_details.count = reference_details.count.saturating_sub(1);
				if reference_details.count == 0 {
					SharedDataReferenceCount::<T, I>::remove(
						shared_data_hash,
						unique_monotonic_identifier,
					);
					if !SharedDataReferenceCount::<T, I>::contains_prefix(shared_data_hash) {
						SharedData::<T, I>::remove(shared_data_hash);
					}
				} else {
					SharedDataReferenceCount::<T, I>::insert(
						shared_data_hash,
						unique_monotonic_identifier,
						reference_details,
					);
				}
			}
		}
		fn inner_provide_shared_data(
			shared_data: <<T::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as VoteStorage>::SharedData,
		) -> Result<(), Error<T, I>> {
			let shared_data_hash = SharedDataHash::of(&shared_data);
			let (unique_monotonic_identifiers, reference_details): (Vec<_>, Vec<_>) =
				SharedDataReferenceCount::<T, I>::iter_prefix(shared_data_hash).unzip();

			if reference_details
				.into_iter()
				.any(|reference_details| reference_details.count != 0)
			{
				SharedData::<T, I>::insert(shared_data_hash, shared_data);
				for unique_monotonic_identifier in unique_monotonic_identifiers {
					ElectionConsensusHistoryUpToDate::<T, I>::remove(unique_monotonic_identifier);
				}
				Ok(())
			} else {
				Err(Error::<T, I>::UnreferencedSharedData)
			}
		}
	}

	#[cfg(any(test, feature = "try-runtime"))]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Ensure the correctness of the state of this pallet.
		///
		/// These invariants should be valid before or after each state transition of this pallet.
		///
		/// # Invariants
		/// * All keys in `ElectionProperties` and `ElectionState` should match
		/// * All keys in `SharedData` should have an entry in `SharedDataReferenceCount`
		/// * `ElectionConsensusHistoryUpToDate` has a corresponding entry in `ElectionProperties`
		/// * `BitmapComponents` have a corresponding entry in `ElectionProperties`
		/// * `IndividualComponents` have a corresponding entry in `ElectionProperties`
		pub fn do_try_state() -> Result<(), DispatchError> {
			let properties_keys = ElectionProperties::<T, I>::iter_keys()
				.map(|id| *id.unique_monotonic())
				.collect::<BTreeSet<_>>();

			ensure!(
				properties_keys == ElectionState::<T, I>::iter_keys().collect::<BTreeSet<_>>(),
				DispatchError::Other(
					"All keys in ElectionProperties and ElectionState should match",
				)
			);

			let shared_ref_count_keys =
				SharedDataReferenceCount::<T, I>::iter_keys().collect::<Vec<_>>();
			for shared_data_key in SharedData::<T, I>::iter_keys() {
				ensure!(
					shared_ref_count_keys.iter().any(|(h, _)| *h == shared_data_key),
					DispatchError::Other(
						"All keys in SharedData should have an entry in SharedDataReferenceCount",
					)
				)
			}

			for election_id in ElectionConsensusHistoryUpToDate::<T, I>::iter_keys() {
				ensure!(
					properties_keys.contains(&election_id),
					DispatchError::Other(
						"ElectionConsensusHistoryUpToDate should have a corresponding entry in ElectionProperties"
					)
				)
			}
			for election_id in BitmapComponents::<T, I>::iter_keys() {
				ensure!(
					properties_keys.contains(&election_id),
					DispatchError::Other(
						"BitmapComponents should have a corresponding entry in ElectionProperties"
					)
				)
			}
			for (election_id, _) in IndividualComponents::<T, I>::iter_keys() {
				ensure!(
					properties_keys.contains(&election_id),
					DispatchError::Other(
						"IndividualComponents should have a corresponding entry in ElectionProperties",
					)
				)
			}

			Ok(())
		}
	}
}
