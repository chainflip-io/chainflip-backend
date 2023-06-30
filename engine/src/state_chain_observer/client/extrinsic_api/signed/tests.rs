use mockall::predicate::eq;
use std::sync::Arc;

use super::{check_account_role_and_wait, signer::PairSigner};
use crate::state_chain_observer::client::{base_rpc_api::MockBaseRpcApi, StreamCache};
use cf_primitives::AccountRole;
use lazy_static::lazy_static;
use sp_core::{
	storage::{StorageData, StorageKey},
	Encode, H256,
};
use sp_runtime::Digest;
use state_chain_runtime::Header;
use utilities::MakeCachedStream;

lazy_static! {
	// Just some dummy call to test with
	pub static ref DUMMY_CALL: state_chain_runtime::RuntimeCall = state_chain_runtime::RuntimeCall::Witnesser(pallet_cf_witnesser::Call::witness_at_epoch {
		call: Box::new(state_chain_runtime::RuntimeCall::PolkadotChainTracking(
			pallet_cf_chain_tracking::Call::update_chain_state {
				new_chain_state: pallet_cf_chain_tracking::ChainState {
					block_height: 0,
					tracked_data: cf_chains::dot::PolkadotTrackedData { median_tip: 0 },
				},
			},
		)),
		epoch_index: 0,
	});
}

pub fn test_header(number: u32) -> Header {
	Header {
		number,
		parent_hash: H256::default(),
		state_root: H256::default(),
		extrinsics_root: H256::default(),
		digest: Digest { logs: Vec::new() },
	}
}

/// Testing the wait_for_required_role feature of the check_account_role_and_wait function.
/// When a client is created, it can wait for the account to have the given role before
/// proceeding. Checking again each new block.
#[tokio::test]
async fn should_wait_for_account_role() {
	let mut mock_rpc_api = MockBaseRpcApi::new();
	let initial_block_hash = H256::default();
	let signer = PairSigner::new(sp_core::Pair::generate().0);
	let test_header = test_header(1);
	const REQUIRED_ROLE: AccountRole = AccountRole::Validator;

	// At first we send no account role so the client should wait for the next block
	mock_rpc_api
		.expect_storage()
		.with(
			eq(initial_block_hash),
			eq(StorageKey(pallet_cf_account_roles::AccountRoles::<
				state_chain_runtime::Runtime,
			>::hashed_key_for(&signer.account_id))),
		)
		.once()
		.return_once(move |_, _| Ok(None));

	// We expect the client to request again, so return a role this time
	mock_rpc_api
		.expect_storage()
		.with(
			eq(test_header.hash()),
			eq(StorageKey(pallet_cf_account_roles::AccountRoles::<
				state_chain_runtime::Runtime,
			>::hashed_key_for(&signer.account_id))),
		)
		.once()
		.return_once(move |_, _| Ok(Some(StorageData(REQUIRED_ROLE.encode()))));

	// Setup an empty block stream
	const BLOCK_CAPACITY: usize = 10;
	let (block_sender, block_receiver) = async_broadcast::broadcast::<(
		state_chain_runtime::Hash,
		state_chain_runtime::Header,
	)>(BLOCK_CAPACITY);
	let mut sc_block_stream = block_receiver.make_cached(
		StreamCache { block_hash: Default::default(), block_number: Default::default() },
		|(block_hash, block_header): &(state_chain_runtime::Hash, state_chain_runtime::Header)| {
			StreamCache { block_hash: *block_hash, block_number: block_header.number }
		},
	);

	// Send a block so that the client will check for an updated account role
	block_sender.broadcast((test_header.hash(), test_header)).await.unwrap();

	check_account_role_and_wait(
		Arc::new(mock_rpc_api),
		&signer,
		REQUIRED_ROLE,
		// wait_for_required_role is enabled
		true,
		&mut sc_block_stream,
	)
	.await
	.unwrap();
}

#[tokio::test]
async fn should_error_if_no_account_role() {
	let mut mock_rpc_api = MockBaseRpcApi::new();
	let initial_block_hash = H256::default();
	let signer = PairSigner::new(sp_core::Pair::generate().0);
	const REQUIRED_ROLE: AccountRole = AccountRole::Validator;

	// Return no account role
	mock_rpc_api
		.expect_storage()
		.with(
			eq(initial_block_hash),
			eq(StorageKey(pallet_cf_account_roles::AccountRoles::<
				state_chain_runtime::Runtime,
			>::hashed_key_for(&signer.account_id))),
		)
		.once()
		.return_once(move |_, _| Ok(None));

	// Setup an empty block stream
	const BLOCK_CAPACITY: usize = 10;
	let (_block_sender, block_receiver) = async_broadcast::broadcast::<(
		state_chain_runtime::Hash,
		state_chain_runtime::Header,
	)>(BLOCK_CAPACITY);
	let mut sc_block_stream = block_receiver.make_cached(
		StreamCache { block_hash: Default::default(), block_number: Default::default() },
		|(block_hash, block_header): &(state_chain_runtime::Hash, state_chain_runtime::Header)| {
			StreamCache { block_hash: *block_hash, block_number: block_header.number }
		},
	);

	check_account_role_and_wait(
		Arc::new(mock_rpc_api),
		&signer,
		REQUIRED_ROLE,
		// wait_for_required_role is disabled
		false,
		&mut sc_block_stream,
	)
	.await
	.unwrap_err();
}

#[tokio::test]
async fn should_error_if_incorrect_account_role() {
	let mut mock_rpc_api = MockBaseRpcApi::new();
	let initial_block_hash = H256::default();
	let signer = PairSigner::new(sp_core::Pair::generate().0);
	const REQUIRED_ROLE: AccountRole = AccountRole::Validator;
	const INCORRECT_ROLE: AccountRole = AccountRole::LiquidityProvider;

	// Return the incorrect account role
	mock_rpc_api
		.expect_storage()
		.with(
			eq(initial_block_hash),
			eq(StorageKey(pallet_cf_account_roles::AccountRoles::<
				state_chain_runtime::Runtime,
			>::hashed_key_for(&signer.account_id))),
		)
		.once()
		.return_once(move |_, _| Ok(Some(StorageData(INCORRECT_ROLE.encode()))));

	// Setup an empty block stream
	const BLOCK_CAPACITY: usize = 10;
	let (_block_sender, block_receiver) = async_broadcast::broadcast::<(
		state_chain_runtime::Hash,
		state_chain_runtime::Header,
	)>(BLOCK_CAPACITY);
	let mut sc_block_stream = block_receiver.make_cached(
		StreamCache { block_hash: Default::default(), block_number: Default::default() },
		|(block_hash, block_header): &(state_chain_runtime::Hash, state_chain_runtime::Header)| {
			StreamCache { block_hash: *block_hash, block_number: block_header.number }
		},
	);

	check_account_role_and_wait(
		Arc::new(mock_rpc_api),
		&signer,
		REQUIRED_ROLE,
		// wait_for_required_role is disabled
		false,
		&mut sc_block_stream,
	)
	.await
	.unwrap_err();
}
