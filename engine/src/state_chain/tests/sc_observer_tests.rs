use std::sync::Arc;

use cf_chains::ChainId;
use cf_traits::{ChainflipAccountData, ChainflipAccountState};
use frame_system::{AccountInfo, Phase};
use mockall::predicate::{self, eq};
use pallet_cf_vaults::{BlockHeightWindow, Vault};
use sp_core::{storage::StorageKey, H256};
use sp_runtime::{AccountId32, Digest};
use state_chain_runtime::Header;

use crate::{
    eth::{EthBroadcaster, EthRpcClient, MockEthRpcApi},
    logging::{self, test_utils::new_test_logger},
    multisig::{MultisigInstruction, MultisigOutcome},
    settings::test_utils::new_test_settings,
    state_chain::{
        client::{test_utils::storage_change_set_from, MockStateChainRpcApi, StateChainClient},
        sc_observer::start,
    },
};

fn test_header(number: u32) -> Header {
    Header {
        number,
        parent_hash: H256::default(),
        state_root: H256::default(),
        extrinsics_root: H256::default(),
        digest: Digest { logs: Vec::new() },
    }
}

#[tokio::test]
async fn sends_initial_extrinsics_and_starts_witnessing_when_active_on_startup() {
    let logger = new_test_logger();

    let eth_rpc_mock = MockEthRpcApi::new();

    let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

    let (multisig_instruction_sender, _multisig_instruction_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigInstruction>();
    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let (_multisig_outcome_sender, multisig_outcome_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigOutcome>();

    let (sm_window_sender, mut sm_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();
    let (km_window_sender, mut km_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();

    // Submits only one extrinsic when no events, the heartbeat
    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    mock_state_chain_rpc_client
        .expect_submit_extrinsic_rpc()
        .times(1)
        .returning(move |_| Ok(H256::default()));

    let latest_block_hash = H256::default();

    // get account info
    let our_account_id = AccountId32::new([0u8; 32]);

    let account_info_storage_key = StorageKey(
        frame_system::Account::<state_chain_runtime::Runtime>::hashed_key_for(&our_account_id),
    );

    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(latest_block_hash)), eq(account_info_storage_key))
        .times(1)
        .returning(move |_, _| {
            Ok(vec![storage_change_set_from(
                AccountInfo {
                    nonce: 0,
                    consumers: 0,
                    providers: 0,
                    sufficients: 0,
                    data: ChainflipAccountData {
                        state: ChainflipAccountState::Validator,
                        last_active_epoch: Some(3),
                    },
                },
                latest_block_hash,
            )])
        });

    // get the epoch
    let epoch_key = StorageKey(
        pallet_cf_validator::CurrentEpoch::<state_chain_runtime::Runtime>::hashed_key().into(),
    );
    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(latest_block_hash)), eq(epoch_key))
        .times(1)
        .returning(move |_, _| Ok(vec![storage_change_set_from(3, latest_block_hash)]));

    // get the current vault
    let vault_key = StorageKey(
        pallet_cf_vaults::Vaults::<state_chain_runtime::Runtime>::hashed_key_for(
            &3,
            &ChainId::Ethereum,
        ),
    );

    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(latest_block_hash)), eq(vault_key))
        .times(1)
        .returning(move |_, _| {
            Ok(vec![storage_change_set_from(
                Vault {
                    public_key: vec![0; 33],
                    active_window: BlockHeightWindow { from: 30, to: None },
                },
                latest_block_hash,
            )])
        });

    let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
        mock_state_chain_rpc_client,
        our_account_id,
        StorageKey(vec![0, 32]),
    ));

    // No blocks in the stream
    let sc_block_stream = tokio_stream::iter(vec![]);

    start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_instruction_sender,
        account_peer_mapping_change_sender,
        multisig_outcome_receiver,
        sm_window_sender,
        km_window_sender,
        latest_block_hash,
        &logger,
    )
    .await;

    // ensure we kicked off the witness processes
    assert_eq!(
        km_window_receiver.recv().await.unwrap(),
        BlockHeightWindow { from: 30, to: None }
    );
    assert_eq!(
        sm_window_receiver.recv().await.unwrap(),
        BlockHeightWindow { from: 30, to: None }
    );
}

#[tokio::test]
async fn sends_initial_extrinsics_and_starts_witnessing_when_outgoing_on_startup() {
    // Current epoch is set to 3. Our last_active_epoch is set to 2.
    // So we should be deemed outgoing, and submit the block height windows as expected to the nodes
    // even though we are passive
    let logger = new_test_logger();

    let eth_rpc_mock = MockEthRpcApi::new();

    let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

    let (multisig_instruction_sender, _multisig_instruction_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigInstruction>();
    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let (_multisig_outcome_sender, multisig_outcome_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigOutcome>();

    let (sm_window_sender, mut sm_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();
    let (km_window_sender, mut km_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();

    // Submits only one extrinsic when no events, the heartbeat
    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    mock_state_chain_rpc_client
        .expect_submit_extrinsic_rpc()
        .times(1)
        .returning(move |_| Ok(H256::default()));

    let latest_block_hash = H256::default();

    // get account info
    let our_account_id = AccountId32::new([0u8; 32]);

    let account_info_storage_key = StorageKey(
        frame_system::Account::<state_chain_runtime::Runtime>::hashed_key_for(&our_account_id),
    );

    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(latest_block_hash)), eq(account_info_storage_key))
        .times(1)
        .returning(move |_, _| {
            Ok(vec![storage_change_set_from(
                AccountInfo {
                    nonce: 0,
                    consumers: 0,
                    providers: 0,
                    sufficients: 0,
                    data: ChainflipAccountData {
                        // NB: We are Passive and last active is one less than current epoch (3)
                        state: ChainflipAccountState::Passive,
                        last_active_epoch: Some(2),
                    },
                },
                latest_block_hash,
            )])
        });

    // get the current epoch, which is 3
    let epoch_key = StorageKey(
        pallet_cf_validator::CurrentEpoch::<state_chain_runtime::Runtime>::hashed_key().into(),
    );
    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(latest_block_hash)), eq(epoch_key))
        .times(1)
        .returning(move |_, _| Ok(vec![storage_change_set_from(3, latest_block_hash)]));

    // get the current vault
    let vault_key = StorageKey(
        pallet_cf_vaults::Vaults::<state_chain_runtime::Runtime>::hashed_key_for(
            &2,
            &ChainId::Ethereum,
        ),
    );

    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(latest_block_hash)), eq(vault_key))
        .times(1)
        .returning(move |_, _| {
            Ok(vec![storage_change_set_from(
                Vault {
                    public_key: vec![0; 33],
                    active_window: BlockHeightWindow {
                        from: 20,
                        to: Some(29),
                    },
                },
                latest_block_hash,
            )])
        });

    let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
        mock_state_chain_rpc_client,
        our_account_id,
        StorageKey(vec![0, 32]),
    ));

    // No blocks in the stream
    let sc_block_stream = tokio_stream::iter(vec![]);

    start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_instruction_sender,
        account_peer_mapping_change_sender,
        multisig_outcome_receiver,
        sm_window_sender,
        km_window_sender,
        latest_block_hash,
        &logger,
    )
    .await;

    // ensure we kicked off the witness processes
    assert_eq!(
        km_window_receiver.recv().await.unwrap(),
        BlockHeightWindow {
            from: 20,
            to: Some(29)
        }
    );
    assert_eq!(
        sm_window_receiver.recv().await.unwrap(),
        BlockHeightWindow {
            from: 20,
            to: Some(29)
        }
    );
}

#[tokio::test]
async fn sends_initial_extrinsics_when_backup_but_not_outgoing_on_startup() {
    // Current epoch is set to 3. Our last_active_epoch is set to 1.
    // So we should be backup, but not outgoing. Hence, we should not send any messages
    // down the witness channels
    let logger = new_test_logger();

    let eth_rpc_mock = MockEthRpcApi::new();

    let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

    let (multisig_instruction_sender, _multisig_instruction_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigInstruction>();
    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let (_multisig_outcome_sender, multisig_outcome_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigOutcome>();

    let (sm_window_sender, mut sm_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();
    let (km_window_sender, mut km_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();

    // Submits only one extrinsic when no events, the heartbeat
    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    mock_state_chain_rpc_client
        .expect_submit_extrinsic_rpc()
        .times(1)
        .returning(move |_| Ok(H256::default()));

    let latest_block_hash = H256::default();

    // get account info
    let our_account_id = AccountId32::new([0u8; 32]);

    let account_info_storage_key = StorageKey(
        frame_system::Account::<state_chain_runtime::Runtime>::hashed_key_for(&our_account_id),
    );

    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(latest_block_hash)), eq(account_info_storage_key))
        .times(1)
        .returning(move |_, _| {
            Ok(vec![storage_change_set_from(
                AccountInfo {
                    nonce: 0,
                    consumers: 0,
                    providers: 0,
                    sufficients: 0,
                    data: ChainflipAccountData {
                        // NB: We are Passive and last active is *two* less than current epoch (3)
                        state: ChainflipAccountState::Backup,
                        last_active_epoch: Some(1),
                    },
                },
                latest_block_hash,
            )])
        });

    // get the current epoch, which is 3
    let epoch_key = StorageKey(
        pallet_cf_validator::CurrentEpoch::<state_chain_runtime::Runtime>::hashed_key().into(),
    );
    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(latest_block_hash)), eq(epoch_key))
        .times(1)
        .returning(move |_, _| Ok(vec![storage_change_set_from(3, latest_block_hash)]));

    let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
        mock_state_chain_rpc_client,
        our_account_id,
        StorageKey(vec![0; 32]),
    ));

    // No blocks in the stream
    let sc_block_stream = tokio_stream::iter(vec![]);

    start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_instruction_sender,
        account_peer_mapping_change_sender,
        multisig_outcome_receiver,
        sm_window_sender,
        km_window_sender,
        latest_block_hash,
        &logger,
    )
    .await;

    // ensure we did NOT kick off the witness processes - as we are *only* backup, not outgoing
    assert!(km_window_receiver.recv().await.is_none());
    assert!(sm_window_receiver.recv().await.is_none());
}

#[tokio::test]
async fn backup_checks_account_data_every_block() {
    let logger = new_test_logger();

    let eth_rpc_mock = MockEthRpcApi::new();

    let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

    let (multisig_instruction_sender, _multisig_instruction_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigInstruction>();
    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let (_multisig_outcome_sender, multisig_outcome_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigOutcome>();

    let (sm_window_sender, mut sm_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();
    let (km_window_sender, mut km_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();

    // Submits only one extrinsic when no events, the heartbeat
    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    mock_state_chain_rpc_client
        .expect_submit_extrinsic_rpc()
        .times(2)
        .returning(move |_| Ok(H256::default()));

    let latest_block_hash = H256::default();

    // get account info
    let our_account_id = AccountId32::new([0u8; 32]);

    let account_info_storage_key = StorageKey(
        frame_system::Account::<state_chain_runtime::Runtime>::hashed_key_for(&our_account_id),
    );

    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(predicate::always(), eq(account_info_storage_key))
        // NB: This is called three times. Once at the start, and then once for every block (x2 in this test)
        .times(3)
        .returning(move |_, _| {
            Ok(vec![storage_change_set_from(
                AccountInfo {
                    nonce: 0,
                    consumers: 0,
                    providers: 0,
                    sufficients: 0,
                    data: ChainflipAccountData {
                        // NB: We are Passive and last active is *two* less than current epoch (3)
                        state: ChainflipAccountState::Backup,
                        last_active_epoch: Some(1),
                    },
                },
                latest_block_hash,
            )])
        });

    // get the current epoch, which is 3
    let epoch_key = StorageKey(
        pallet_cf_validator::CurrentEpoch::<state_chain_runtime::Runtime>::hashed_key().into(),
    );
    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(predicate::always(), eq(epoch_key))
        .times(3)
        .returning(move |_, _| Ok(vec![storage_change_set_from(3, H256::default())]));

    // Get events from the block
    // use this fake events key, to save creating chain metadata in the tests
    let events_key = StorageKey(vec![2; 32]);
    // We will match on every block hash, but only the events key, as we want to return no events
    // on every block
    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(predicate::always(), eq(events_key.clone()))
        .times(2)
        .returning(|_, _| Ok(vec![]));

    let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
        mock_state_chain_rpc_client,
        our_account_id,
        events_key,
    ));

    // two empty blocks in the stream (empty because all queries for the events of a block will
    // return no events, see above)
    let sc_block_stream = tokio_stream::iter(vec![Ok(test_header(20)), Ok(test_header(21))]);

    start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_instruction_sender,
        account_peer_mapping_change_sender,
        multisig_outcome_receiver,
        sm_window_sender,
        km_window_sender,
        latest_block_hash,
        &logger,
    )
    .await;

    // ensure we did NOT kick off the witness processes at any point - as we are *only* backup, not outgoing
    assert!(km_window_receiver.recv().await.is_none());
    assert!(sm_window_receiver.recv().await.is_none());
}

#[tokio::test]
async fn validator_checks_account_data_on_new_epoch_event() {
    let logger = new_test_logger();

    let eth_rpc_mock = MockEthRpcApi::new();

    let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

    // === FAKE BLOCKHEADERS ===
    // two empty blocks in the stream
    let empty_block_header = test_header(20);
    let new_epoch_block_header = test_header(21);

    let sc_block_stream = tokio_stream::iter(vec![
        Ok(empty_block_header.clone()),
        // in the mock for the events, we return a new epoch event for the block with this header
        Ok(new_epoch_block_header.clone()),
    ]);

    let (multisig_instruction_sender, _multisig_instruction_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigInstruction>();
    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let (_multisig_outcome_sender, multisig_outcome_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigOutcome>();

    let (sm_window_sender, mut sm_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();
    let (km_window_sender, mut km_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();

    // Submits only one extrinsic when no events, the heartbeat
    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    mock_state_chain_rpc_client
        .expect_submit_extrinsic_rpc()
        .times(2)
        .returning(move |_| Ok(H256::default()));

    let latest_block_hash = H256::default();

    // get account info
    let our_account_id = AccountId32::new([0u8; 32]);

    let account_info_storage_key = StorageKey(
        frame_system::Account::<state_chain_runtime::Runtime>::hashed_key_for(&our_account_id),
    );

    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(
            eq(Some(latest_block_hash)),
            eq(account_info_storage_key.clone()),
        )
        .times(1)
        .returning(move |_, _| {
            Ok(vec![storage_change_set_from(
                AccountInfo {
                    nonce: 0,
                    consumers: 0,
                    providers: 0,
                    sufficients: 0,
                    data: ChainflipAccountData {
                        state: ChainflipAccountState::Validator,
                        last_active_epoch: Some(3),
                    },
                },
                latest_block_hash,
            )])
        });

    // The second time we query for our account data is when we've received a new epoch event
    let new_epoch_block_header_hash = new_epoch_block_header.hash();
    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(
            eq(Some(new_epoch_block_header.hash())),
            eq(account_info_storage_key),
        )
        .times(1)
        .returning(move |_, _| {
            Ok(vec![storage_change_set_from(
                AccountInfo {
                    nonce: 0,
                    consumers: 0,
                    providers: 0,
                    sufficients: 0,
                    data: ChainflipAccountData {
                        state: ChainflipAccountState::Validator,
                        last_active_epoch: Some(4),
                    },
                },
                new_epoch_block_header_hash,
            )])
        });

    // get the current epoch, which is 3
    let epoch_key = StorageKey(
        pallet_cf_validator::CurrentEpoch::<state_chain_runtime::Runtime>::hashed_key().into(),
    );
    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(latest_block_hash)), eq(epoch_key.clone()))
        .times(1)
        .returning(move |_, _| Ok(vec![storage_change_set_from(3, H256::default())]));

    // the second time we get the current epoch is on a new epoch event
    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(new_epoch_block_header.hash())), eq(epoch_key))
        .times(1)
        .returning(move |_, _| Ok(vec![storage_change_set_from(4, H256::default())]));

    // get the current vault
    let vault_key = StorageKey(
        pallet_cf_vaults::Vaults::<state_chain_runtime::Runtime>::hashed_key_for(
            &3,
            &ChainId::Ethereum,
        ),
    );

    // get the vault on start up because we're active
    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(latest_block_hash)), eq(vault_key.clone()))
        .times(1)
        .returning(move |_, _| {
            Ok(vec![storage_change_set_from(
                Vault {
                    public_key: vec![0; 33],
                    active_window: BlockHeightWindow { from: 30, to: None },
                },
                latest_block_hash,
            )])
        });

    let vault_key_after_new_epoch = StorageKey(pallet_cf_vaults::Vaults::<
        state_chain_runtime::Runtime,
    >::hashed_key_for(&4, &ChainId::Ethereum));

    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(
            eq(Some(new_epoch_block_header.hash())),
            eq(vault_key_after_new_epoch),
        )
        .times(1)
        .returning(move |_, _| {
            Ok(vec![storage_change_set_from(
                Vault {
                    public_key: vec![1; 33],
                    active_window: BlockHeightWindow { from: 40, to: None },
                },
                new_epoch_block_header_hash,
            )])
        });

    // Get events from the block
    // use this fake events key, to save creating chain metadata in the tests
    let events_key = StorageKey(vec![2; 32]);
    // We will match on every block hash, but only the events key, as we want to return no events
    // on every block
    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(empty_block_header.hash())), eq(events_key.clone()))
        .times(1)
        .returning(|_, _| Ok(vec![]));

    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(
            eq(Some(new_epoch_block_header.clone().hash())),
            eq(events_key.clone()),
        )
        .times(1)
        .returning(move |_, _| {
            Ok(vec![storage_change_set_from(
                vec![(
                    // TODO: Check that this phase is correct for a new epoch event
                    Phase::ApplyExtrinsic(0),
                    state_chain_runtime::Event::Validator(pallet_cf_validator::Event::NewEpoch(4)),
                    vec![H256::default()],
                )],
                new_epoch_block_header.hash(),
            )])
        });

    let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
        mock_state_chain_rpc_client,
        our_account_id,
        events_key,
    ));

    start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_instruction_sender,
        account_peer_mapping_change_sender,
        multisig_outcome_receiver,
        sm_window_sender,
        km_window_sender,
        latest_block_hash,
        &logger,
    )
    .await;

    // ensure we did kick off the witness processes at the start
    assert_eq!(
        km_window_receiver.recv().await.unwrap(),
        BlockHeightWindow { from: 30, to: None }
    );
    assert_eq!(
        sm_window_receiver.recv().await.unwrap(),
        BlockHeightWindow { from: 30, to: None }
    );

    // after a new epoch, we should have sent new messages down the channels
    assert_eq!(
        km_window_receiver.recv().await.unwrap(),
        BlockHeightWindow { from: 40, to: None }
    );
    assert_eq!(
        sm_window_receiver.recv().await.unwrap(),
        BlockHeightWindow { from: 40, to: None }
    );
}

#[tokio::test]
#[ignore = "runs forever, useful for testing without having to start the whole CFE"]
async fn run_the_sc_observer() {
    let settings = new_test_settings().unwrap();
    let logger = logging::test_utils::new_test_logger();

    let (latest_block_hash, block_stream, state_chain_client) =
        crate::state_chain::client::connect_to_state_chain(&settings.state_chain, false, &logger)
            .await
            .unwrap();

    let (multisig_instruction_sender, _multisig_instruction_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigInstruction>();
    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();
    let (_multisig_outcome_sender, multisig_outcome_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigOutcome>();

    let eth_rpc_client = EthRpcClient::new(&settings.eth, &logger).await.unwrap();
    let eth_broadcaster =
        EthBroadcaster::new(&settings.eth, eth_rpc_client.clone(), &logger).unwrap();

    let (sm_window_sender, _sm_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();
    let (km_window_sender, _km_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();

    start(
        state_chain_client,
        block_stream,
        eth_broadcaster,
        multisig_instruction_sender,
        account_peer_mapping_change_sender,
        multisig_outcome_receiver,
        sm_window_sender,
        km_window_sender,
        latest_block_hash,
        &logger,
    )
    .await;
}
