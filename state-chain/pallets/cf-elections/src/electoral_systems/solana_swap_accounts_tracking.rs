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
	NONCE_AVAILABILITY_THRESHOLD_FOR_INITIATING_SWAP_ACCOUNT_CLOSURES,
};
use cf_utilities::success_threshold_from_share_count;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	sp_runtime::traits::CheckedSub,
	Parameter,
};
use itertools::Itertools;
use sp_std::vec::Vec;

pub trait SolanaVaultSwapAccountsHook<Account, SwapDetails, E> {
	fn close_accounts(accounts: Vec<Account>) -> Result<(), E>;
	fn initiate_vault_swap(swap_details: SwapDetails);
	fn get_number_of_available_sol_nonce_accounts() -> usize;
}

pub type SolanaVaultSwapAccountsLastClosedAt<BlockNumber> = BlockNumber;

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TypeInfo, Encode, Decode)]
pub struct SolanaVaultSwapsKnownAccounts<Account: Ord> {
	pub witnessed_open_accounts: Vec<Account>,
	pub closure_initiated_accounts: BTreeSet<Account>,
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for SolanaVaultSwapsKnownAccounts<VaultSwapAccountAndSender> {
	fn benchmark_value() -> Self {
		Self {
			witnessed_open_accounts: sp_std::vec![BenchmarkValue::benchmark_value()],
			closure_initiated_accounts: BTreeSet::from([BenchmarkValue::benchmark_value()]),
		}
	}
}

#[derive(
	Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TypeInfo, Encode, Decode, Ord, PartialOrd,
)]
pub struct SolanaVaultSwapsVote<Account: Ord, SwapDetails: Ord> {
	pub new_accounts: BTreeSet<(Account, SwapDetails)>,
	pub confirm_closed_accounts: BTreeSet<Account>,
}

#[cfg(feature = "runtime-benchmarks")]
impl<Account: Ord + BenchmarkValue, SwapDetails: Ord + BenchmarkValue> BenchmarkValue
	for SolanaVaultSwapsVote<Account, SwapDetails>
{
	fn benchmark_value() -> Self {
		Self {
			new_accounts: BTreeSet::from([
				BenchmarkValue::benchmark_value(),
				BenchmarkValue::benchmark_value(),
			]),
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
		SwapDetails: MaybeSerializeDeserialize + Member + Parameter + Ord,
		BlockNumber: MaybeSerializeDeserialize + Member + Parameter + Ord + CheckedSub + Into<u32> + Copy,
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
	type ElectionIdentifierExtra = ();
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
		_election_identifier_with_extra: crate::electoral_system::ElectionIdentifierOf<Self>,
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
		electoral_access: &mut ElectoralAccess,
		election_identifiers: Vec<ElectionIdentifier<Self::ElectionIdentifierExtra>>,
		current_block_number: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		if let Some(election_identifier) = election_identifiers
			.into_iter()
			.at_most_one()
			.map_err(|_| CorruptStorageError::new())?
		{
			let mut election_access = electoral_access.election_mut(election_identifier)?;
			if let Some(consensus) = election_access.check_consensus()?.has_consensus() {
				let mut known_accounts = election_access.properties()?;
				election_access.delete();
				known_accounts.witnessed_open_accounts.extend(consensus.new_accounts.iter().map(
					|(account, swap_details)| {
						Hook::initiate_vault_swap((*swap_details).clone());
						(*account).clone()
					},
				));
				consensus.confirm_closed_accounts.into_iter().for_each(|acc| {
					known_accounts.closure_initiated_accounts.remove(&acc);
				});

				if Hook::get_number_of_available_sol_nonce_accounts() >
					NONCE_AVAILABILITY_THRESHOLD_FOR_INITIATING_SWAP_ACCOUNT_CLOSURES &&
					(known_accounts.witnessed_open_accounts.len() >=
						MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES ||
						(*current_block_number)
							.checked_sub(&electoral_access.unsynchronised_state()?)
							.expect("current block number is always greater than when apicall was last created")
							.into() >= MAX_WAIT_BLOCKS_FOR_SWAP_ACCOUNT_CLOSURE_APICALLS)
				{
					let accounts_to_close: Vec<_> = if known_accounts.witnessed_open_accounts.len() >
						MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES
					{
						known_accounts
							.witnessed_open_accounts
							.drain(..MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES)
							.collect()
					} else {
						sp_std::mem::take(&mut known_accounts.witnessed_open_accounts)
					};
					match Hook::close_accounts(accounts_to_close.clone()) {
						Ok(()) => {
							known_accounts.closure_initiated_accounts.extend(accounts_to_close);
							electoral_access.set_unsynchronised_state(*current_block_number)?;
						},
						Err(e) => {
							log::error!(
								"failed to build Solana CloseSolanaVaultSwapAccounts apicall: {:?}",
								e
							);
							known_accounts.witnessed_open_accounts.extend(accounts_to_close);
						},
					}
				}
				electoral_access.new_election((), known_accounts, ())?;
			}
		} else {
			electoral_access.new_election(
				(),
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
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
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
				vote.new_accounts.iter().for_each(|new_account| {
					counts_new_accounts
						.entry(new_account)
						.and_modify(|c| *c += *count)
						.or_insert(*count);
				});
				vote.confirm_closed_accounts.iter().for_each(|confirm_closed_account| {
					counts_confirm_closed_accounts
						.entry(confirm_closed_account)
						.and_modify(|c| *c += *count)
						.or_insert(*count);
				});
			});

			counts_new_accounts.retain(|_, count| *count >= success_threshold);
			let new_accounts = counts_new_accounts.into_keys().cloned().collect::<BTreeSet<_>>();
			counts_confirm_closed_accounts.retain(|_, count| *count >= success_threshold);
			let confirm_closed_accounts =
				counts_confirm_closed_accounts.into_keys().cloned().collect::<BTreeSet<_>>();

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
