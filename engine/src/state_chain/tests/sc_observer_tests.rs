use std::{collections::BTreeSet, sync::Arc};

use cf_chains::{
    eth::{AggKey, UnsignedTransaction},
    Chain, Ethereum,
};
use codec::Encode;
use frame_system::Phase;
use mockall::predicate::{self, eq};
use pallet_cf_broadcast::BroadcastAttemptId;
use pallet_cf_vaults::{Vault, Vaults};
use sp_core::{
    storage::{StorageData, StorageKey},
    Hasher, H256, U256,
};
use sp_runtime::{traits::Keccak256, AccountId32, Digest};
use state_chain_runtime::{CfeSettings, EthereumInstance, Header, Runtime};
use tokio::sync::{broadcast, watch};
use web3::types::{Bytes, SignedTransaction};

use crate::{
    eth::{
        rpc::{EthWsRpcClient, MockEthRpcApi},
        EthBroadcaster, ObserveInstruction,
    },
    logging::test_utils::new_test_logger,
    multisig::client::{mocks::MockMultisigClientApi, CeremonyFailureReason},
    settings::Settings,
    state_chain::{
        client::{
            mock_events_key, test_utils::storage_change_set_from, MockStateChainRpcApi,
            StateChainClient, OUR_ACCOUNT_ID_BYTES,
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

/// ETH From Block for epoch three
const EPOCH_THREE_FROM: <cf_chains::Ethereum as Chain>::ChainBlockNumber = 30;
const EPOCH_THREE_START: ObserveInstruction = ObserveInstruction::Start(EPOCH_THREE_FROM, 3);
const EPOCH_THREE_END: ObserveInstruction = ObserveInstruction::End(EPOCH_FOUR_FROM);
/// ETH From Block for epoch four
const EPOCH_FOUR_FROM: <cf_chains::Ethereum as Chain>::ChainBlockNumber = 40;
const EPOCH_FOUR_START: ObserveInstruction = ObserveInstruction::Start(EPOCH_FOUR_FROM, 4);

fn expect_sc_observer_start(
    mock_state_chain_rpc_client: &mut MockStateChainRpcApi,
    historical_active_epochs: &[u32],
    epochs_active_from_block: &[(u32, Option<u64>)],
) -> H256 {
    let initial_block_hash = H256::default();

    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(initial_block_hash),
            eq(StorageKey(pallet_cf_validator::HistoricalActiveEpochs::<
                state_chain_runtime::Runtime,
            >::hashed_key_for(&AccountId32::new(
                OUR_ACCOUNT_ID_BYTES,
            )))),
        )
        .times(1)
        .returning({
            let historical_active_epochs = Vec::from(historical_active_epochs);
            move |_, _| Ok(Some(StorageData(historical_active_epochs.encode())))
        });

    for &(epoch, option_active_from_block) in epochs_active_from_block {
        mock_state_chain_rpc_client
            .expect_storage()
            .with(
                eq(initial_block_hash),
                eq(StorageKey(
                    Vaults::<Runtime, EthereumInstance>::hashed_key_for(&epoch),
                )),
            )
            .times(1)
            .returning(move |_, _| {
                Ok(option_active_from_block.map(|active_from_block| {
                    StorageData(
                        Vault::<Ethereum> {
                            public_key: AggKey::from_pubkey_compressed([0; 33]),
                            active_from_block,
                        }
                        .encode(),
                    )
                }))
            });
    }

    mock_state_chain_rpc_client
        .expect_submit_extrinsic_rpc()
        .times(1)
        .returning(move |_| Ok(H256::default()));

    initial_block_hash
}

#[tokio::test]
async fn sends_initial_extrinsics_and_starts_witnessing_when_current_authority_on_startup() {
    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    let initial_block_hash = expect_sc_observer_start(
        &mut mock_state_chain_rpc_client,
        &[3],
        &[(3, Some(EPOCH_THREE_FROM)), (4, None)],
    );
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

    let (instruction_sender, mut instruction_receiver) = broadcast::channel(10);

    let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

    sc_observer::start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        instruction_sender,
        cfe_settings_update_sender,
        initial_block_hash,
        &logger,
    )
    .await;

    // ensure we kicked off the witness processes
    assert_eq!(
        instruction_receiver.recv().await.unwrap(),
        EPOCH_THREE_START
    );
}

#[tokio::test]
async fn sends_initial_extrinsics_and_starts_witnessing_when_historic_on_startup() {
    // Current epoch is set to 4. Our last_active_epoch is set to 3.
    // So we should be deemed historical, and submit the block height windows as expected to the nodes

    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    let initial_block_hash = expect_sc_observer_start(
        &mut mock_state_chain_rpc_client,
        &[3],
        &[(3, Some(EPOCH_THREE_FROM)), (4, Some(EPOCH_FOUR_FROM))],
    );
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

    let (instruction_sender, mut instruction_receiver) = broadcast::channel(10);

    let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

    sc_observer::start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        instruction_sender,
        cfe_settings_update_sender,
        initial_block_hash,
        &logger,
    )
    .await;

    // ensure we kicked off the witness processes
    assert_eq!(
        instruction_receiver.recv().await.unwrap(),
        EPOCH_THREE_START
    );

    assert_eq!(instruction_receiver.recv().await.unwrap(), EPOCH_THREE_END);

    assert!(instruction_receiver.recv().await.is_err());
}

#[tokio::test]
async fn sends_initial_extrinsics_when_not_historic_on_startup() {
    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    let initial_block_hash = expect_sc_observer_start(&mut mock_state_chain_rpc_client, &[], &[]);
    let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
        mock_state_chain_rpc_client,
    ));

    let sc_block_stream = tokio_stream::iter(vec![]);

    let logger = new_test_logger();

    let eth_rpc_mock = MockEthRpcApi::new();

    let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

    let multisig_client = Arc::new(MockMultisigClientApi::new());

    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let (instruction_sender, mut instruction_receiver) = broadcast::channel(10);
    let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

    sc_observer::start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        instruction_sender,
        cfe_settings_update_sender,
        initial_block_hash,
        &logger,
    )
    .await;

    // ensure we did NOT kick off the witness processes - as we are *only* backup, not outgoing
    assert!(instruction_receiver.recv().await.is_err());
}

#[tokio::test]
async fn current_authority_to_current_authority_on_new_epoch_event() {
    let logger = new_test_logger();

    let eth_broadcaster = EthBroadcaster::new_test(MockEthRpcApi::new(), &logger);

    let multisig_client = Arc::new(MockMultisigClientApi::new());

    // === FAKE BLOCKHEADERS ===
    // two empty blocks in the stream
    let empty_block_header = test_header(20);
    let new_epoch_block_header = test_header(21);
    let new_epoch_block_header_hash = new_epoch_block_header.hash();

    let sc_block_stream = tokio_stream::iter(vec![
        Ok(empty_block_header.clone()),
        // in the mock for the events, we return a new epoch event for the block with this header
        Ok(new_epoch_block_header.clone()),
    ]);

    let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
        tokio::sync::mpsc::unbounded_channel();

    let (instruction_sender, mut instruction_receiver) = broadcast::channel(10);

    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    let initial_block_hash = expect_sc_observer_start(
        &mut mock_state_chain_rpc_client,
        &[3],
        &[(3, Some(EPOCH_THREE_FROM)), (4, None)],
    );

    let vault_key_after_new_epoch =
        StorageKey(Vaults::<Runtime, EthereumInstance>::hashed_key_for(&4));

    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(new_epoch_block_header_hash),
            eq(vault_key_after_new_epoch),
        )
        .times(2)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                Vault::<Ethereum> {
                    public_key: AggKey::from_pubkey_compressed([0; 33]),
                    active_from_block: EPOCH_FOUR_FROM,
                }
                .encode(),
            )))
        });
    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(new_epoch_block_header_hash),
            eq(StorageKey(
                pallet_cf_validator::AuthorityIndex::<Runtime>::hashed_key_for(
                    &4,
                    &AccountId32::new(OUR_ACCOUNT_ID_BYTES),
                ),
            )),
        )
        .times(1)
        .returning(move |_, _| Ok(Some(StorageData(1_u32.encode()))));

    // Heartbeat on block number 20
    mock_state_chain_rpc_client
        .expect_submit_extrinsic_rpc()
        .times(1)
        .returning(move |_| Ok(H256::default()));

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

    let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

    sc_observer::start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        instruction_sender,
        cfe_settings_update_sender,
        initial_block_hash,
        &logger,
    )
    .await;

    // ensure we did kick off the witness processes at the start
    assert_eq!(
        instruction_receiver.recv().await.unwrap(),
        EPOCH_THREE_START
    );

    assert_eq!(instruction_receiver.recv().await.unwrap(), EPOCH_THREE_END);

    // after a new epoch, we should have sent new messages down the channels
    assert_eq!(instruction_receiver.recv().await.unwrap(), EPOCH_FOUR_START);

    assert!(instruction_receiver.recv().await.is_err());
}

#[tokio::test]
async fn not_historical_to_authority_on_new_epoch() {
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

    let (instruction_sender, mut instruction_receiver) = broadcast::channel(10);

    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    let initial_block_hash = expect_sc_observer_start(&mut mock_state_chain_rpc_client, &[], &[]);

    // Heartbeat on block number 20
    mock_state_chain_rpc_client
        .expect_submit_extrinsic_rpc()
        .times(1)
        .returning(move |_| Ok(H256::default()));

    let new_epoch_block_header_hash = new_epoch_block_header.hash();

    let new_epoch = 4;

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
                    active_from_block: EPOCH_FOUR_FROM,
                }
                .encode(),
            )))
        });
    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(new_epoch_block_header_hash),
            eq(StorageKey(
                pallet_cf_validator::AuthorityIndex::<Runtime>::hashed_key_for(
                    &4,
                    &AccountId32::new(OUR_ACCOUNT_ID_BYTES),
                ),
            )),
        )
        .times(1)
        .returning(move |_, _| Ok(Some(StorageData(1_u32.encode()))));

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

    let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

    sc_observer::start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        instruction_sender,
        cfe_settings_update_sender,
        initial_block_hash,
        &logger,
    )
    .await;

    // after a new epoch, we should have sent new messages down the channels
    assert_eq!(instruction_receiver.recv().await.unwrap(), EPOCH_FOUR_START);

    assert!(instruction_receiver.recv().await.is_err());
}

#[tokio::test]
async fn current_authority_to_historical_on_new_epoch_event() {
    // === FAKE BLOCKHEADERS ===
    let empty_block_header = test_header(20);
    let new_epoch_block_header = test_header(21);

    let sc_block_stream = tokio_stream::iter([
        Ok(empty_block_header.clone()),
        // in the mock for the events, we return a new epoch event for the block with this header
        Ok(new_epoch_block_header.clone()),
        // after we become a historical authority, we should keep checking for our status as a node now
        Ok(test_header(22)),
        Ok(test_header(23)),
    ]);

    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    let initial_block_hash = expect_sc_observer_start(
        &mut mock_state_chain_rpc_client,
        &[3],
        &[(3, Some(EPOCH_THREE_FROM)), (4, None)],
    );

    // Heartbeat on block number 20
    mock_state_chain_rpc_client
        .expect_submit_extrinsic_rpc()
        .times(1)
        .returning(move |_| Ok(H256::default()));

    let new_epoch_block_header_hash = new_epoch_block_header.hash();

    // get the current vault
    let vault_key = StorageKey(Vaults::<Runtime, EthereumInstance>::hashed_key_for(&4));

    // NB: Because we're outgoing, we use the same vault key, now we have a close to the window
    mock_state_chain_rpc_client
        .expect_storage()
        .with(eq(new_epoch_block_header_hash), eq(vault_key))
        .times(1)
        .returning(move |_, _| {
            Ok(Some(StorageData(
                Vault::<Ethereum> {
                    public_key: AggKey::from_pubkey_compressed([0; 33]),
                    active_from_block: EPOCH_FOUR_FROM,
                }
                .encode(),
            )))
        });
    mock_state_chain_rpc_client
        .expect_storage()
        .with(
            eq(new_epoch_block_header_hash),
            eq(StorageKey(
                pallet_cf_validator::AuthorityIndex::<Runtime>::hashed_key_for(
                    &4,
                    &AccountId32::new(OUR_ACCOUNT_ID_BYTES),
                ),
            )),
        )
        .times(1)
        .returning(move |_, _| Ok(None));

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

    let (instruction_sender, mut instruction_receiver) = broadcast::channel(10);

    let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

    sc_observer::start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        instruction_sender,
        cfe_settings_update_sender,
        initial_block_hash,
        &logger,
    )
    .await;

    // ensure we did kick off the witness processes at the start
    assert_eq!(
        instruction_receiver.recv().await.unwrap(),
        EPOCH_THREE_START
    );

    assert_eq!(instruction_receiver.recv().await.unwrap(), EPOCH_THREE_END);

    assert!(instruction_receiver.recv().await.is_err());
}

// TODO: We should test that this works for historical epochs too. We should be able to sign for historical epochs we
// were a part of
#[tokio::test]
async fn only_encodes_and_signs_when_specified() {
    // === FAKE BLOCKHEADERS ===

    let block_header = test_header(21);
    let sc_block_stream = tokio_stream::iter([Ok(block_header.clone())]);

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

    eth_rpc_mock
        .expect_send_raw_transaction()
        .times(1)
        .returning(|tx| Ok(Keccak256::hash(&tx.0[..])));

    let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
    let initial_block_hash = expect_sc_observer_start(
        &mut mock_state_chain_rpc_client,
        &[3],
        &[(3, Some(EPOCH_THREE_FROM)), (4, None)],
    );

    // Submitting `transaction_ready_for_broadcast()`
    mock_state_chain_rpc_client
        .expect_submit_extrinsic_rpc()
        .times(1)
        .returning(move |_| Ok(H256::default()));

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

    let (instruction_sender, mut instruction_receiver) = broadcast::channel(10);

    let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

    sc_observer::start(
        state_chain_client,
        sc_block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        instruction_sender,
        cfe_settings_update_sender,
        initial_block_hash,
        &logger,
    )
    .await;

    // ensure we kicked off the witness processes
    assert_eq!(
        instruction_receiver.recv().await.unwrap(),
        EPOCH_THREE_START
    );

    assert!(instruction_receiver.recv().await.is_err());
}

#[tokio::test]
#[ignore = "runs forever, useful for testing without having to start the whole CFE"]
async fn run_the_sc_observer() {
    let settings = Settings::new_test().unwrap();
    let logger = new_test_logger();

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

    let (instruction_sender, _) = broadcast::channel(10);

    let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

    sc_observer::start(
        state_chain_client,
        block_stream,
        eth_broadcaster,
        multisig_client,
        account_peer_mapping_change_sender,
        instruction_sender,
        cfe_settings_update_sender,
        initial_block_hash,
        &logger,
    )
    .await;
}

// Test that the ceremony requests are calling the correct MultisigClientApi functions
// depending on whether we are participating in the ceremony or not.
#[tokio::test]
async fn should_handle_ceremony_request() {
    let logger = new_test_logger();
    let latest_ceremony_id = 1;
    let key_id = crate::multisig::KeyId(vec![0u8; 32]);
    let sign_data = crate::multisig::MessageHash([0u8; 32]);
    let our_account_id = AccountId32::new(OUR_ACCOUNT_ID_BYTES);
    let not_our_account_id = AccountId32::new([1u8; 32]);
    assert_ne!(our_account_id, not_our_account_id);

    // Mock interfaces
    let mut multisig_client = MockMultisigClientApi::new();
    let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
        MockStateChainRpcApi::new(),
    ));

    // Set up the mock api to expect the 2 not_participating_ceremony calls
    multisig_client
        .expect_not_participating_ceremony()
        .with(predicate::eq(latest_ceremony_id))
        .returning(|_| ());

    multisig_client
        .expect_not_participating_ceremony()
        .with(predicate::eq(latest_ceremony_id + 1))
        .returning(|_| ());

    // Set up the mock api to expect the keygen and sign calls for the ceremonies we are participating in.
    // It doesn't matter what failure reasons they return.
    multisig_client
        .expect_keygen()
        .with(
            predicate::eq(latest_ceremony_id + 2),
            predicate::eq(vec![our_account_id.clone()]),
        )
        .returning(|_, _| {
            Err((
                BTreeSet::new(),
                CeremonyFailureReason::ExpiredBeforeBeingAuthorized,
            ))
        });

    multisig_client
        .expect_sign()
        .with(
            predicate::eq(latest_ceremony_id + 3),
            predicate::eq(key_id.clone()),
            predicate::eq(vec![our_account_id.clone()]),
            predicate::eq(sign_data.clone()),
        )
        .returning(|_, _, _, _| {
            Err((
                BTreeSet::new(),
                CeremonyFailureReason::ExpiredBeforeBeingAuthorized,
            ))
        });

    let multisig_client = Arc::new(multisig_client);

    // Handle a keygen request that we are not participating in
    sc_observer::test_handle_keygen_request(
        multisig_client.clone(),
        state_chain_client.clone(),
        latest_ceremony_id,
        vec![not_our_account_id.clone()],
        logger.clone(),
    )
    .await;

    // Handle a signing request that we are not participating in
    sc_observer::test_handle_signing_request(
        multisig_client.clone(),
        state_chain_client.clone(),
        latest_ceremony_id + 1,
        key_id.clone(),
        vec![not_our_account_id.clone()],
        sign_data.clone(),
        logger.clone(),
    )
    .await;

    // Handle a keygen request that we are participating in
    sc_observer::test_handle_keygen_request(
        multisig_client.clone(),
        state_chain_client.clone(),
        latest_ceremony_id + 2,
        vec![our_account_id.clone()],
        logger.clone(),
    )
    .await;

    // Handle a signing request that we are participating in
    sc_observer::test_handle_signing_request(
        multisig_client,
        state_chain_client.clone(),
        latest_ceremony_id + 3,
        key_id,
        vec![our_account_id],
        sign_data,
        logger,
    )
    .await;
}
