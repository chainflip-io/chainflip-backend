use std::sync::Arc;

use cf_chains::{
    eth::{AggKey, UnsignedTransaction},
    Ethereum,
};
use cf_traits::{BackupOrPassive, ChainflipAccountData, ChainflipAccountState};
use codec::Encode;
use frame_system::{AccountInfo, Phase};
use mockall::predicate::{self, eq};
use pallet_cf_broadcast::BroadcastAttemptId;
use pallet_cf_validator::HistoricalActiveEpochs;
use pallet_cf_vaults::{BlockHeightWindow, Vault, Vaults};
use sp_core::{
    storage::{StorageData, StorageKey},
    H256, U256,
};
use sp_runtime::{AccountId32, Digest};
use state_chain_runtime::{EthereumInstance, Header, Runtime};
use web3::types::{Bytes, SignedTransaction};

use crate::{
    eth::{EthBroadcaster, EthWsRpcClient, MockEthRpcApi},
    logging::{self, test_utils::new_test_logger},
    multisig::client::MockMultisigClientApi,
    settings::test_utils::new_test_settings,
    state_chain::{
        client::{
            mock_account_storage_key, mock_events_key, test_utils::storage_change_set_from,
            MockStateChainRpcApi, StateChainClient, OUR_ACCOUNT_ID_BYTES,
        },
        sc_observer,
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

fn account_info_from_data(state: ChainflipAccountState) -> AccountInfo<u32, ChainflipAccountData> {
    AccountInfo {
        nonce: 0,
        consumers: 0,
        providers: 0,
        sufficients: 0,
        data: ChainflipAccountData { state },
    }
}

fn mock_historical_epochs_key() -> StorageKey {
    StorageKey(HistoricalActiveEpochs::<Runtime>::hashed_key_for(
        AccountId32::new(OUR_ACCOUNT_ID_BYTES),
    ))
}

/// ETH Window for epoch three after epoch starts, so we know the end
const WINDOW_EPOCH_TWO_END: BlockHeightWindow = BlockHeightWindow {
    from: 20,
    to: Some(29),
};

/// ETH Window for epoch three initially. No end known
const WINDOW_EPOCH_THREE_INITIAL: BlockHeightWindow = BlockHeightWindow { from: 30, to: None };

/// ETH Window for epoch three after epoch starts, so we know the end
const WINDOW_EPOCH_THREE_END: BlockHeightWindow = BlockHeightWindow {
    from: 30,
    to: Some(39),
};

/// ETH Window for epoch three initially. No end known
const WINDOW_EPOCH_FOUR_INITIAL: BlockHeightWindow = BlockHeightWindow { from: 40, to: None };

#[tokio::test]
async fn sends_initial_extrinsics_and_starts_witnessing_when_current_authority_on_startup() {
    // Submits only one extrinsic when no events, the heartbeat
    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    mock_state_chain_rpc_client
        .expect_submit_extrinsic_rpc()
        .times(1)
        .returning(move |_| Ok(H256::default()));

    let initial_block_hash = H256::default();

    mock_state_chain_rpc_client
        .expect_storage()
        .with(eq(initial_block_hash), eq(mock_account_storage_key()))
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                account_info_from_data(ChainflipAccountState::CurrentAuthority).encode(),
            )))
        });

    // mock the call to historical_active_epochs
    mock_state_chain_rpc_client
        .expect_storage()
        .with(eq(initial_block_hash), eq(mock_historical_epochs_key()))
        .times(1)
        .returning(move |_, _| Ok(Some(StorageData(vec![3].encode()))));

    // get the current vault
    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(initial_block_hash),
            eq(StorageKey(
                Vaults::<Runtime, EthereumInstance>::hashed_key_for(&3),
            )),
        )
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                Vault::<Ethereum> {
                    public_key: AggKey::from_pubkey_compressed([0; 33]),
                    active_window: WINDOW_EPOCH_THREE_INITIAL,
                }
                .encode(),
            )))
        });

    let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
        mock_state_chain_rpc_client,
    ));

    let multisig_client = Arc::new(MockMultisigClientApi::new());

    // No blocks in the stream
    let sc_block_stream = tokio_stream::iter(vec![]);

    let logger = new_test_logger();

    let eth_rpc_mock = MockEthRpcApi::new();

    let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let (sm_window_sender, mut sm_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();
    let (km_window_sender, mut km_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();
    sc_observer::start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        sm_window_sender,
        km_window_sender,
        initial_block_hash,
        &logger,
    )
    .await;

    // ensure we kicked off the witness processes
    assert_eq!(
        km_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_THREE_INITIAL
    );
    assert_eq!(
        sm_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_THREE_INITIAL
    );
}

#[tokio::test]
async fn sends_initial_extrinsics_and_starts_witnessing_when_historic_on_startup() {
    // Current epoch is set to 3. Our last_active_epoch is set toƒ} 2.
    // So we should be deemed outgoing, and submit the block height windows as expected to the nodes
    // even though we are passive

    // Submits only one extrinsic when no events, the heartbeat
    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    mock_state_chain_rpc_client
        .expect_submit_extrinsic_rpc()
        .times(1)
        .returning(move |_| Ok(H256::default()));

    let initial_block_hash = H256::default();

    // get account info
    mock_state_chain_rpc_client
        .expect_storage()
        .with(eq(initial_block_hash), eq(mock_account_storage_key()))
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                account_info_from_data(ChainflipAccountState::HistoricalAuthority(
                    BackupOrPassive::Passive,
                ))
                .encode(),
            )))
        });

    // mock the call to historical_active_epochs
    let historical_epoch = 2;
    mock_state_chain_rpc_client
        .expect_storage()
        .with(eq(initial_block_hash), eq(mock_historical_epochs_key()))
        .times(1)
        .returning(move |_, _| Ok(Some(StorageData(vec![historical_epoch].encode()))));

    // get the current vault
    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(initial_block_hash),
            eq(StorageKey(
                Vaults::<Runtime, EthereumInstance>::hashed_key_for(&historical_epoch),
            )),
        )
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                Vault::<Ethereum> {
                    public_key: AggKey::from_pubkey_compressed([0; 33]),
                    active_window: WINDOW_EPOCH_TWO_END,
                }
                .encode(),
            )))
        });

    let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
        mock_state_chain_rpc_client,
    ));

    // No blocks in the stream
    let sc_block_stream = tokio_stream::iter(vec![]);

    let logger = new_test_logger();

    let eth_rpc_mock = MockEthRpcApi::new();

    let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

    let multisig_client = Arc::new(MockMultisigClientApi::new());

    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let (sm_window_sender, mut sm_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();
    let (km_window_sender, mut km_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();

    sc_observer::start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        sm_window_sender,
        km_window_sender,
        initial_block_hash,
        &logger,
    )
    .await;

    // ensure we kicked off the witness processes
    assert_eq!(
        km_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_TWO_END
    );
    assert_eq!(
        sm_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_TWO_END
    );

    assert!(km_window_receiver.recv().await.is_none());
    assert!(sm_window_receiver.recv().await.is_none());
}

#[tokio::test]
async fn sends_initial_extrinsics_when_backup_but_not_historic_on_startup() {
    // Current epoch is set to 3. Our last_active_epoch is set to 1.
    // So we should be backup, but not outgoing. Hence, we should not send any messages
    // down the witness channels

    // Submits only one extrinsic when no events, the heartbeat
    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    mock_state_chain_rpc_client
        .expect_submit_extrinsic_rpc()
        .times(1)
        .returning(move |_| Ok(H256::default()));

    let initial_block_hash = H256::default();

    // get account info
    mock_state_chain_rpc_client
        .expect_storage()
        .with(eq(initial_block_hash), eq(mock_account_storage_key()))
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                account_info_from_data(ChainflipAccountState::BackupOrPassive(
                    BackupOrPassive::Backup,
                ))
                .encode(),
            )))
        });

    let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
        mock_state_chain_rpc_client,
    ));

    // No blocks in the stream
    let sc_block_stream = tokio_stream::iter(vec![]);

    let logger = new_test_logger();

    let eth_rpc_mock = MockEthRpcApi::new();

    let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

    let multisig_client = Arc::new(MockMultisigClientApi::new());

    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let (sm_window_sender, mut sm_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();
    let (km_window_sender, mut km_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();

    sc_observer::start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        sm_window_sender,
        km_window_sender,
        initial_block_hash,
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

    let multisig_client = Arc::new(MockMultisigClientApi::new());

    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();

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

    let initial_block_hash = H256::default();

    // get account info
    mock_state_chain_rpc_client
        .expect_storage()
        .with(predicate::always(), eq(mock_account_storage_key()))
        // NB: This is called three times. Once at the start, and then once for every block (x2 in this test)
        .times(3)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                account_info_from_data(ChainflipAccountState::BackupOrPassive(
                    BackupOrPassive::Backup,
                ))
                .encode(),
            )))
        });

    // Get events from the block
    // We will match on every block hash, but only the events key, as we want to return no events
    // on every block
    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(predicate::always(), eq(mock_events_key()))
        .times(2)
        .returning(|_, _| Ok(vec![]));

    let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
        mock_state_chain_rpc_client,
    ));

    // two empty blocks in the stream (empty because all queries for the events of a block will
    // return no events, see above)
    let sc_block_stream = tokio_stream::iter(vec![Ok(test_header(20)), Ok(test_header(21))]);

    sc_observer::start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        sm_window_sender,
        km_window_sender,
        initial_block_hash,
        &logger,
    )
    .await;

    // ensure we did NOT kick off the witness processes at any point - as we are *only* backup, not outgoing
    assert!(km_window_receiver.recv().await.is_none());
    assert!(sm_window_receiver.recv().await.is_none());
}

#[tokio::test]
async fn current_authority_to_current_authority_on_new_epoch_event() {
    let logger = new_test_logger();

    let eth_rpc_mock = MockEthRpcApi::new();

    let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

    let multisig_client = Arc::new(MockMultisigClientApi::new());

    // === FAKE BLOCKHEADERS ===
    // two empty blocks in the stream
    let empty_block_header = test_header(20);
    let new_epoch_block_header = test_header(21);

    let sc_block_stream = tokio_stream::iter(vec![
        Ok(empty_block_header.clone()),
        // in the mock for the events, we return a new epoch event for the block with this header
        Ok(new_epoch_block_header.clone()),
    ]);

    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();

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

    let initial_block_hash = H256::default();

    mock_state_chain_rpc_client
        .expect_storage()
        .with(eq(initial_block_hash), eq(mock_account_storage_key()))
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                account_info_from_data(ChainflipAccountState::CurrentAuthority).encode(),
            )))
        });

    // The second time we query for our account data is when we've received a new epoch event
    let new_epoch_block_header_hash = new_epoch_block_header.hash();
    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(new_epoch_block_header_hash),
            eq(mock_account_storage_key()),
        )
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                account_info_from_data(ChainflipAccountState::CurrentAuthority).encode(),
            )))
        });

    // mock the call to historical_active_epochs
    mock_state_chain_rpc_client
        .expect_storage()
        .with(eq(initial_block_hash), eq(mock_historical_epochs_key()))
        .times(1)
        .returning(move |_, _| Ok(Some(StorageData(vec![3].encode()))));

    // the second time we get the current epoch is on a new epoch event
    // we now have 2 epochs in the history, we only get the last one
    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(new_epoch_block_header_hash),
            eq(mock_historical_epochs_key()),
        )
        .times(1)
        .returning(move |_, _| Ok(Some(StorageData(vec![3, 4].encode()))));

    // get the current vault
    let vault_key = StorageKey(Vaults::<Runtime, EthereumInstance>::hashed_key_for(&3));

    // get the vault on start up because we're active
    mock_state_chain_rpc_client
        .expect_storage()
        .with(eq(initial_block_hash), eq(vault_key.clone()))
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                Vault::<Ethereum> {
                    public_key: AggKey::from_pubkey_compressed([0; 33]),
                    active_window: WINDOW_EPOCH_THREE_INITIAL,
                }
                .encode(),
            )))
        });

    let vault_key_after_new_epoch =
        StorageKey(Vaults::<Runtime, EthereumInstance>::hashed_key_for(&4));

    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(new_epoch_block_header_hash),
            eq(vault_key_after_new_epoch),
        )
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                Vault::<Ethereum> {
                    public_key: AggKey::from_pubkey_compressed([0; 33]),
                    active_window: WINDOW_EPOCH_FOUR_INITIAL,
                }
                .encode(),
            )))
        });

    // Get events from the block
    // We will match on every block hash, but only the events key, as we want to return no events
    // on every block
    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(empty_block_header.hash())), eq(mock_events_key()))
        .times(1)
        .returning(|_, _| Ok(vec![]));

    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(new_epoch_block_header_hash)), eq(mock_events_key()))
        .times(1)
        .returning(move |_, _| {
            Ok(vec![storage_change_set_from(
                vec![(
                    Phase::ApplyExtrinsic(0),
                    state_chain_runtime::Event::Validator(pallet_cf_validator::Event::NewEpoch(4)),
                    vec![H256::default()],
                )],
                new_epoch_block_header_hash,
            )])
        });

    let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
        mock_state_chain_rpc_client,
    ));

    sc_observer::start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        sm_window_sender,
        km_window_sender,
        initial_block_hash,
        &logger,
    )
    .await;

    // ensure we did kick off the witness processes at the start
    assert_eq!(
        km_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_THREE_INITIAL
    );
    assert_eq!(
        sm_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_THREE_INITIAL
    );

    // after a new epoch, we should have sent new messages down the channels
    assert_eq!(
        km_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_FOUR_INITIAL
    );
    assert_eq!(
        sm_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_FOUR_INITIAL
    );

    assert!(km_window_receiver.recv().await.is_none());
    assert!(sm_window_receiver.recv().await.is_none());
}

#[tokio::test]
async fn backup_not_historical_to_authority_on_new_epoch() {
    let logger = new_test_logger();

    let eth_rpc_mock = MockEthRpcApi::new();

    let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

    let multisig_client = Arc::new(MockMultisigClientApi::new());

    // === FAKE BLOCKHEADERS ===
    // two empty blocks in the stream
    let empty_block_header = test_header(20);
    let new_epoch_block_header = test_header(21);

    let sc_block_stream = tokio_stream::iter(vec![
        Ok(empty_block_header.clone()),
        // in the mock for the events, we return a new epoch event for the block with this header
        Ok(new_epoch_block_header.clone()),
    ]);

    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();

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

    let initial_block_hash = H256::default();

    // get account info

    // We start as a backup node and fetch on start up, and then the empty block
    mock_state_chain_rpc_client
        .expect_storage()
        .with(predicate::always(), eq(mock_account_storage_key()))
        .times(2)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                account_info_from_data(ChainflipAccountState::BackupOrPassive(
                    BackupOrPassive::Backup,
                ))
                .encode(),
            )))
        });

    // The second time we query for our account data is when we've received a new epoch event
    let new_epoch_block_header_hash = new_epoch_block_header.hash();
    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(new_epoch_block_header_hash),
            eq(mock_account_storage_key()),
        )
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                account_info_from_data(ChainflipAccountState::CurrentAuthority).encode(),
            )))
        });

    // mock the call to historical_active_epochs after we rotate into the new epoch
    let new_epoch = 4;
    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(new_epoch_block_header_hash),
            eq(mock_historical_epochs_key()),
        )
        .times(1)
        .returning(move |_, _| Ok(Some(StorageData(vec![new_epoch].encode()))));

    // We'll get the vault from the new epoch `new_epoch` when we become active
    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(new_epoch_block_header_hash),
            eq(StorageKey(
                Vaults::<Runtime, EthereumInstance>::hashed_key_for(&new_epoch),
            )),
        )
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                Vault::<Ethereum> {
                    public_key: AggKey::from_pubkey_compressed([0; 33]),
                    active_window: WINDOW_EPOCH_FOUR_INITIAL,
                }
                .encode(),
            )))
        });

    // Get events from the block
    // We will match on every block hash, but only the events key, as we want to return no events
    // on every block
    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(empty_block_header.hash())), eq(mock_events_key()))
        .times(1)
        .returning(|_, _| Ok(vec![]));

    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(new_epoch_block_header_hash)), eq(mock_events_key()))
        .times(1)
        .returning(move |_, _| {
            Ok(vec![storage_change_set_from(
                vec![(
                    Phase::ApplyExtrinsic(0),
                    state_chain_runtime::Event::Validator(pallet_cf_validator::Event::NewEpoch(4)),
                    vec![H256::default()],
                )],
                new_epoch_block_header_hash,
            )])
        });

    let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
        mock_state_chain_rpc_client,
    ));

    sc_observer::start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        sm_window_sender,
        km_window_sender,
        initial_block_hash,
        &logger,
    )
    .await;

    // after a new epoch, we should have sent new messages down the channels
    assert_eq!(
        km_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_FOUR_INITIAL
    );
    assert_eq!(
        sm_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_FOUR_INITIAL
    );

    assert!(km_window_receiver.recv().await.is_none());
    assert!(sm_window_receiver.recv().await.is_none());
}

#[tokio::test]
async fn current_authority_to_historical_passive_on_new_epoch_event() {
    // === FAKE BLOCKHEADERS ===
    let empty_block_header = test_header(20);
    let new_epoch_block_header = test_header(21);

    let sc_block_stream = tokio_stream::iter(vec![
        Ok(empty_block_header.clone()),
        // in the mock for the events, we return a new epoch event for the block with this header
        Ok(new_epoch_block_header.clone()),
        // after we go to passive, we should keep checking for our status as a node now
        Ok(test_header(22)),
        Ok(test_header(23)),
    ]);

    // Submits only one extrinsic when no events, the heartbeat
    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    mock_state_chain_rpc_client
        .expect_submit_extrinsic_rpc()
        .times(2)
        .returning(move |_| Ok(H256::default()));

    let initial_block_hash = H256::default();

    // get account info
    mock_state_chain_rpc_client
        .expect_storage()
        .with(eq(initial_block_hash), eq(mock_account_storage_key()))
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                account_info_from_data(ChainflipAccountState::CurrentAuthority).encode(),
            )))
        });

    // The second time we query for our account data is when we've received a new epoch event
    let new_epoch_block_header_hash = new_epoch_block_header.hash();
    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(new_epoch_block_header_hash),
            eq(mock_account_storage_key()),
        )
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                account_info_from_data(ChainflipAccountState::HistoricalAuthority(
                    BackupOrPassive::Passive,
                ))
                .encode(),
            )))
        });

    // after we become passive, we have two blocks of checking our status
    mock_state_chain_rpc_client
        .expect_storage()
        .with(predicate::always(), eq(mock_account_storage_key()))
        .times(2)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                account_info_from_data(ChainflipAccountState::HistoricalAuthority(
                    BackupOrPassive::Passive,
                ))
                .encode(),
            )))
        });

    // we get the historical_active_epochs on startup because we're a current authority
    // we get the only epoch we've been in, 3
    let first_epoch = 3;
    mock_state_chain_rpc_client
        .expect_storage()
        .with(eq(initial_block_hash), eq(mock_historical_epochs_key()))
        .times(1)
        .returning(move |_, _| Ok(Some(StorageData(vec![first_epoch].encode()))));

    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(new_epoch_block_header_hash),
            eq(mock_historical_epochs_key()),
        )
        .times(1)
        .returning(move |_, _| Ok(Some(StorageData(vec![first_epoch].encode()))));

    // get the current vault
    let vault_key = StorageKey(Vaults::<Runtime, EthereumInstance>::hashed_key_for(
        &first_epoch,
    ));

    // get the vault on start up because we're active
    mock_state_chain_rpc_client
        .expect_storage()
        .with(eq(initial_block_hash), eq(vault_key.clone()))
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                Vault::<Ethereum> {
                    public_key: AggKey::from_pubkey_compressed([0; 33]),
                    active_window: WINDOW_EPOCH_THREE_INITIAL,
                }
                .encode(),
            )))
        });

    // NB: Because we're outgoing, we use the same vault key, now we have a close to the window
    mock_state_chain_rpc_client
        .expect_storage()
        .with(eq(new_epoch_block_header_hash), eq(vault_key))
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                Vault::<Ethereum> {
                    public_key: AggKey::from_pubkey_compressed([0; 33]),
                    active_window: WINDOW_EPOCH_THREE_END,
                }
                .encode(),
            )))
        });

    // Get events from the block

    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(new_epoch_block_header_hash)), eq(mock_events_key()))
        .times(1)
        .returning(move |_, _| {
            Ok(vec![storage_change_set_from(
                vec![(
                    Phase::ApplyExtrinsic(0),
                    state_chain_runtime::Event::Validator(pallet_cf_validator::Event::NewEpoch(4)),
                    vec![H256::default()],
                )],
                new_epoch_block_header_hash,
            )])
        });

    // We will match on every block hash, but only the events key, as we want to return no events
    // on every block
    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(predicate::always(), eq(mock_events_key()))
        .times(3)
        .returning(|_, _| Ok(vec![]));

    let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
        mock_state_chain_rpc_client,
    ));

    let logger = new_test_logger();

    let eth_rpc_mock = MockEthRpcApi::new();

    let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

    let multisig_client = Arc::new(MockMultisigClientApi::new());

    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let (sm_window_sender, mut sm_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();
    let (km_window_sender, mut km_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();

    sc_observer::start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        sm_window_sender,
        km_window_sender,
        initial_block_hash,
        &logger,
    )
    .await;

    // ensure we did kick off the witness processes at the start
    assert_eq!(
        km_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_THREE_INITIAL
    );
    assert_eq!(
        sm_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_THREE_INITIAL
    );

    // after a new epoch, we should have sent new messages to stop witnessing once we reach the final block height
    assert_eq!(
        km_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_THREE_END
    );
    assert_eq!(
        sm_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_THREE_END
    );

    assert!(km_window_receiver.recv().await.is_none());
    assert!(sm_window_receiver.recv().await.is_none());
}

// TODO: We should test that this works for historical epochs too. We should be able to sign for historical epochs we
// were a part of
#[tokio::test]
async fn only_encodes_and_signs_when_current_authority_and_specified() {
    // === FAKE BLOCKHEADERS ===

    let block_header = test_header(21);
    let sc_block_stream = tokio_stream::iter(vec![Ok(block_header.clone())]);

    let mut eth_rpc_mock = MockEthRpcApi::new();

    // when we are selected to sign we must estimate gas and sign
    // NB: We only do this once, since we are only selected to sign once
    eth_rpc_mock
        .expect_estimate_gas()
        .times(1)
        .returning(|_, _| Ok(U256::from(100_000)));

    eth_rpc_mock
        .expect_sign_transaction()
        .times(1)
        .returning(|_, _| {
            // just a nothing signed transaction
            Ok(SignedTransaction {
                message_hash: H256::default(),
                v: 1,
                r: H256::default(),
                s: H256::default(),
                raw_transaction: Bytes(Vec::new()),
                transaction_hash: H256::default(),
            })
        });

    // Submits the extrinsic for the heartbeat
    // and for submitting `transaction_ready_for_broadcast()`
    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    mock_state_chain_rpc_client
        .expect_submit_extrinsic_rpc()
        .times(2)
        .returning(move |_| Ok(H256::default()));

    let initial_block_hash = H256::default();

    // get account info
    mock_state_chain_rpc_client
        .expect_storage()
        .with(eq(initial_block_hash), eq(mock_account_storage_key()))
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                account_info_from_data(ChainflipAccountState::CurrentAuthority).encode(),
            )))
        });

    // get the historical epochs
    mock_state_chain_rpc_client
        .expect_storage()
        .with(eq(initial_block_hash), eq(mock_historical_epochs_key()))
        .times(1)
        .returning(move |_, _| Ok(Some(StorageData(vec![3].encode()))));

    // get the current vault

    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(initial_block_hash),
            eq(StorageKey(
                Vaults::<Runtime, EthereumInstance>::hashed_key_for(&3),
            )),
        )
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                Vault::<Ethereum> {
                    public_key: AggKey::from_pubkey_compressed([0; 33]),
                    active_window: WINDOW_EPOCH_THREE_INITIAL,
                }
                .encode(),
            )))
        });

    // get the events for the new block - will contain 2 events, one for us to sign and one for us not to sign
    let block_header_hash = block_header.hash();
    mock_state_chain_rpc_client
        .expect_storage_events_at()
        .with(eq(Some(block_header_hash)), eq(mock_events_key()))
        .times(1)
        .returning(move |_, _| {
            Ok(vec![storage_change_set_from(
                vec![
                    (
                        // sign this one
                        Phase::ApplyExtrinsic(0),
                        state_chain_runtime::Event::EthereumBroadcaster(
                            pallet_cf_broadcast::Event::TransactionSigningRequest(
                                BroadcastAttemptId::default(),
                                AccountId32::new(OUR_ACCOUNT_ID_BYTES),
                                UnsignedTransaction::default(),
                            ),
                        ),
                        vec![H256::default()],
                    ),
                    (
                        // do NOT sign this one
                        Phase::ApplyExtrinsic(1),
                        state_chain_runtime::Event::EthereumBroadcaster(
                            pallet_cf_broadcast::Event::TransactionSigningRequest(
                                BroadcastAttemptId::default(),
                                AccountId32::new([1; 32]),
                                UnsignedTransaction::default(),
                            ),
                        ),
                        vec![H256::default()],
                    ),
                ],
                block_header_hash,
            )])
        });

    let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
        mock_state_chain_rpc_client,
    ));

    let logger = new_test_logger();

    let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

    let multisig_client = Arc::new(MockMultisigClientApi::new());

    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let (sm_window_sender, mut sm_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();
    let (km_window_sender, mut km_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();

    sc_observer::start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        sm_window_sender,
        km_window_sender,
        initial_block_hash,
        &logger,
    )
    .await;

    // ensure we kicked off the witness processes
    assert_eq!(
        km_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_THREE_INITIAL
    );
    assert_eq!(
        sm_window_receiver.recv().await.unwrap(),
        WINDOW_EPOCH_THREE_INITIAL
    );

    assert!(km_window_receiver.recv().await.is_none());
    assert!(sm_window_receiver.recv().await.is_none());
}

#[tokio::test]
#[ignore = "runs forever, useful for testing without having to start the whole CFE"]
async fn run_the_sc_observer() {
    let settings = new_test_settings().unwrap();
    let logger = logging::test_utils::new_test_logger();

    let (initial_block_hash, block_stream, state_chain_client) =
        crate::state_chain::client::connect_to_state_chain(&settings.state_chain, false, &logger)
            .await
            .unwrap();

    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let eth_ws_rpc_client = EthWsRpcClient::new(&settings.eth, &logger).await.unwrap();
    let eth_broadcaster =
        EthBroadcaster::new(&settings.eth, eth_ws_rpc_client.clone(), &logger).unwrap();

    let multisig_client = Arc::new(MockMultisigClientApi::new());

    let (sm_window_sender, _sm_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();
    let (km_window_sender, _km_window_receiver) =
        tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();

    sc_observer::start(
        state_chain_client,
        block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        sm_window_sender,
        km_window_sender,
        initial_block_hash,
        &logger,
    )
    .await;
}
