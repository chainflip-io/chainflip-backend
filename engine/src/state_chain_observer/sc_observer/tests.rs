// use std::{collections::BTreeSet, sync::Arc};

// use cf_chains::{
//     eth::{AggKey, UnsignedTransaction},
//     Chain, Ethereum,
// };
// use codec::Encode;
// use frame_system::Phase;
// use futures::FutureExt;
// use itertools::Itertools;
// use mockall::predicate::{self, eq};
// use pallet_cf_broadcast::BroadcastAttemptId;
// use pallet_cf_vaults::{Vault, Vaults};

// use sp_core::{
//     storage::{StorageData, StorageKey},
//     Hasher, H256, U256,
// };
// use sp_runtime::{traits::Keccak256, AccountId32, Digest};
// use state_chain_runtime::{CfeSettings, EthereumInstance, Header, Runtime};
// use tokio::sync::{broadcast, watch};
// use web3::types::{Bytes, SignedTransaction};

// use crate::{
//     eth::{
//         rpc::{EthWsRpcClient, MockEthRpcApi},
//         EpochStart, EthBroadcaster,
//     },
//     logging::test_utils::new_test_logger,
//     multisig::client::{mocks::MockMultisigClientApi, CeremonyFailureReason},
//     settings::Settings,
//     state_chain_observer::{
//         client::{
//             mock_events_key, test_utils::storage_change_set_from, MockStateChainRpcApi,
//             StateChainClient, OUR_ACCOUNT_ID_BYTES,
//         },
//         sc_observer,
//     },
//     task_scope::with_task_scope,
// };

// fn test_header(number: u32) -> Header {
//     Header {
//         number,
//         parent_hash: H256::default(),
//         state_root: H256::default(),
//         extrinsics_root: H256::default(),
//         digest: Digest { logs: Vec::new() },
//     }
// }

// /// Epoch index for epoch index
// const EPOCH_FOUR_INDEX: u32 = 4;
// /// ETH From Block for epoch four
// const EPOCH_FOUR_FROM: <cf_chains::Ethereum as Chain>::ChainBlockNumber = 40;

// fn expectations_on_start(
//     mock_state_chain_rpc_client: &mut MockStateChainRpcApi,
//     historical_epochs: &[(u32, bool, u64)],
// ) -> (H256, Vec<EpochStart>) {
//     assert!(!historical_epochs.is_empty());

//     let initial_block_hash = H256::default();

//     mock_state_chain_rpc_client
//         .expect_storage()
//         .with(
//             eq(initial_block_hash),
//             eq(StorageKey(pallet_cf_validator::HistoricalActiveEpochs::<
//                 state_chain_runtime::Runtime,
//             >::hashed_key_for(&AccountId32::new(
//                 OUR_ACCOUNT_ID_BYTES,
//             )))),
//         )
//         .times(1)
//         .return_once({
//             let historical_active_epochs = historical_epochs
//                 .iter()
//                 .filter(|(_epoch, participant, _eth_block)| *participant)
//                 .map(|(epoch, ..)| *epoch)
//                 .collect::<Vec<_>>()
//                 .encode();
//             move |_, _| Ok(Some(StorageData(historical_active_epochs)))
//         });

//     mock_state_chain_rpc_client
//         .expect_storage()
//         .with(
//             eq(initial_block_hash),
//             eq(StorageKey(
//                 pallet_cf_validator::CurrentEpoch::<state_chain_runtime::Runtime>::hashed_key()
//                     .into(),
//             )),
//         )
//         .times(1)
//         .returning({
//             let last_epoch = historical_epochs.last().unwrap().0;
//             move |_, _| Ok(Some(StorageData(last_epoch.encode())))
//         });

//     for &(epoch, _participate, eth_block) in historical_epochs {
//         mock_state_chain_rpc_client
//             .expect_storage()
//             .with(
//                 eq(initial_block_hash),
//                 eq(StorageKey(
//                     Vaults::<Runtime, EthereumInstance>::hashed_key_for(&epoch),
//                 )),
//             )
//             .times(1)
//             .returning(move |_, _| {
//                 Ok(Some(StorageData(
//                     Vault::<Ethereum> {
//                         public_key: AggKey::from_pubkey_compressed([0; 33]),
//                         active_from_block: eth_block,
//                     }
//                     .encode(),
//                 )))
//             });
//     }

//     mock_state_chain_rpc_client
//         .expect_submit_extrinsic_rpc()
//         .never();

//     (
//         initial_block_hash,
//         // Expected EpochStart's output via epoch_start_sender
//         historical_epochs
//             .iter()
//             .with_position()
//             .map(|epoch| {
//                 let current = matches!(
//                     &epoch,
//                     itertools::Position::Only(_) | itertools::Position::Last(_)
//                 );
//                 let (epoch, participant, eth_block) = epoch.into_inner();

//                 EpochStart {
//                     index: *epoch,
//                     eth_block: *eth_block,
//                     current,
//                     participant: *participant,
//                 }
//             })
//             .collect(),
//     )
// }

// #[tokio::test]
// async fn sends_initial_extrinsics_and_starts_witnessing_when_current_authority_on_startup() {
//     let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
//     let (initial_block_hash, expected_epoch_starts) =
//         expectations_on_start(&mut mock_state_chain_rpc_client, &[(3, true, 30)]);
//     let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
//         mock_state_chain_rpc_client,
//     ));

//     let multisig_client = Arc::new(MockMultisigClientApi::new());

//     // No blocks in the stream
//     let sc_block_stream = tokio_stream::iter(vec![]);

//     let logger = new_test_logger();

//     let eth_rpc_mock = MockEthRpcApi::new();

//     let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

//     let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     let (epoch_start_sender, mut epoch_start_receiver) = broadcast::channel(10);

//     let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

//     let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     sc_observer::start(
//         state_chain_client,
//         sc_block_stream,
//         eth_broadcaster,
//         multisig_client,
//         account_peer_mapping_change_sender,
//         epoch_start_sender,
//         eth_monitor_ingress_sender,
//         cfe_settings_update_sender,
//         initial_block_hash,
//         logger,
//     )
//     .await
//     .unwrap_err();

//     for epoch_start in expected_epoch_starts {
//         assert_eq!(epoch_start_receiver.recv().await.unwrap(), epoch_start);
//     }

//     assert!(epoch_start_receiver.recv().await.is_err());
// }

// #[tokio::test]
// async fn sends_initial_extrinsics_and_starts_witnessing_when_historic_on_startup() {
//     let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
//     let (initial_block_hash, expected_epoch_starts) = expectations_on_start(
//         &mut mock_state_chain_rpc_client,
//         &[
//             (EPOCH_FOUR_INDEX - 1, true, EPOCH_FOUR_FROM - 10),
//             (EPOCH_FOUR_INDEX, false, EPOCH_FOUR_FROM),
//         ],
//     );
//     let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
//         mock_state_chain_rpc_client,
//     ));

//     // No blocks in the stream
//     let sc_block_stream = tokio_stream::iter(vec![]);

//     let logger = new_test_logger();

//     let eth_rpc_mock = MockEthRpcApi::new();

//     let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

//     let multisig_client = Arc::new(MockMultisigClientApi::new());

//     let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     let (epoch_start_sender, mut epoch_start_receiver) = broadcast::channel(10);

//     let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

//     let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     sc_observer::start(
//         state_chain_client,
//         sc_block_stream,
//         eth_broadcaster,
//         multisig_client,
//         account_peer_mapping_change_sender,
//         epoch_start_sender,
//         eth_monitor_ingress_sender,
//         cfe_settings_update_sender,
//         initial_block_hash,
//         logger,
//     )
//     .await
//     .unwrap_err();

//     for epoch_start in expected_epoch_starts {
//         assert_eq!(epoch_start_receiver.recv().await.unwrap(), epoch_start);
//     }

//     assert!(epoch_start_receiver.recv().await.is_err());
// }

// #[tokio::test]
// async fn sends_initial_extrinsics_when_not_historic_on_startup() {
//     let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
//     let (initial_block_hash, expected_epoch_starts) =
//         expectations_on_start(&mut mock_state_chain_rpc_client, &[(3, false, 30)]);
//     let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
//         mock_state_chain_rpc_client,
//     ));

//     let sc_block_stream = tokio_stream::iter(vec![]);

//     let logger = new_test_logger();

//     let eth_rpc_mock = MockEthRpcApi::new();

//     let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

//     let multisig_client = Arc::new(MockMultisigClientApi::new());

//     let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     let (epoch_start_sender, mut epoch_start_receiver) = broadcast::channel(10);
//     let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

//     let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     sc_observer::start(
//         state_chain_client,
//         sc_block_stream,
//         eth_broadcaster,
//         multisig_client,
//         account_peer_mapping_change_sender,
//         epoch_start_sender,
//         eth_monitor_ingress_sender,
//         cfe_settings_update_sender,
//         initial_block_hash,
//         logger,
//     )
//     .await
//     .unwrap_err();

//     for epoch_start in expected_epoch_starts {
//         assert_eq!(epoch_start_receiver.recv().await.unwrap(), epoch_start);
//     }

//     assert!(epoch_start_receiver.recv().await.is_err());
// }

// #[tokio::test]
// async fn current_authority_to_current_authority_on_new_epoch_event() {
//     let logger = new_test_logger();

//     let eth_broadcaster = EthBroadcaster::new_test(MockEthRpcApi::new(), &logger);

//     let multisig_client = Arc::new(MockMultisigClientApi::new());

//     // === FAKE BLOCKHEADERS ===
//     // two empty blocks in the stream
//     let empty_block_header = test_header(20);
//     let new_epoch_block_header = test_header(21);
//     let new_epoch_block_header_hash = new_epoch_block_header.hash();

//     let sc_block_stream = tokio_stream::iter(vec![
//         Ok(empty_block_header.clone()),
//         // in the mock for the events, we return a new epoch event for the block with this header
//         Ok(new_epoch_block_header.clone()),
//     ]);

//     let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     let (epoch_start_sender, mut epoch_start_receiver) = broadcast::channel(10);

//     let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
//     let (initial_block_hash, expected_epoch_starts) = expectations_on_start(
//         &mut mock_state_chain_rpc_client,
//         &[(EPOCH_FOUR_INDEX, true, EPOCH_FOUR_FROM - 10)],
//     );

//     let vault_key_after_new_epoch = StorageKey(
//         Vaults::<Runtime, EthereumInstance>::hashed_key_for(&EPOCH_FOUR_INDEX),
//     );

//     mock_state_chain_rpc_client
//         .expect_storage()
//         .with(
//             eq(new_epoch_block_header_hash),
//             eq(vault_key_after_new_epoch),
//         )
//         .times(1)
//         .returning(move |_, _| {
//             Ok(Some(StorageData(
//                 Vault::<Ethereum> {
//                     public_key: AggKey::from_pubkey_compressed([0; 33]),
//                     active_from_block: EPOCH_FOUR_FROM,
//                 }
//                 .encode(),
//             )))
//         });
//     mock_state_chain_rpc_client
//         .expect_storage()
//         .with(
//             eq(new_epoch_block_header_hash),
//             eq(StorageKey(
//                 pallet_cf_validator::AuthorityIndex::<Runtime>::hashed_key_for(
//                     &EPOCH_FOUR_INDEX,
//                     &AccountId32::new(OUR_ACCOUNT_ID_BYTES),
//                 ),
//             )),
//         )
//         .times(1)
//         .returning(move |_, _| Ok(Some(StorageData(1_u32.encode()))));

//     // Get events from the block
//     // We will match on every block hash, but only the events key, as we want to return no events
//     // on every block
//     mock_state_chain_rpc_client
//         .expect_storage_events_at()
//         .with(eq(Some(empty_block_header.hash())), eq(mock_events_key()))
//         .times(1)
//         .returning(|_, _| Ok(vec![]));

//     mock_state_chain_rpc_client
//         .expect_storage_events_at()
//         .with(eq(Some(new_epoch_block_header_hash)), eq(mock_events_key()))
//         .times(1)
//         .returning(move |_, _| {
//             Ok(vec![storage_change_set_from(
//                 vec![(
//                     Phase::ApplyExtrinsic(0),
//                     state_chain_runtime::Event::Validator(pallet_cf_validator::Event::NewEpoch(
//                         EPOCH_FOUR_INDEX,
//                     )),
//                     vec![H256::default()],
//                 )],
//                 new_epoch_block_header_hash,
//             )])
//         });

//     let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
//         mock_state_chain_rpc_client,
//     ));

//     let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

//     let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     sc_observer::start(
//         state_chain_client,
//         sc_block_stream,
//         eth_broadcaster,
//         multisig_client,
//         account_peer_mapping_change_sender,
//         epoch_start_sender,
//         eth_monitor_ingress_sender,
//         cfe_settings_update_sender,
//         initial_block_hash,
//         logger,
//     )
//     .await
//     .unwrap_err();

//     for epoch_start in expected_epoch_starts {
//         assert_eq!(epoch_start_receiver.recv().await.unwrap(), epoch_start);
//     }

//     assert_eq!(
//         epoch_start_receiver.recv().await.unwrap(),
//         EpochStart {
//             index: EPOCH_FOUR_INDEX,
//             eth_block: EPOCH_FOUR_FROM,
//             current: true,
//             participant: true
//         }
//     );

//     assert!(epoch_start_receiver.recv().await.is_err());
// }

// #[tokio::test]
// async fn not_historical_to_authority_on_new_epoch() {
//     let logger = new_test_logger();

//     let eth_rpc_mock = MockEthRpcApi::new();

//     let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

//     let multisig_client = Arc::new(MockMultisigClientApi::new());

//     // === FAKE BLOCKHEADERS ===
//     // two empty blocks in the stream
//     let empty_block_header = test_header(20);
//     let new_epoch_block_header = test_header(21);

//     let sc_block_stream = tokio_stream::iter(vec![
//         Ok(empty_block_header.clone()),
//         // in the mock for the events, we return a new epoch event for the block with this header
//         Ok(new_epoch_block_header.clone()),
//     ]);

//     let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     let (epoch_start_sender, mut epoch_start_receiver) = broadcast::channel(10);

//     let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
//     let (initial_block_hash, expected_epoch_starts) = expectations_on_start(
//         &mut mock_state_chain_rpc_client,
//         &[(EPOCH_FOUR_INDEX - 1, false, EPOCH_FOUR_FROM - 10)],
//     );

//     let new_epoch_block_header_hash = new_epoch_block_header.hash();

//     // We'll get the vault from the new epoch `new_epoch` when we become active
//     mock_state_chain_rpc_client
//         .expect_storage()
//         .with(
//             eq(new_epoch_block_header_hash),
//             eq(StorageKey(
//                 Vaults::<Runtime, EthereumInstance>::hashed_key_for(&EPOCH_FOUR_INDEX),
//             )),
//         )
//         .times(1)
//         .returning(move |_, _| {
//             Ok(Some(StorageData(
//                 Vault::<Ethereum> {
//                     public_key: AggKey::from_pubkey_compressed([0; 33]),
//                     active_from_block: EPOCH_FOUR_FROM,
//                 }
//                 .encode(),
//             )))
//         });
//     mock_state_chain_rpc_client
//         .expect_storage()
//         .with(
//             eq(new_epoch_block_header_hash),
//             eq(StorageKey(
//                 pallet_cf_validator::AuthorityIndex::<Runtime>::hashed_key_for(
//                     &EPOCH_FOUR_INDEX,
//                     &AccountId32::new(OUR_ACCOUNT_ID_BYTES),
//                 ),
//             )),
//         )
//         .times(1)
//         .returning(move |_, _| Ok(Some(StorageData(1_u32.encode()))));

//     // Get events from the block
//     // We will match on every block hash, but only the events key, as we want to return no events
//     // on every block
//     mock_state_chain_rpc_client
//         .expect_storage_events_at()
//         .with(eq(Some(empty_block_header.hash())), eq(mock_events_key()))
//         .times(1)
//         .returning(|_, _| Ok(vec![]));

//     mock_state_chain_rpc_client
//         .expect_storage_events_at()
//         .with(eq(Some(new_epoch_block_header_hash)), eq(mock_events_key()))
//         .times(1)
//         .returning(move |_, _| {
//             Ok(vec![storage_change_set_from(
//                 vec![(
//                     Phase::ApplyExtrinsic(0),
//                     state_chain_runtime::Event::Validator(pallet_cf_validator::Event::NewEpoch(
//                         EPOCH_FOUR_INDEX,
//                     )),
//                     vec![H256::default()],
//                 )],
//                 new_epoch_block_header_hash,
//             )])
//         });

//     let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
//         mock_state_chain_rpc_client,
//     ));

//     let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

//     let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     sc_observer::start(
//         state_chain_client,
//         sc_block_stream,
//         eth_broadcaster,
//         multisig_client,
//         account_peer_mapping_change_sender,
//         epoch_start_sender,
//         eth_monitor_ingress_sender,
//         cfe_settings_update_sender,
//         initial_block_hash,
//         logger,
//     )
//     .await
//     .unwrap_err();

//     for epoch_start in expected_epoch_starts {
//         assert_eq!(epoch_start_receiver.recv().await.unwrap(), epoch_start);
//     }

//     assert_eq!(
//         epoch_start_receiver.recv().await.unwrap(),
//         EpochStart {
//             index: EPOCH_FOUR_INDEX,
//             eth_block: EPOCH_FOUR_FROM,
//             current: true,
//             participant: true
//         }
//     );

//     assert!(epoch_start_receiver.recv().await.is_err());
// }

// #[tokio::test]
// async fn current_authority_to_historical_on_new_epoch_event() {
//     // === FAKE BLOCKHEADERS ===
//     let empty_block_header = test_header(20);
//     let new_epoch_block_header = test_header(21);

//     let sc_block_stream = tokio_stream::iter([
//         Ok(empty_block_header.clone()),
//         // in the mock for the events, we return a new epoch event for the block with this header
//         Ok(new_epoch_block_header.clone()),
//         // after we become a historical authority, we should keep checking for our status as a node now
//         Ok(test_header(22)),
//         Ok(test_header(23)),
//     ]);

//     let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
//     let (initial_block_hash, expected_epoch_starts) =
//         expectations_on_start(&mut mock_state_chain_rpc_client, &[(3, true, 30)]);

//     let new_epoch_block_header_hash = new_epoch_block_header.hash();

//     // get the current vault
//     let vault_key = StorageKey(Vaults::<Runtime, EthereumInstance>::hashed_key_for(
//         &EPOCH_FOUR_INDEX,
//     ));

//     // NB: Because we're outgoing, we use the same vault key, now we have a close to the window
//     mock_state_chain_rpc_client
//         .expect_storage()
//         .with(eq(new_epoch_block_header_hash), eq(vault_key))
//         .times(1)
//         .returning(move |_, _| {
//             Ok(Some(StorageData(
//                 Vault::<Ethereum> {
//                     public_key: AggKey::from_pubkey_compressed([0; 33]),
//                     active_from_block: EPOCH_FOUR_FROM,
//                 }
//                 .encode(),
//             )))
//         });
//     mock_state_chain_rpc_client
//         .expect_storage()
//         .with(
//             eq(new_epoch_block_header_hash),
//             eq(StorageKey(
//                 pallet_cf_validator::AuthorityIndex::<Runtime>::hashed_key_for(
//                     &EPOCH_FOUR_INDEX,
//                     &AccountId32::new(OUR_ACCOUNT_ID_BYTES),
//                 ),
//             )),
//         )
//         .times(1)
//         .returning(move |_, _| Ok(None));

//     // Get events from the block
//     mock_state_chain_rpc_client
//         .expect_storage_events_at()
//         .with(eq(Some(new_epoch_block_header_hash)), eq(mock_events_key()))
//         .times(1)
//         .returning(move |_, _| {
//             Ok(vec![storage_change_set_from(
//                 vec![(
//                     Phase::ApplyExtrinsic(0),
//                     state_chain_runtime::Event::Validator(pallet_cf_validator::Event::NewEpoch(
//                         EPOCH_FOUR_INDEX,
//                     )),
//                     vec![H256::default()],
//                 )],
//                 new_epoch_block_header_hash,
//             )])
//         });

//     // We will match on every block hash, but only the events key, as we want to return no events
//     // on every block
//     mock_state_chain_rpc_client
//         .expect_storage_events_at()
//         .with(predicate::always(), eq(mock_events_key()))
//         .times(3)
//         .returning(|_, _| Ok(vec![]));

//     let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
//         mock_state_chain_rpc_client,
//     ));

//     let logger = new_test_logger();

//     let eth_rpc_mock = MockEthRpcApi::new();

//     let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

//     let multisig_client = Arc::new(MockMultisigClientApi::new());

//     let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     let (epoch_start_sender, mut epoch_start_receiver) = broadcast::channel(10);

//     let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

//     let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     sc_observer::start(
//         state_chain_client,
//         sc_block_stream,
//         eth_broadcaster,
//         multisig_client,
//         account_peer_mapping_change_sender,
//         epoch_start_sender,
//         eth_monitor_ingress_sender,
//         cfe_settings_update_sender,
//         initial_block_hash,
//         logger,
//     )
//     .await
//     .unwrap_err();

//     for epoch_start in expected_epoch_starts {
//         assert_eq!(epoch_start_receiver.recv().await.unwrap(), epoch_start);
//     }

//     assert_eq!(
//         epoch_start_receiver.recv().await.unwrap(),
//         EpochStart {
//             index: EPOCH_FOUR_INDEX,
//             eth_block: EPOCH_FOUR_FROM,
//             current: true,
//             participant: false
//         }
//     );

//     assert!(epoch_start_receiver.recv().await.is_err());
// }

// // TODO: We should test that this works for historical epochs too. We should be able to sign for historical epochs we
// // were a part of
// #[tokio::test]
// async fn only_encodes_and_signs_when_specified() {
//     // === FAKE BLOCKHEADERS ===

//     let block_header = test_header(21);
//     let sc_block_stream = tokio_stream::iter([Ok(block_header.clone())]);

//     let mut eth_rpc_mock = MockEthRpcApi::new();

//     // when we are selected to sign we must estimate gas and sign
//     // NB: We only do this once, since we are only selected to sign once
//     eth_rpc_mock
//         .expect_estimate_gas()
//         .times(1)
//         .returning(|_, _| Ok(U256::from(100_000)));

//     eth_rpc_mock
//         .expect_sign_transaction()
//         .times(1)
//         .returning(|_, _| {
//             // just a nothing signed transaction
//             Ok(SignedTransaction {
//                 message_hash: H256::default(),
//                 v: 1,
//                 r: H256::default(),
//                 s: H256::default(),
//                 raw_transaction: Bytes(Vec::new()),
//                 transaction_hash: H256::default(),
//             })
//         });

//     eth_rpc_mock
//         .expect_send_raw_transaction()
//         .times(1)
//         .returning(|tx| Ok(Keccak256::hash(&tx.0[..])));

//     let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
//     let (initial_block_hash, expected_epoch_starts) =
//         expectations_on_start(&mut mock_state_chain_rpc_client, &[(3, true, 30)]);

//     // Submitting `transaction_ready_for_broadcast()`
//     mock_state_chain_rpc_client
//         .expect_submit_extrinsic_rpc()
//         .times(1)
//         .returning(move |_| Ok(H256::default()));

//     // get the events for the new block - will contain 2 events, one for us to sign and one for us not to sign
//     let block_header_hash = block_header.hash();
//     mock_state_chain_rpc_client
//         .expect_storage_events_at()
//         .with(eq(Some(block_header_hash)), eq(mock_events_key()))
//         .times(1)
//         .returning(move |_, _| {
//             Ok(vec![storage_change_set_from(
//                 vec![
//                     (
//                         // sign this one
//                         Phase::ApplyExtrinsic(0),
//                         state_chain_runtime::Event::EthereumBroadcaster(
//                             pallet_cf_broadcast::Event::TransactionSigningRequest(
//                                 BroadcastAttemptId::default(),
//                                 AccountId32::new(OUR_ACCOUNT_ID_BYTES),
//                                 UnsignedTransaction::default(),
//                             ),
//                         ),
//                         vec![H256::default()],
//                     ),
//                     (
//                         // do NOT sign this one
//                         Phase::ApplyExtrinsic(1),
//                         state_chain_runtime::Event::EthereumBroadcaster(
//                             pallet_cf_broadcast::Event::TransactionSigningRequest(
//                                 BroadcastAttemptId::default(),
//                                 AccountId32::new([1; 32]),
//                                 UnsignedTransaction::default(),
//                             ),
//                         ),
//                         vec![H256::default()],
//                     ),
//                 ],
//                 block_header_hash,
//             )])
//         });

//     let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(
//         mock_state_chain_rpc_client,
//     ));

//     let logger = new_test_logger();

//     let eth_broadcaster = EthBroadcaster::new_test(eth_rpc_mock, &logger);

//     let multisig_client = Arc::new(MockMultisigClientApi::new());

//     let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     let (epoch_start_sender, mut epoch_start_receiver) = broadcast::channel(10);

//     let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

//     let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     sc_observer::start(
//         state_chain_client,
//         sc_block_stream,
//         eth_broadcaster,
//         multisig_client,
//         account_peer_mapping_change_sender,
//         epoch_start_sender,
//         eth_monitor_ingress_sender,
//         cfe_settings_update_sender,
//         initial_block_hash,
//         logger,
//     )
//     .await
//     .unwrap_err();

//     for epoch_start in expected_epoch_starts {
//         assert_eq!(epoch_start_receiver.recv().await.unwrap(), epoch_start);
//     }

//     assert!(epoch_start_receiver.recv().await.is_err());
// }

// #[tokio::test]
// #[ignore = "runs forever, useful for testing without having to start the whole CFE"]
// async fn run_the_sc_observer() {
//     let settings = Settings::new_test().unwrap();
//     let logger = new_test_logger();

//     let (initial_block_hash, block_stream, state_chain_client) =
//         crate::state_chain_observer::client::connect_to_state_chain(
//             &settings.state_chain,
//             false,
//             &logger,
//         )
//         .await
//         .unwrap();

//     let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     let eth_ws_rpc_client = EthWsRpcClient::new(&settings.eth, &logger).await.unwrap();
//     let eth_broadcaster =
//         EthBroadcaster::new(&settings.eth, eth_ws_rpc_client.clone(), &logger).unwrap();

//     let multisig_client = Arc::new(MockMultisigClientApi::new());

//     let (epoch_start_sender, _) = broadcast::channel(10);

//     let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

//     let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
//         tokio::sync::mpsc::unbounded_channel();

//     sc_observer::start(
//         state_chain_client,
//         block_stream,
//         eth_broadcaster,
//         multisig_client,
//         account_peer_mapping_change_sender,
//         epoch_start_sender,
//         eth_monitor_ingress_sender,
//         cfe_settings_update_sender,
//         initial_block_hash,
//         logger,
//     )
//     .await
//     .unwrap_err();
// }

// // Test that the ceremony requests are calling the correct MultisigClientApi functions
// // depending on whether we are participating in the ceremony or not.
// #[tokio::test]
// async fn should_handle_signing_request() {
//     let logger = new_test_logger();
//     let first_ceremony_id = 1;
//     let key_id = crate::multisig::KeyId(vec![0u8; 32]);
//     let sign_data = crate::multisig::MessageHash([0u8; 32]);
//     let our_account_id = AccountId32::new(OUR_ACCOUNT_ID_BYTES);
//     let not_our_account_id = AccountId32::new([1u8; 32]);
//     assert_ne!(our_account_id, not_our_account_id);

//     let mut rpc = MockStateChainRpcApi::new();
//     // Reporting signing outcome
//     rpc.expect_submit_extrinsic_rpc()
//         .times(1)
//         .returning(move |_| Ok(H256::default()));
//     let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(rpc));
//     let mut multisig_client = MockMultisigClientApi::new();

//     multisig_client
//         .expect_update_latest_ceremony_id()
//         .with(predicate::eq(first_ceremony_id))
//         .returning(|_| ());

//     let next_ceremony_id = first_ceremony_id + 1;
//     multisig_client
//         .expect_sign()
//         .with(
//             predicate::eq(next_ceremony_id),
//             predicate::eq(key_id.clone()),
//             predicate::eq(BTreeSet::from_iter([our_account_id.clone()])),
//             predicate::eq(sign_data.clone()),
//         )
//         .returning(|_, _, _, _| {
//             Err((
//                 BTreeSet::new(),
//                 CeremonyFailureReason::ExpiredBeforeBeingAuthorized,
//             ))
//         });

//     let multisig_client = Arc::new(multisig_client);

//     with_task_scope(|scope| {
//         async {
//             // Handle a signing request that we are not participating in
//             sc_observer::handle_signing_request(
//                 scope,
//                 multisig_client.clone(),
//                 state_chain_client.clone(),
//                 first_ceremony_id,
//                 key_id.clone(),
//                 BTreeSet::from_iter([not_our_account_id.clone()]),
//                 sign_data.clone(),
//                 logger.clone(),
//             )
//             .await;

//             // Handle a signing request that we are participating in
//             sc_observer::handle_signing_request(
//                 scope,
//                 multisig_client,
//                 state_chain_client.clone(),
//                 next_ceremony_id,
//                 key_id,
//                 BTreeSet::from_iter([our_account_id]),
//                 sign_data,
//                 logger,
//             )
//             .await;

//             Ok(())
//         }
//         .boxed()
//     })
//     .await
//     .unwrap();
// }

// #[tokio::test]
// async fn should_handle_keygen_request() {
//     let logger = new_test_logger();
//     let first_ceremony_id = 1;
//     let our_account_id = AccountId32::new(OUR_ACCOUNT_ID_BYTES);
//     let not_our_account_id = AccountId32::new([1u8; 32]);
//     assert_ne!(our_account_id, not_our_account_id);

//     let mut rpc = MockStateChainRpcApi::new();
//     // Submitting keygen outcome
//     rpc.expect_submit_extrinsic_rpc()
//         .times(1)
//         .returning(move |_| Ok(H256::default()));
//     let state_chain_client = Arc::new(StateChainClient::create_test_sc_client(rpc));
//     let mut multisig_client = MockMultisigClientApi::new();

//     multisig_client
//         .expect_update_latest_ceremony_id()
//         .with(predicate::eq(first_ceremony_id))
//         .returning(|_| ());

//     let next_ceremony_id = first_ceremony_id + 1;
//     // Set up the mock api to expect the keygen and sign calls for the ceremonies we are participating in.
//     // It doesn't matter what failure reasons they return.
//     multisig_client
//         .expect_keygen()
//         .with(
//             predicate::eq(next_ceremony_id),
//             predicate::eq(BTreeSet::from_iter([our_account_id.clone()])),
//         )
//         .returning(|_, _| {
//             Err((
//                 BTreeSet::new(),
//                 CeremonyFailureReason::ExpiredBeforeBeingAuthorized,
//             ))
//         });

//     let multisig_client = Arc::new(multisig_client);

//     with_task_scope(|scope| {
//         async {
//             // Handle a keygen request that we are not participating in
//             sc_observer::handle_keygen_request(
//                 scope,
//                 multisig_client.clone(),
//                 state_chain_client.clone(),
//                 first_ceremony_id,
//                 BTreeSet::from_iter([not_our_account_id.clone()]),
//                 logger.clone(),
//             )
//             .await;

//             // Handle a keygen request that we are participating in
//             sc_observer::handle_keygen_request(
//                 scope,
//                 multisig_client.clone(),
//                 state_chain_client.clone(),
//                 next_ceremony_id,
//                 BTreeSet::from_iter([our_account_id]),
//                 logger.clone(),
//             )
//             .await;
//             Ok(())
//         }
//         .boxed()
//     })
//     .await
//     .unwrap();
// }
