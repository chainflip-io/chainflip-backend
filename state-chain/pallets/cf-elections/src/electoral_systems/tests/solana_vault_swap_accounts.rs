use cf_chains::sol::{
	MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES,
	MAX_WAIT_BLOCKS_FOR_SWAP_ACCOUNT_CLOSURE_APICALLS,
	NONCE_AVAILABILITY_THRESHOLD_FOR_INITIATING_SWAP_ACCOUNT_CLOSURES,
};
use sp_std::collections::btree_set::BTreeSet;

use super::{mocks::*, register_checks};
use crate::{
	electoral_system::{ConsensusStatus, ConsensusVote, ConsensusVotes},
	electoral_systems::solana_vault_swap_accounts::{
		SolanaVaultSwapAccounts, SolanaVaultSwapAccountsHook, SolanaVaultSwapsKnownAccounts,
		SolanaVaultSwapsVote,
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
	pub static FAIL_CLOSE_ACCOUNTS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
	pub static NO_OF_SOL_NONCES: std::cell::Cell<usize> = const { std::cell::Cell::new(10) };
}

struct MockHook;

impl SolanaVaultSwapAccountsHook<Account, SwapDetails, ()> for MockHook {
	fn close_accounts(_accounts: Vec<Account>) -> Result<(), ()> {
		CLOSE_ACCOUNTS_CALLED.with(|hook_called| hook_called.set(hook_called.get() + 1));
		if FAIL_CLOSE_ACCOUNTS.with(|hook_called| hook_called.get()) {
			Err(())
		} else {
			Ok(())
		}
	}

	fn initiate_vault_swap(_swap_details: SwapDetails) {
		INITIATE_VAULT_SWAP_CALLED.with(|hook_called| hook_called.set(hook_called.get() + 1));
	}

	fn get_number_of_available_sol_nonce_accounts() -> usize {
		GET_NUMBER_OF_SOL_NONCES_CALLED.with(|hook_called| hook_called.set(hook_called.get() + 1));
		NO_OF_SOL_NONCES.with(|hook_called| hook_called.get())
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
			assert_eq!(INITIATE_VAULT_SWAP_CALLED.with(|hook_called| hook_called.get()), 2, "Hook not called expected number of times");
		},
		initiate_vault_swap_hook_called_four_times(_pre,_post) {
			assert_eq!(INITIATE_VAULT_SWAP_CALLED.with(|hook_called| hook_called.get()), 4, "Hook not called expected number of times");
		},
		initiate_vault_swap_hook_called_15_times(_pre, _post) {
			assert_eq!(INITIATE_VAULT_SWAP_CALLED.with(|hook_called| hook_called.get()), 15, "Hook not called expected number of times");
		},
		close_accounts_hook_not_called(_pre, _post) {
			assert_eq!(CLOSE_ACCOUNTS_CALLED.with(|hook_called| hook_called.get()),	0, "Hook should not have been called!");
		},
		close_accounts_hook_called_once(_pre, _post) {
			assert_eq!(CLOSE_ACCOUNTS_CALLED.with(|hook_called| hook_called.get()),	1, "Hook not called expected number of times");
		},
		close_accounts_hook_called_twice(_pre, _post) {
			assert_eq!(CLOSE_ACCOUNTS_CALLED.with(|hook_called| hook_called.get()),	2, "Hook not called expected number of times");
		},
		get_sol_nonces_hook_not_called(_pre, _post) {
			assert_eq!(GET_NUMBER_OF_SOL_NONCES_CALLED.with(|hook_called| hook_called.get()), 0, "Hook should not have been called!");
		},
		get_sol_nonces_hook_called_once(_pre, _post) {
			assert_eq!(GET_NUMBER_OF_SOL_NONCES_CALLED.with(|hook_called| hook_called.get()), 1, "Hook not called expected number of times");
		},
		get_sol_nonces_hook_called_twice(_pre, _post) {
			assert_eq!(GET_NUMBER_OF_SOL_NONCES_CALLED.with(|hook_called| hook_called.get()), 2, "Hook not called expected number of times");
		},
	}
}

pub const TEST_NUMBER_OF_ACCOUNTS: u64 = 15;

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
		.force_consensus_update(ConsensusStatus::Gained {
			new: generate_votes_for_account_range(0..TEST_NUMBER_OF_ACCOUNTS),
			most_recent: None,
		})
		// account closure will be initiated since account limit is reached, even though time limit
		// has not reached yet.
		.test_on_finalize(
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
		.force_consensus_update(ConsensusStatus::Gained {
			new: generate_votes_for_account_range(0..2),
			most_recent: None,
		})
		// account closure will not initiate since we havent reached time or account limit
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
		.force_consensus_update(ConsensusStatus::Gained {
			new: generate_votes_for_account_range(2..4),
			most_recent: None,
		})
		// time limit reached. account closure initiated even though account number limit not
		// reached
		.test_on_finalize(
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

#[test]
fn on_finalize_close_accounts_error() {
	let max_batch_size: u64 = MAX_BATCH_SIZE_OF_VAULT_SWAP_ACCOUNT_CLOSURES.try_into().unwrap();
	FAIL_CLOSE_ACCOUNTS.with(|hook_called| hook_called.set(true));
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
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: generate_votes_for_account_range(0..TEST_NUMBER_OF_ACCOUNTS),
		})
		.test_on_finalize(
			&1u32,
			|_| {},
			vec![
				Check::<MinimalVaultSwapAccounts>::only_one_election(),
				Check::<MinimalVaultSwapAccounts>::initiate_vault_swap_hook_called_15_times(),
				Check::<MinimalVaultSwapAccounts>::close_accounts_hook_called_once(),
				Check::<MinimalVaultSwapAccounts>::get_sol_nonces_hook_called_once(),
			],
		)
		.expect_election_properties_only_election(SolanaVaultSwapsKnownAccounts {
			// if close_accounts errors, the accounts are pushed back into open accounts at the end
			// of the vector.
			witnessed_open_accounts: (max_batch_size..TEST_NUMBER_OF_ACCOUNTS)
				.chain(0u64..max_batch_size)
				.collect::<Vec<_>>(),
			closure_initiated_accounts: BTreeSet::new(),
		});
}

#[test]
fn on_finalize_nonces_below_threshold() {
	NO_OF_SOL_NONCES.with(|hook_called| {
		hook_called.set(NONCE_AVAILABILITY_THRESHOLD_FOR_INITIATING_SWAP_ACCOUNT_CLOSURES - 1)
	});
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
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: generate_votes_for_account_range(0..TEST_NUMBER_OF_ACCOUNTS),
		})
		.test_on_finalize(
			&1u32,
			|_| {},
			vec![
				Check::<MinimalVaultSwapAccounts>::only_one_election(),
				Check::<MinimalVaultSwapAccounts>::initiate_vault_swap_hook_called_15_times(),
				Check::<MinimalVaultSwapAccounts>::close_accounts_hook_not_called(),
				Check::<MinimalVaultSwapAccounts>::get_sol_nonces_hook_called_once(),
			],
		)
		.expect_election_properties_only_election(SolanaVaultSwapsKnownAccounts {
			witnessed_open_accounts: (0..TEST_NUMBER_OF_ACCOUNTS).collect::<Vec<_>>(),
			closure_initiated_accounts: BTreeSet::new(),
		});
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
				new_accounts: BTreeSet::from([
					(NEW_ACCOUNT_1, Some(())),
					(NEW_ACCOUNT_2, Some(())),
				]),
				confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_1]),
			}),
		);

	TestSetup::<MinimalVaultSwapAccounts>::default()
		.build_with_initial_election()
		.expect_consensus(
			generate_votes_specific_case([0, 80, 80, 80]),
			Some(SolanaVaultSwapsVote {
				new_accounts: BTreeSet::from([
					(NEW_ACCOUNT_1, Some(())),
					(NEW_ACCOUNT_2, Some(())),
					(NEW_ACCOUNT_3, Some(())),
				]),
				confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_1]),
			}),
		);

	TestSetup::<MinimalVaultSwapAccounts>::default()
		.build_with_initial_election()
		.expect_consensus(
			generate_votes_specific_case([0, 0, 80, 80]),
			Some(SolanaVaultSwapsVote {
				new_accounts: BTreeSet::from([(NEW_ACCOUNT_3, Some(()))]),
				confirm_closed_accounts: BTreeSet::from([]),
			}),
		);

	TestSetup::<MinimalVaultSwapAccounts>::default()
		.build_with_initial_election()
		.expect_consensus(ConsensusVotes { votes: vec![] }, None);

	TestSetup::<MinimalVaultSwapAccounts>::default()
		.build_with_initial_election()
		.expect_consensus(generate_vote_no_consensus(), None);
}

fn generate_vote_no_consensus() -> ConsensusVotes<MinimalVaultSwapAccounts> {
	let vote_1 = SolanaVaultSwapsVote {
		new_accounts: BTreeSet::from([(1, Some(())), (2, Some(()))]),
		confirm_closed_accounts: BTreeSet::new(),
	};

	let vote_2 = SolanaVaultSwapsVote {
		new_accounts: BTreeSet::from([(3, Some(())), (4, Some(()))]),
		confirm_closed_accounts: BTreeSet::new(),
	};

	ConsensusVotes {
		votes: (0..80)
			.map(|_| ConsensusVote { vote: Some(((), vote_1.clone())), validator_id: () })
			.chain(
				(0..80)
					.map(|_| ConsensusVote { vote: Some(((), vote_2.clone())), validator_id: () }),
			)
			.collect::<Vec<_>>(),
	}
}

fn generate_votes_specific_case(
	no_of_each_vote: [usize; 4],
) -> ConsensusVotes<MinimalVaultSwapAccounts> {
	let votes = [
		SolanaVaultSwapsVote {
			new_accounts: BTreeSet::from([
				(NEW_ACCOUNT_1, Some(())),
				(NEW_ACCOUNT_2, Some(())),
				(NEW_ACCOUNT_3, Some(())),
			]),
			confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_1, CLOSED_ACCOUNT_2]),
		},
		SolanaVaultSwapsVote {
			new_accounts: BTreeSet::from([(NEW_ACCOUNT_1, Some(())), (NEW_ACCOUNT_2, Some(()))]),
			confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_1]),
		},
		SolanaVaultSwapsVote {
			new_accounts: BTreeSet::from([(NEW_ACCOUNT_1, Some(())), (NEW_ACCOUNT_3, Some(()))]),
			confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_1]),
		},
		SolanaVaultSwapsVote {
			new_accounts: BTreeSet::from([(NEW_ACCOUNT_2, Some(())), (NEW_ACCOUNT_3, Some(()))]),
			confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_2]),
		},
	];
	ConsensusVotes {
		votes: no_of_each_vote
			.iter()
			.enumerate()
			.flat_map(|(i, &count)| {
				let vote = votes[i].clone();
				std::iter::repeat_with(move || ConsensusVote {
					vote: Some(((), vote.clone())),
					validator_id: (),
				})
				.take(count)
			})
			.collect::<Vec<_>>(),
	}
}

fn generate_votes_for_account_range(
	r: std::ops::Range<u64>,
) -> SolanaVaultSwapsVote<Account, SwapDetails> {
	SolanaVaultSwapsVote {
		new_accounts: r.map(|i| (i, Some(()))).collect::<BTreeSet<_>>(),
		confirm_closed_accounts: BTreeSet::from([CLOSED_ACCOUNT_1, CLOSED_ACCOUNT_2]),
	}
}
