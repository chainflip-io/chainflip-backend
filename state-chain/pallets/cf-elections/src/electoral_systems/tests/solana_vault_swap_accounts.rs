use cf_chains::sol::MAX_WAIT_BLOCKS_FOR_SWAP_ACCOUNT_CLOSURE_APICALLS;
use sp_std::collections::btree_set::BTreeSet;

use super::{mocks::*, register_checks};
use crate::{
	electoral_system::{ConsensusVote, ConsensusVotes},
	electoral_systems::solana_vault_swap_accounts::{
		SolanaVaultSwapAccounts, SolanaVaultSwapAccountsHook, SolanaVaultSwapsVote,
	},
};

pub type Account = u64;
pub type SwapDetails = ();
pub type BlockNumber = u32;
pub type ValidatorId = ();

thread_local! {
	pub static CLOSE_ACCOUNTS_CALLED: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
	pub static INITIATE_VAULT_SWAP_CALLED: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
	pub static GET_NUMBER_OF_SOL_NONCES_CALLED: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

struct MockHook;

impl SolanaVaultSwapAccountsHook<Account, SwapDetails, ()> for MockHook {
	fn close_accounts(_accounts: Vec<Account>) -> Result<(), ()> {
		CLOSE_ACCOUNTS_CALLED.with(|hook_called| hook_called.set(hook_called.get() + 1));
		Ok(())
	}

	fn initiate_vault_swap(_swap_details: SwapDetails) {
		INITIATE_VAULT_SWAP_CALLED.with(|hook_called| hook_called.set(hook_called.get() + 1));
	}

	fn get_number_of_available_sol_nonce_accounts() -> usize {
		GET_NUMBER_OF_SOL_NONCES_CALLED.with(|hook_called| hook_called.set(hook_called.get() + 1));
		10
	}
}

impl MockHook {
	pub fn close_accounts_called() -> u8 {
		CLOSE_ACCOUNTS_CALLED.with(|hook_called| hook_called.get())
	}
	pub fn init_swap_called() -> u8 {
		INITIATE_VAULT_SWAP_CALLED.with(|hook_called| hook_called.get())
	}
	pub fn get_number_of_available_sol_nonce_accounts_called() -> u8 {
		GET_NUMBER_OF_SOL_NONCES_CALLED.with(|hook_called| hook_called.get())
	}
}

type MinimalVaultSwapAccounts =
	SolanaVaultSwapAccounts<Account, SwapDetails, BlockNumber, (), MockHook, ValidatorId, ()>;

register_checks! {
	MinimalVaultSwapAccounts {
		only_one_election(_pre, post) {
			assert_eq!(post.election_identifiers.len(), 1, "Only one election should exist.");
		},
		initiate_vault_swap_hook_not_called(_pre, _post) {
			assert_eq!(INITIATE_VAULT_SWAP_CALLED.with(|hook_called| hook_called.get()), 0, "Hook should have been called once so far!");
		},
		initiate_vault_swap_hook_called_twice(_pre, _post) {
			assert_eq!(INITIATE_VAULT_SWAP_CALLED.with(|hook_called| hook_called.get()), 2, "Hook should have been called once so far!");
		},
		initiate_vault_swap_hook_called_four_times(_pre,_post) {
			assert_eq!(INITIATE_VAULT_SWAP_CALLED.with(|hook_called| hook_called.get()), 4, "Hook should have been called once so far!");
		},
		initiate_vault_swap_hook_called_15_times(_pre, _post) {
			assert_eq!(INITIATE_VAULT_SWAP_CALLED.with(|hook_called| hook_called.get()), 15, "Hook should have been called once so far!");
		},
		close_accounts_hook_not_called(_pre, _post) {
			assert_eq!(
				CLOSE_ACCOUNTS_CALLED.with(|hook_called| hook_called.get()),
				0,
				"Hook should not have been called!"
			);
		},
		close_accounts_hook_called_once(_pre, _post) {
			assert_eq!(
				CLOSE_ACCOUNTS_CALLED.with(|hook_called| hook_called.get()),
				1,
				"Hook should not have been called!"
			);
		},
		close_accounts_hook_called_twice(_pre, _post) {
			assert_eq!(
				CLOSE_ACCOUNTS_CALLED.with(|hook_called| hook_called.get()),
				2,
				"Hook should not have been called!"
			);
		},
		get_sol_nonces_hook_not_called(_pre, _post) {
			assert_eq!(
				GET_NUMBER_OF_SOL_NONCES_CALLED.with(|hook_called| hook_called.get()),
				0,
				"Hook should not have been called!"
			);
		},
		get_sol_nonces_hook_called_once(_pre, _post) {
			assert_eq!(
				GET_NUMBER_OF_SOL_NONCES_CALLED.with(|hook_called| hook_called.get()),
				1,
				"Hook should not have been called!"
			);
		},
		get_sol_nonces_hook_called_twice(_pre, _post) {
			assert_eq!(
				GET_NUMBER_OF_SOL_NONCES_CALLED.with(|hook_called| hook_called.get()),
				2,
				"Hook should not have been called!"
			);
		},
	}
}

#[test]
fn on_finalize_accounts_limit_reached() {
	TestSetup::default()
		.with_unsynchronised_state(0)
		.build()
		.test_on_finalize(
			&0u32,
			|_| {
				assert_eq!(
					MockHook::close_accounts_called(),
					0,
					"Hook should not have been called!"
				);
				assert_eq!(MockHook::init_swap_called(), 0, "Hook should not have been called!");
				assert_eq!(
					MockHook::get_number_of_available_sol_nonce_accounts_called(),
					0,
					"Hook should not have been called!"
				);
			},
			vec![
				Check::<MinimalVaultSwapAccounts>::only_one_election(),
				Check::<MinimalVaultSwapAccounts>::initiate_vault_swap_hook_not_called(),
				Check::<MinimalVaultSwapAccounts>::close_accounts_hook_not_called(),
				Check::<MinimalVaultSwapAccounts>::get_sol_nonces_hook_not_called(),
			],
		)
		.expect_consensus(
			generate_votes_n_to_m_accounts(0, 15),
			Some(SolanaVaultSwapsVote {
				new_accounts: (0..15u64).map(|i| (i, ())).collect::<BTreeSet<_>>(),
				confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_1, CLOSED_ACCOUNT_2]),
			}),
		)
		.test_on_finalize(
			// check duration has not yet elapsed, so no change
			&1u32,
			|_| {},
			vec![
				Check::<MinimalVaultSwapAccounts>::only_one_election(),
				Check::<MinimalVaultSwapAccounts>::initiate_vault_swap_hook_called_15_times(),
				Check::<MinimalVaultSwapAccounts>::close_accounts_hook_called_once(),
				Check::<MinimalVaultSwapAccounts>::get_sol_nonces_hook_called_once(),
			],
		);
}

#[test]
fn on_finalize_time_limit_reached() {
	TestSetup::default()
		.with_unsynchronised_state(0)
		.build()
		.test_on_finalize(
			&0u32,
			|_| {
				assert_eq!(
					MockHook::close_accounts_called(),
					0,
					"Hook should not have been called!"
				);
				assert_eq!(MockHook::init_swap_called(), 0, "Hook should not have been called!");
				assert_eq!(
					MockHook::get_number_of_available_sol_nonce_accounts_called(),
					0,
					"Hook should not have been called!"
				);
			},
			vec![
				Check::<MinimalVaultSwapAccounts>::only_one_election(),
				Check::<MinimalVaultSwapAccounts>::initiate_vault_swap_hook_not_called(),
				Check::<MinimalVaultSwapAccounts>::close_accounts_hook_not_called(),
				Check::<MinimalVaultSwapAccounts>::get_sol_nonces_hook_not_called(),
			],
		)
		.expect_consensus(
			generate_votes_n_to_m_accounts(0, 2),
			Some(SolanaVaultSwapsVote {
				new_accounts: (0..2u64).map(|i| (i, ())).collect::<BTreeSet<_>>(),
				confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_1, CLOSED_ACCOUNT_2]),
			}),
		)
		.test_on_finalize(
			&0,
			|_| {},
			vec![
				Check::<MinimalVaultSwapAccounts>::only_one_election(),
				Check::<MinimalVaultSwapAccounts>::initiate_vault_swap_hook_called_twice(),
				Check::<MinimalVaultSwapAccounts>::close_accounts_hook_not_called(),
				Check::<MinimalVaultSwapAccounts>::get_sol_nonces_hook_called_once(),
			],
		)
		.expect_consensus(
			generate_votes_n_to_m_accounts(2, 4),
			Some(SolanaVaultSwapsVote {
				new_accounts: (2..4u64).map(|i| (i, ())).collect::<BTreeSet<_>>(),
				confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_1, CLOSED_ACCOUNT_2]),
			}),
		)
		.test_on_finalize(
			// check duration has not yet elapsed, so no change
			&MAX_WAIT_BLOCKS_FOR_SWAP_ACCOUNT_CLOSURE_APICALLS,
			|_| {},
			vec![
				Check::<MinimalVaultSwapAccounts>::only_one_election(),
				Check::<MinimalVaultSwapAccounts>::initiate_vault_swap_hook_called_four_times(),
				Check::<MinimalVaultSwapAccounts>::close_accounts_hook_called_once(),
				Check::<MinimalVaultSwapAccounts>::get_sol_nonces_hook_called_twice(),
			],
		);
}

pub const NEW_ACCOUNT_1: u64 = 1u64;
pub const NEW_ACCOUNT_2: u64 = 2u64;
pub const NEW_ACCOUNT_3: u64 = 3u64;

pub const CLOSED_ACCOUNT_1: u64 = 4u64;
pub const CLOSED_ACCOUNT_2: u64 = 5u64;

#[test]
fn test_consensus() {
	TestSetup::<MinimalVaultSwapAccounts>::default()
		.build_with_initial_election()
		.expect_consensus(
			generate_votes_specific_case([80, 80, 0, 0]),
			Some(SolanaVaultSwapsVote {
				new_accounts: BTreeSet::from([(NEW_ACCOUNT_1, ()), (NEW_ACCOUNT_2, ())]),
				confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_1]),
			}),
		);

	TestSetup::<MinimalVaultSwapAccounts>::default()
		.build_with_initial_election()
		.expect_consensus(
			generate_votes_specific_case([0, 80, 80, 80]),
			Some(SolanaVaultSwapsVote {
				new_accounts: BTreeSet::from([
					(NEW_ACCOUNT_1, ()),
					(NEW_ACCOUNT_2, ()),
					(NEW_ACCOUNT_3, ()),
				]),
				confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_1]),
			}),
		);

	TestSetup::<MinimalVaultSwapAccounts>::default()
		.build_with_initial_election()
		.expect_consensus(
			generate_votes_specific_case([0, 0, 80, 80]),
			Some(SolanaVaultSwapsVote {
				new_accounts: BTreeSet::from([(NEW_ACCOUNT_3, ())]),
				confirm_closed_accounts: BTreeSet::from([]),
			}),
		);
}

fn generate_votes_specific_case(
	no_of_each_vote: [usize; 4],
) -> ConsensusVotes<MinimalVaultSwapAccounts> {
	let vote_1 = SolanaVaultSwapsVote {
		new_accounts: BTreeSet::from([
			(NEW_ACCOUNT_1, ()),
			(NEW_ACCOUNT_2, ()),
			(NEW_ACCOUNT_3, ()),
		]),
		confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_1, CLOSED_ACCOUNT_2]),
	};

	let vote_2 = SolanaVaultSwapsVote {
		new_accounts: BTreeSet::from([(NEW_ACCOUNT_1, ()), (NEW_ACCOUNT_2, ())]),
		confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_1]),
	};

	let vote_3 = SolanaVaultSwapsVote {
		new_accounts: BTreeSet::from([(NEW_ACCOUNT_1, ()), (NEW_ACCOUNT_3, ())]),
		confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_1]),
	};

	let vote_4 = SolanaVaultSwapsVote {
		new_accounts: BTreeSet::from([(NEW_ACCOUNT_2, ()), (NEW_ACCOUNT_3, ())]),
		confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_2]),
	};

	ConsensusVotes {
		votes: (0..no_of_each_vote[0])
			.map(|_| ConsensusVote { vote: Some(((), vote_1.clone())), validator_id: () })
			.chain(
				(0..no_of_each_vote[1])
					.map(|_| ConsensusVote { vote: Some(((), vote_2.clone())), validator_id: () }),
			)
			.chain(
				(0..no_of_each_vote[2])
					.map(|_| ConsensusVote { vote: Some(((), vote_3.clone())), validator_id: () }),
			)
			.chain(
				(0..no_of_each_vote[3])
					.map(|_| ConsensusVote { vote: Some(((), vote_4.clone())), validator_id: () }),
			)
			.collect::<Vec<_>>(),
	}
}

fn generate_votes_n_to_m_accounts(n: u64, m: u64) -> ConsensusVotes<MinimalVaultSwapAccounts> {
	let vote = SolanaVaultSwapsVote {
		new_accounts: (n..m).map(|i| (i, ())).collect::<BTreeSet<_>>(),
		confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_1, CLOSED_ACCOUNT_2]),
	};

	ConsensusVotes {
		votes: (0..150)
			.map(|_| ConsensusVote { vote: Some(((), vote.clone())), validator_id: () })
			.collect::<Vec<_>>(),
	}
}
