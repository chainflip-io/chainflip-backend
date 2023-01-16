//! Contains tests related to Accounts in the runtime

use crate::{network, GENESIS_EPOCH};
use cf_traits::EpochInfo;
use pallet_cf_reputation::Reputations;
use pallet_cf_staking::{ClaimAmount, MinimumStake};
use pallet_cf_validator::{AccountPeerMapping, MappedPeers, VanityNames};
use state_chain_runtime::{Reputation, Runtime, Validator};

#[test]
fn account_deletion_removes_relevant_storage_items() {
	super::genesis::default().build().execute_with(|| {
		let genesis_nodes = Validator::current_authorities();

		// Create a single backup node which we will use to test deletion
		let (mut testnet, backup_nodes) = network::Network::create(1_u8, &genesis_nodes);

		let backup_node = backup_nodes.first().unwrap().clone();

		let min_stake = MinimumStake::<Runtime>::get();

		testnet
			.stake_manager_contract
			.stake(backup_node.clone(), min_stake, GENESIS_EPOCH);
		testnet.move_forward_blocks(1);

		network::Cli::register_as_validator(&backup_node);

		network::setup_peer_mapping(&backup_node);
		let (peer_id, _, _) = AccountPeerMapping::<Runtime>::get(&backup_node).unwrap();
		assert!(MappedPeers::<Runtime>::contains_key(peer_id));

		network::Cli::activate_account(&backup_node);
		Reputation::heartbeat(state_chain_runtime::RuntimeOrigin::signed(backup_node.clone()))
			.unwrap();
		assert!(Reputations::<Runtime>::get(backup_node.clone()).online_credits > 0);

		let elon_vanity_name = "ElonShibMoonInu";
		network::Cli::set_vanity_name(&backup_node, elon_vanity_name);
		let vanity_names = VanityNames::<Runtime>::get();
		assert_eq!(*vanity_names.get(&backup_node).unwrap(), elon_vanity_name.as_bytes().to_vec());

		network::Cli::claim(&backup_node, ClaimAmount::Exact(min_stake), [0x22; 20]);

		// Sign the claim request
		testnet.move_forward_blocks(1);

		testnet
			.stake_manager_contract
			.execute_claim(backup_node.clone(), min_stake, GENESIS_EPOCH);

		// Let witnesses be registered, completing the claiming process. This should trigger an
		// account deletion.
		testnet.move_forward_blocks(1);

		assert_eq!(pallet_cf_flip::Account::<Runtime>::get(&backup_node), Default::default());
		assert!(AccountPeerMapping::<Runtime>::get(&backup_node).is_none());
		assert!(!MappedPeers::<Runtime>::contains_key(peer_id));
		let vanity_names = VanityNames::<Runtime>::get();
		assert!(vanity_names.get(&backup_node).is_none());
		assert_eq!(pallet_cf_account_roles::AccountRoles::<Runtime>::get(&backup_node), None);
		assert_eq!(Reputations::<Runtime>::get(backup_node).online_credits, 0);
	});
}
