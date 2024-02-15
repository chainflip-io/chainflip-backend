//! This file contains tests related to the Witnessing functionalities.

use super::*;
use crate::{genesis::GENESIS_BALANCE, network::fund_authorities_and_join_auction};

use frame_support::Hashable;
use sp_std::collections::btree_set::BTreeSet;

use cf_primitives::AccountRole;
use cf_traits::EpochInfo;
use cf_utilities::success_threshold_from_share_count;
use state_chain_runtime::{chainflip::Offence, constants::common::LATE_WITNESS_GRACE_PERIOD};

use pallet_cf_reputation::Penalty;
use pallet_cf_witnesser::{CallHash, CallHashExecuted, WitnessDeadline};

#[test]
fn can_punish_failed_witnesser() {
	const EPOCH_BLOCKS: u32 = 1000;
	const MAX_AUTHORITIES: AuthorityCount = 50;
	super::genesis::with_test_defaults()
		.blocks_per_epoch(EPOCH_BLOCKS)
		.max_authorities(MAX_AUTHORITIES)
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) = fund_authorities_and_join_auction(MAX_AUTHORITIES);
			testnet.move_to_the_next_epoch();

			// Split current authority into 2 groups. Some to witness and some will be punished for
			// failing to witness.
			let mut to_witness = Validator::current_authorities().into_iter().collect::<Vec<_>>();
			let success_threshold =
				success_threshold_from_share_count(to_witness.len() as u32) as u64;
			let to_punish = to_witness.split_off(success_threshold as usize);

			let epoch = Validator::current_epoch();
			let call: Box<RuntimeCall> =
				Box::new(RuntimeCall::System(frame_system::Call::<Runtime>::remark {
					remark: vec![],
				}));
			let call_hash = CallHash(call.blake2_256());

			// Set the penalty for failing to witness. Use a long suspension period to make it
			// easier to check the penalty.
			assert_ok!(Reputation::set_penalty(
				pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
				Offence::FailedToWitnessInTime,
				Penalty { reputation: -100, suspension: EPOCH_BLOCKS },
			));

			// Before the deadline is set, no one has been reported.
			assert!(
				Reputation::validators_suspended_for(&[Offence::FailedToWitnessInTime]).is_empty()
			);

			// Setup a new set of authorities:
			// Create new engine nodes, load funds and start bidding.
			const INITIAL_FUNDING: FlipBalance = GENESIS_BALANCE * 3;
			let new_authorities = (0..50)
				.map(|_| {
					let node = testnet.create_engine();
					testnet.state_chain_gateway_contract.fund_account(
						node.clone(),
						INITIAL_FUNDING,
						epoch,
					);
					node
				})
				.collect::<BTreeSet<_>>();
			testnet.move_forward_blocks(2);
			for node in &new_authorities {
				network::new_account(node, AccountRole::Validator);
				network::setup_account_and_peer_mapping(node);
				let _ = Funding::start_bidding(RuntimeOrigin::signed(node.clone()));
			}
			// Have current authorities stop bidding, so the next epoch will use the new set of
			// Authorities.
			Validator::current_authorities().into_iter().for_each(|v| {
				assert_ok!(Funding::stop_bidding(RuntimeOrigin::signed(v.clone())));
			});

			// Witness at the end of epoch, so the grace period ends in the next epoch
			testnet.move_to_the_end_of_epoch();
			let target_block = System::block_number() + LATE_WITNESS_GRACE_PERIOD;
			to_witness.into_iter().for_each(|v| {
				assert_ok!(Witnesser::witness_at_epoch(
					RuntimeOrigin::signed(v),
					call.clone(),
					epoch
				));
			});

			assert!(CallHashExecuted::<Runtime>::contains_key(epoch, call_hash));
			assert_eq!(WitnessDeadline::<Runtime>::get(target_block), vec![(epoch, call_hash)]);

			// New epoch uses the new authorities.
			testnet.move_to_the_next_epoch();
			assert_eq!(Validator::current_epoch(), epoch + 1);
			assert_eq!(Validator::current_authorities(), new_authorities);

			// After deadline has passed, the correct set of authority nodes are reported.
			assert_eq!(
				Reputation::validators_suspended_for(&[Offence::FailedToWitnessInTime]),
				BTreeSet::from_iter(to_punish)
			);

			// storage is cleaned up.
			assert_eq!(WitnessDeadline::<Runtime>::decode_len(target_block), None);
		});
}
