use core::cmp::Reverse;

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet};

#[cfg(feature = "runtime-benchmarks")]
use cf_chains::benchmarking_value::BenchmarkValue;
#[cfg(feature = "runtime-benchmarks")]
use cf_chains::sol::api::VaultSwapAccountAndSender;

use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusVotes, ElectionReadAccess, ElectionWriteAccess, ElectoralSystem,
		ElectoralWriteAccess, VotePropertiesOf,
	},
	vote_storage::{self, VoteStorage},
	CorruptStorageError, ElectionIdentifier,
};
use cf_chains::sol::{
	MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES,
	MAX_WAIT_BLOCKS_FOR_SWAP_ACCOUNT_CLOSURE_APICALLS,
	NONCE_AVAILABILITY_THRESHOLD_FOR_INITIATING_FETCH,
	NONCE_AVAILABILITY_THRESHOLD_FOR_INITIATING_SWAP_ACCOUNT_CLOSURES,
};

use cf_utilities::success_threshold_from_share_count;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	sp_runtime::traits::Saturating,
	Parameter,
};
use itertools::Itertools;
use sp_std::vec::Vec;

pub trait SolanaVaultSwapAccountsHook<Account, SwapDetails, E> {
	fn maybe_fetch_and_close_accounts(accounts: Vec<Account>) -> Result<(), E>;
	fn initiate_vault_swap(swap_details: SwapDetails);
	fn get_number_of_available_sol_nonce_accounts() -> usize;
}

pub trait FromSolOrNot {
	fn sol_or_not(s: &Self) -> bool;
}

pub type SolanaVaultSwapAccountsLastClosedAt<BlockNumber> = BlockNumber;

#[derive(
	Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TypeInfo, Encode, Decode, Default,
)]
pub struct SolanaVaultSwapsKnownAccounts<Account: Ord> {
	pub witnessed_open_accounts: Vec<(Account, bool)>,
	pub closure_initiated_accounts: BTreeSet<Account>,
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for SolanaVaultSwapsKnownAccounts<VaultSwapAccountAndSender> {
	fn benchmark_value() -> Self {
		Self {
			witnessed_open_accounts: sp_std::vec![(BenchmarkValue::benchmark_value(), true)],
			closure_initiated_accounts: BTreeSet::from([BenchmarkValue::benchmark_value()]),
		}
	}
}

#[derive(
	Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TypeInfo, Encode, Decode, Ord, PartialOrd,
)]
pub struct SolanaVaultSwapsVote<Account: Ord, SwapDetails: Ord + FromSolOrNot> {
	pub new_accounts: BTreeSet<(Account, Option<SwapDetails>)>,
	pub confirm_closed_accounts: BTreeSet<Account>,
}

#[cfg(feature = "runtime-benchmarks")]
impl<Account: Ord + BenchmarkValue, SwapDetails: Ord + BenchmarkValue + FromSolOrNot> BenchmarkValue
	for SolanaVaultSwapsVote<Account, SwapDetails>
{
	fn benchmark_value() -> Self {
		Self {
			new_accounts: BTreeSet::from([(
				BenchmarkValue::benchmark_value(),
				Some(BenchmarkValue::benchmark_value()),
			)]),
			confirm_closed_accounts: BTreeSet::from([BenchmarkValue::benchmark_value()]),
		}
	}
}

pub struct SolanaVaultSwapAccounts<
	Account,
	SwapDetails,
	BlockNumber,
	Settings,
	Hook,
	ValidatorId,
	E,
> {
	_phantom: core::marker::PhantomData<(
		Account,
		SwapDetails,
		BlockNumber,
		Settings,
		Hook,
		ValidatorId,
		E,
	)>,
}
impl<
		E: sp_std::fmt::Debug + 'static,
		Account: MaybeSerializeDeserialize + Member + Parameter + Ord,
		SwapDetails: MaybeSerializeDeserialize + Member + Parameter + Ord + FromSolOrNot,
		BlockNumber: MaybeSerializeDeserialize + Member + Parameter + Ord + Saturating + Into<u32> + Copy,
		Settings: Member + Parameter + MaybeSerializeDeserialize + Eq,
		Hook: SolanaVaultSwapAccountsHook<Account, SwapDetails, E> + 'static,
		ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	> ElectoralSystem
	for SolanaVaultSwapAccounts<Account, SwapDetails, BlockNumber, Settings, Hook, ValidatorId, E>
{
	type ValidatorId = ValidatorId;
	type ElectoralUnsynchronisedState = SolanaVaultSwapAccountsLastClosedAt<BlockNumber>;
	type ElectoralUnsynchronisedStateMapKey = ();
	type ElectoralUnsynchronisedStateMapValue = ();

	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = u64;
	type ElectionProperties = SolanaVaultSwapsKnownAccounts<Account>;
	type ElectionState = ();
	type Vote = vote_storage::bitmap::Bitmap<SolanaVaultSwapsVote<Account, SwapDetails>>;
	type Consensus = SolanaVaultSwapsVote<Account, SwapDetails>;
	type OnFinalizeContext = BlockNumber;
	type OnFinalizeReturn = ();

	fn generate_vote_properties(
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(())
	}

	fn is_vote_desired<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_access: &ElectionAccess,
		_current_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
	) -> Result<bool, CorruptStorageError> {
		Ok(true)
	}

	fn is_vote_needed(
		(_, current_partial_vote, _): (
			VotePropertiesOf<Self>,
			<Self::Vote as VoteStorage>::PartialVote,
			AuthorityVoteOf<Self>,
		),
		(proposed_partial_vote, _): (
			<Self::Vote as VoteStorage>::PartialVote,
			<Self::Vote as VoteStorage>::Vote,
		),
	) -> bool {
		current_partial_vote != proposed_partial_vote
	}

	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		election_identifiers: Vec<ElectionIdentifier<Self::ElectionIdentifierExtra>>,
		current_block_number: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		if let Some(election_identifier) = election_identifiers
			.into_iter()
			.at_most_one()
			.map_err(|_| CorruptStorageError::new())?
		{
			let mut election_access = ElectoralAccess::election_mut(election_identifier);

			if let Some(consensus) = election_access.check_consensus()?.has_consensus() {
				let mut known_accounts = election_access.properties()?;
				election_access.delete();
				known_accounts.witnessed_open_accounts.extend(consensus.new_accounts.iter().map(
					|(account, maybe_swap_details)| {
						let mut sol_or_not = false;
						if let Some(swap_details) = maybe_swap_details.as_ref() {
							Hook::initiate_vault_swap(swap_details.clone());
							sol_or_not = SwapDetails::sol_or_not(swap_details);
						}
						(account.clone(), sol_or_not)
					},
				));
				consensus.confirm_closed_accounts.into_iter().for_each(|acc| {
					known_accounts.closure_initiated_accounts.remove(&acc);
				});
				election_access =
					ElectoralAccess::new_election(Default::default(), known_accounts, ())?;
			}

			let no_of_available_nonces = Hook::get_number_of_available_sol_nonce_accounts();
			let mut known_accounts = election_access.properties()?;
			// We need to have at least two nonces available since we need to have one nonce
			// reserved for solana rotation
			if no_of_available_nonces > NONCE_AVAILABILITY_THRESHOLD_FOR_INITIATING_FETCH &&
				!known_accounts.witnessed_open_accounts.is_empty()
			{
				known_accounts.witnessed_open_accounts.sort_by_key(|a| Reverse(a.1));
				// Since closing accounts is a low priority action, we wait for certain number
				// of sol nonces to be free for us to initiate account closures which
				// indicates that there is not enough Chainflip activity on the sol side
				// and so we can process account closures.
				//
				// we also wait for certain number of accounts to buffer up or allow a certain
				// amount of time to pass before initiating account closures.
				if known_accounts.witnessed_open_accounts[0].1 ||
					(no_of_available_nonces >
						NONCE_AVAILABILITY_THRESHOLD_FOR_INITIATING_SWAP_ACCOUNT_CLOSURES &&
						(known_accounts.witnessed_open_accounts.len() >=
							MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES ||
							(*current_block_number)
								// current block number is always greater than when apicall was
								// last created
								.saturating_sub(ElectoralAccess::unsynchronised_state()?)
								.into() >= MAX_WAIT_BLOCKS_FOR_SWAP_ACCOUNT_CLOSURE_APICALLS))
				{
					let (accounts_to_close, _): (Vec<_>, Vec<_>) = known_accounts
						.witnessed_open_accounts
						.drain(
							..sp_std::cmp::min(
								known_accounts.witnessed_open_accounts.len(),
								MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES,
							),
						)
						.unzip();

					match Hook::maybe_fetch_and_close_accounts(accounts_to_close.clone()) {
						Ok(()) => {
							known_accounts.closure_initiated_accounts.extend(accounts_to_close);
							election_access.refresh(
								election_access
									.election_identifier()
									.extra()
									.checked_add(1)
									.ok_or_else(CorruptStorageError::new)?,
								known_accounts,
							)?;
							ElectoralAccess::set_unsynchronised_state(*current_block_number)?;
						},
						Err(e) => {
							log::error!("Failed to initiate account closure: {:?}", e);
						},
					}
				}
			}
		} else {
			ElectoralAccess::new_election(
				Default::default(),
				SolanaVaultSwapsKnownAccounts {
					witnessed_open_accounts: Vec::new(),
					closure_initiated_accounts: BTreeSet::new(),
				},
				(),
			)?;
		}

		Ok(())
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		consensus_votes: ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		let num_authorities = consensus_votes.num_authorities();
		let success_threshold = success_threshold_from_share_count(num_authorities);
		let active_votes = consensus_votes.active_votes();
		let num_active_votes = active_votes.len() as u32;
		Ok(if num_active_votes >= success_threshold {
			let mut counts_votes = BTreeMap::new();
			let mut counts_new_accounts = BTreeMap::new();
			let mut counts_confirm_closed_accounts = BTreeMap::new();

			for vote in active_votes {
				counts_votes.entry(vote).and_modify(|count| *count += 1).or_insert(1);
			}

			counts_votes.iter().for_each(|(vote, count)| {
				count_votes(&vote.new_accounts, &mut counts_new_accounts, count);
				count_votes(
					&vote.confirm_closed_accounts,
					&mut counts_confirm_closed_accounts,
					count,
				);
			});

			counts_new_accounts.retain(|_, count| *count >= success_threshold);
			let new_accounts = counts_new_accounts.into_keys().collect::<BTreeSet<_>>();
			counts_confirm_closed_accounts.retain(|_, count| *count >= success_threshold);
			let confirm_closed_accounts =
				counts_confirm_closed_accounts.into_keys().collect::<BTreeSet<_>>();

			if new_accounts.is_empty() && confirm_closed_accounts.is_empty() {
				None
			} else {
				Some(SolanaVaultSwapsVote { new_accounts, confirm_closed_accounts })
			}
		} else {
			None
		})
	}
}

fn count_votes<T: Ord + Clone>(
	accounts: &BTreeSet<T>,
	counts_accounts: &mut BTreeMap<T, u32>,
	count: &u32,
) {
	accounts.iter().for_each(|account| {
		counts_accounts
			.entry((*account).clone())
			.and_modify(|c| *c += *count)
			.or_insert(*count);
	});
}
