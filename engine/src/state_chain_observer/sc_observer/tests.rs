use std::{collections::BTreeSet, sync::Arc};

use crate::{btc::rpc::MockBtcRpcApi, multisig::SignatureToThresholdSignature};
use cf_chains::{
	btc::BitcoinNetwork,
	eth::{Ethereum, Transaction},
	ChainCrypto,
};
use cf_primitives::{AccountRole, KeyId, PolkadotAccountId, GENESIS_EPOCH};
use frame_system::Phase;
use futures::{FutureExt, StreamExt};
use mockall::predicate::{self, eq};
use pallet_cf_broadcast::BroadcastAttemptId;
use pallet_cf_vaults::Vault;

use sp_core::{Hasher, H256};
use sp_runtime::{traits::Keccak256, AccountId32, Digest};
use state_chain_runtime::{AccountId, CfeSettings, EthereumInstance, Header};
use tokio::sync::watch;
use web3::types::{Bytes, SignedTransaction};

use crate::{
	btc::BtcBroadcaster,
	dot::{rpc::MockDotRpcApi, DotBroadcaster},
	eth::{
		rpc::{EthWsRpcClient, MockEthRpcApi},
		EthBroadcaster,
	},
	multisig::{
		client::{KeygenFailureReason, MockMultisigClientApi, SigningFailureReason},
		eth::EthSigning,
		CryptoScheme,
	},
	settings::Settings,
	state_chain_observer::{client::mocks::MockStateChainClient, sc_observer},
	task_scope::task_scope,
	witnesser::EpochStart,
};

use super::EthAddressToMonitorSender;

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
async fn starts_witnessing_when_current_authority() {
	let initial_epoch = 3;
	let initial_epoch_from_block_eth = 30;
	let initial_block_hash = H256::default();
	let account_id = AccountId::new([0; 32]);

	let mut state_chain_client = MockStateChainClient::new();

	state_chain_client.expect_account_id().return_once({
		let account_id = account_id.clone();
		|| account_id
	});

	state_chain_client.
expect_storage_map_entry::<pallet_cf_validator::HistoricalActiveEpochs<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash), eq(account_id))
		.once()
		.return_once(move |_, _| Ok(vec![initial_epoch]));
	state_chain_client
		.expect_storage_value::<pallet_cf_validator::CurrentEpoch<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(move |_| Ok(initial_epoch));
	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::EthereumInstance,
		>>()
		.with(eq(initial_block_hash), eq(initial_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault {
				public_key: Default::default(),
				active_from_block: initial_epoch_from_block_eth,
			}))
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::PolkadotInstance,
		>>()
		.with(eq(initial_block_hash), eq(initial_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: 80 }))
		});

	state_chain_client
		.expect_storage_value::<pallet_cf_environment::BitcoinNetworkSelection<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(move |_| Ok(BitcoinNetwork::Testnet));

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::BitcoinInstance,
		>>()
		.with(eq(initial_block_hash), eq(initial_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: 98 }))
		});

	state_chain_client
			.expect_storage_value::<pallet_cf_environment::PolkadotVaultAccountId<
				state_chain_runtime::Runtime,
			>>()
			.with(eq(initial_block_hash))
			.once()
			.return_once(|_| Ok(Some(PolkadotAccountId::from([3u8; 32]))));

	// No blocks in the stream
	let sc_block_stream = tokio_stream::iter(vec![]);

	let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (epoch_start_sender, epoch_start_receiver) = async_broadcast::broadcast(10);

	let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

	let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_flip_ingress_sender, _eth_monitor_flip_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_usdc_ingress_sender, _eth_monitor_usdc_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (dot_epoch_start_sender, _dot_epoch_start_receiver_1) = async_broadcast::broadcast(10);

	let (dot_monitor_ingress_sender, _dot_monitor_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (dot_monitor_signature_sender, _dot_monitor_signature_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (btc_epoch_start_sender, _btc_epoch_start_receiver_1) = async_broadcast::broadcast(10);

	let (btc_monitor_signature_sender, _btc_monitor_signature_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	sc_observer::start(
		Arc::new(state_chain_client),
		sc_block_stream,
		EthBroadcaster::new_test(MockEthRpcApi::new()),
		DotBroadcaster::new(MockDotRpcApi::new()),
		BtcBroadcaster::new(MockBtcRpcApi::new()),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
		account_peer_mapping_change_sender,
		epoch_start_sender,
		EthAddressToMonitorSender {
			eth: eth_monitor_ingress_sender,
			flip: eth_monitor_flip_ingress_sender,
			usdc: eth_monitor_usdc_ingress_sender,
		},
		dot_epoch_start_sender,
		dot_monitor_ingress_sender,
		dot_monitor_signature_sender,
		btc_epoch_start_sender,
		btc_monitor_signature_sender,
		cfe_settings_update_sender,
		initial_block_hash,
	)
	.await
	.unwrap_err();
	assert_eq!(
		epoch_start_receiver.collect::<Vec<_>>().await,
		vec![EpochStart::<Ethereum> {
			epoch_index: initial_epoch,
			block_number: initial_epoch_from_block_eth,
			current: true,
			participant: true,
			data: ()
		}]
	);
}

#[tokio::test]
async fn starts_witnessing_when_historic_on_startup() {
	let active_epoch = 3;
	let active_epoch_from_block_eth = 30;
	let current_epoch = 4;
	let current_epoch_from_block_eth = 40;

	let current_epoch_from_block_dot = 80;

	let current_epoch_from_block_btc = 100;

	let initial_block_hash = H256::default();
	let account_id = AccountId::new([0; 32]);

	let mut state_chain_client = MockStateChainClient::new();

	state_chain_client.expect_account_id().once().return_once({
		let account_id = account_id.clone();
		|| account_id
	});

	state_chain_client.
expect_storage_map_entry::<pallet_cf_validator::HistoricalActiveEpochs<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash), eq(account_id))
		.once()
		.return_once(move |_, _| Ok(vec![active_epoch]));
	state_chain_client
		.expect_storage_value::<pallet_cf_validator::CurrentEpoch<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(move |_| Ok(current_epoch));
	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::EthereumInstance,
		>>()
		.with(eq(initial_block_hash), eq(active_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault {
				public_key: Default::default(),
				active_from_block: active_epoch_from_block_eth,
			}))
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::PolkadotInstance,
		>>()
		.with(eq(initial_block_hash), eq(active_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault {
				public_key: Default::default(),
				active_from_block: current_epoch_from_block_dot,
			}))
		});

	state_chain_client
		.expect_storage_value::<pallet_cf_environment::PolkadotVaultAccountId<
			state_chain_runtime::Runtime,
		>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(|_| Ok(Some(PolkadotAccountId::from([3u8; 32]))));

	state_chain_client
		.expect_storage_value::<pallet_cf_environment::BitcoinNetworkSelection<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(move |_| Ok(BitcoinNetwork::Testnet));

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::BitcoinInstance,
		>>()
		.with(eq(initial_block_hash), eq(active_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: 98 }))
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::EthereumInstance,
		>>()
		.with(eq(initial_block_hash), eq(current_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault {
				public_key: Default::default(),
				active_from_block: current_epoch_from_block_eth,
			}))
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::PolkadotInstance,
		>>()
		.with(eq(initial_block_hash), eq(current_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault {
				public_key: Default::default(),
				active_from_block: current_epoch_from_block_dot,
			}))
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::BitcoinInstance,
		>>()
		.with(eq(initial_block_hash), eq(current_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: current_epoch_from_block_btc }))
		});

	state_chain_client
		.expect_storage_value::<pallet_cf_environment::PolkadotVaultAccountId<
			state_chain_runtime::Runtime,
		>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(|_| Ok(Some(PolkadotAccountId::from([3u8; 32]))));

	// No blocks in the stream
	let sc_block_stream = tokio_stream::iter(vec![]);

	let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (epoch_start_sender, epoch_start_receiver) = async_broadcast::broadcast(10);

	let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

	let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_flip_ingress_sender, _eth_monitor_flip_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_usdc_ingress_sender, _eth_monitor_usdc_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (dot_epoch_start_sender, _dot_epoch_start_receiver_1) = async_broadcast::broadcast(10);

	let (dot_monitor_ingress_sender, _dot_monitor_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (dot_monitor_signature_sender, _dot_monitor_signature_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (btc_epoch_start_sender, _btc_epoch_start_receiver_1) = async_broadcast::broadcast(10);

	let (btc_monitor_signature_sender, _btc_monitor_signature_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	sc_observer::start(
		Arc::new(state_chain_client),
		sc_block_stream,
		EthBroadcaster::new_test(MockEthRpcApi::new()),
		DotBroadcaster::new(MockDotRpcApi::new()),
		BtcBroadcaster::new(MockBtcRpcApi::new()),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
		account_peer_mapping_change_sender,
		epoch_start_sender,
		EthAddressToMonitorSender {
			eth: eth_monitor_ingress_sender,
			flip: eth_monitor_flip_ingress_sender,
			usdc: eth_monitor_usdc_ingress_sender,
		},
		dot_epoch_start_sender,
		dot_monitor_ingress_sender,
		dot_monitor_signature_sender,
		btc_epoch_start_sender,
		btc_monitor_signature_sender,
		cfe_settings_update_sender,
		initial_block_hash,
	)
	.await
	.unwrap_err();

	assert_eq!(
		epoch_start_receiver.collect::<Vec<_>>().await,
		vec![
			EpochStart::<Ethereum> {
				epoch_index: active_epoch,
				block_number: active_epoch_from_block_eth,
				current: false,
				participant: true,
				data: ()
			},
			EpochStart::<Ethereum> {
				epoch_index: current_epoch,
				block_number: current_epoch_from_block_eth,
				current: true,
				participant: false,
				data: ()
			}
		]
	);
}

#[tokio::test]
async fn does_not_start_witnessing_when_not_historic_or_current_authority() {
	let initial_epoch = 3;
	let initial_epoch_from_block_eth = 30;
	let initial_block_hash = H256::default();
	let account_id = AccountId::new([0; 32]);

	let mut state_chain_client = MockStateChainClient::new();

	state_chain_client.expect_account_id().return_once({
		let account_id = account_id.clone();
		|| account_id
	});

	state_chain_client.expect_storage_map_entry::<pallet_cf_validator::HistoricalActiveEpochs<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash), eq(account_id))
		.once()
		.return_once(move |_, _| Ok(vec![]));
	state_chain_client
		.expect_storage_value::<pallet_cf_validator::CurrentEpoch<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(move |_| Ok(initial_epoch));
	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::EthereumInstance,
		>>()
		.with(eq(initial_block_hash), eq(3))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault {
				public_key: Default::default(),
				active_from_block: initial_epoch_from_block_eth,
			}))
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::PolkadotInstance,
		>>()
		.with(eq(initial_block_hash), eq(3))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: 80 }))
		});

	state_chain_client
		.expect_storage_value::<pallet_cf_environment::PolkadotVaultAccountId<
			state_chain_runtime::Runtime,
		>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(|_| Ok(Some(PolkadotAccountId::from([3u8; 32]))));

	state_chain_client
		.expect_storage_value::<pallet_cf_environment::BitcoinNetworkSelection<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(move |_| Ok(BitcoinNetwork::Testnet));

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::BitcoinInstance,
		>>()
		.with(eq(initial_block_hash), eq(initial_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: 98 }))
		});

	let sc_block_stream = tokio_stream::iter(vec![]);

	let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (epoch_start_sender, epoch_start_receiver) = async_broadcast::broadcast(10);
	let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

	let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_flip_ingress_sender, _eth_monitor_flip_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_usdc_ingress_sender, _eth_monitor_usdc_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (dot_epoch_start_sender, _dot_epoch_start_receiver_1) = async_broadcast::broadcast(10);

	let (dot_monitor_ingress_sender, _dot_monitor_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (dot_monitor_signature_sender, _dot_monitor_signature_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (btc_epoch_start_sender, _btc_epoch_start_receiver_1) = async_broadcast::broadcast(10);

	let (btc_monitor_signature_sender, _btc_monitor_signature_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	sc_observer::start(
		Arc::new(state_chain_client),
		sc_block_stream,
		EthBroadcaster::new_test(MockEthRpcApi::new()),
		DotBroadcaster::new(MockDotRpcApi::new()),
		BtcBroadcaster::new(MockBtcRpcApi::new()),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
		account_peer_mapping_change_sender,
		epoch_start_sender,
		EthAddressToMonitorSender {
			eth: eth_monitor_ingress_sender,
			flip: eth_monitor_flip_ingress_sender,
			usdc: eth_monitor_usdc_ingress_sender,
		},
		dot_epoch_start_sender,
		dot_monitor_ingress_sender,
		dot_monitor_signature_sender,
		btc_epoch_start_sender,
		btc_monitor_signature_sender,
		cfe_settings_update_sender,
		initial_block_hash,
	)
	.await
	.unwrap_err();

	assert_eq!(
		epoch_start_receiver.collect::<Vec<_>>().await,
		vec![EpochStart::<Ethereum> {
			epoch_index: initial_epoch,
			block_number: initial_epoch_from_block_eth,
			current: true,
			participant: false,
			data: (),
		}]
	);
}

#[tokio::test]
async fn current_authority_to_current_authority_on_new_epoch_event() {
	let initial_epoch = 4;
	let initial_epoch_from_block_eth = 40;

	let initial_epoch_from_block_dot = 72;

	let initial_epoch_from_block_btc = 98;

	let new_epoch = 5;
	let new_epoch_from_block = 50;
	let initial_block_hash = H256::default();
	let account_id = AccountId::new([0; 32]);

	let mut state_chain_client = MockStateChainClient::new();

	state_chain_client.expect_account_id().return_once({
		let account_id = account_id.clone();
		|| account_id
	});

	state_chain_client.
expect_storage_map_entry::<pallet_cf_validator::HistoricalActiveEpochs<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash), eq(account_id.clone()))
		.once()
		.return_once(move |_, _| Ok(vec![initial_epoch]));
	state_chain_client
		.expect_storage_value::<pallet_cf_validator::CurrentEpoch<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(move |_| Ok(initial_epoch));
	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::EthereumInstance,
		>>()
		.with(eq(initial_block_hash), eq(initial_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault {
				public_key: Default::default(),
				active_from_block: initial_epoch_from_block_eth,
			}))
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::PolkadotInstance,
		>>()
		.with(eq(initial_block_hash), eq(initial_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault {
				public_key: Default::default(),
				active_from_block: initial_epoch_from_block_dot,
			}))
		});

	state_chain_client
		.expect_storage_value::<pallet_cf_environment::PolkadotVaultAccountId<
			state_chain_runtime::Runtime,
		>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(|_| Ok(Some(PolkadotAccountId::from([3u8; 32]))));

	state_chain_client
		.expect_storage_value::<pallet_cf_environment::BitcoinNetworkSelection<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(move |_| Ok(BitcoinNetwork::Testnet));

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::BitcoinInstance,
		>>()
		.with(eq(initial_block_hash), eq(initial_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: initial_epoch_from_block_btc }))
		});

	let empty_block_header = test_header(20);
	let new_epoch_block_header = test_header(21);
	let new_epoch_block_header_hash = new_epoch_block_header.hash();
	let sc_block_stream =
		tokio_stream::iter(vec![empty_block_header.clone(), new_epoch_block_header.clone()]);
	state_chain_client
		.expect_storage_value::<frame_system::Events<state_chain_runtime::Runtime>>()
		.with(eq(empty_block_header.hash()))
		.once()
		.return_once(move |_| Ok(vec![]));
	state_chain_client
		.expect_storage_value::<frame_system::Events<state_chain_runtime::Runtime>>()
		.with(eq(new_epoch_block_header_hash))
		.once()
		.return_once(move |_| {
			Ok(vec![Box::new(frame_system::EventRecord {
				phase: Phase::ApplyExtrinsic(0),
				event: state_chain_runtime::RuntimeEvent::Validator(
					pallet_cf_validator::Event::NewEpoch(new_epoch),
				),
				topics: vec![H256::default()],
			})])
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::EthereumInstance,
		>>()
		.with(eq(new_epoch_block_header_hash), eq(new_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault {
				public_key: Default::default(),
				active_from_block: new_epoch_from_block,
			}))
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::PolkadotInstance,
		>>()
		.with(eq(new_epoch_block_header_hash), eq(new_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault {
				public_key: Default::default(),
				active_from_block: initial_epoch_from_block_dot,
			}))
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::BitcoinInstance,
		>>()
		.with(eq(new_epoch_block_header_hash), eq(new_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: initial_epoch_from_block_btc }))
		});

	state_chain_client
	.expect_storage_value::<pallet_cf_environment::PolkadotVaultAccountId<
		state_chain_runtime::Runtime,
	>>()
	.with(eq(new_epoch_block_header_hash))
	.once()
	.return_once(|_| Ok(Some(PolkadotAccountId::from([3u8; 32]))));

	state_chain_client.expect_storage_double_map_entry::<pallet_cf_validator::AuthorityIndex<state_chain_runtime::Runtime>>()
		.with(eq(new_epoch_block_header_hash), eq(5), eq(account_id.clone()))
		.once()
		.return_once(move |_, _, _| Ok(Some(1)));

	let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (epoch_start_sender, epoch_start_receiver) = async_broadcast::broadcast(10);

	let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

	let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_flip_ingress_sender, _eth_monitor_flip_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_usdc_ingress_sender, _eth_monitor_usdc_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (dot_epoch_start_sender, _dot_epoch_start_receiver_1) = async_broadcast::broadcast(10);

	let (dot_monitor_ingress_sender, _dot_monitor_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (dot_monitor_signature_sender, _dot_monitor_signature_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (btc_epoch_start_sender, _btc_epoch_start_receiver_1) = async_broadcast::broadcast(10);

	let (btc_monitor_signature_sender, _btc_monitor_signature_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	sc_observer::start(
		Arc::new(state_chain_client),
		sc_block_stream,
		EthBroadcaster::new_test(MockEthRpcApi::new()),
		DotBroadcaster::new(MockDotRpcApi::new()),
		BtcBroadcaster::new(MockBtcRpcApi::new()),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
		account_peer_mapping_change_sender,
		epoch_start_sender,
		EthAddressToMonitorSender {
			eth: eth_monitor_ingress_sender,
			flip: eth_monitor_flip_ingress_sender,
			usdc: eth_monitor_usdc_ingress_sender,
		},
		dot_epoch_start_sender,
		dot_monitor_ingress_sender,
		dot_monitor_signature_sender,
		btc_epoch_start_sender,
		btc_monitor_signature_sender,
		cfe_settings_update_sender,
		initial_block_hash,
	)
	.await
	.unwrap_err();

	assert_eq!(
		epoch_start_receiver.collect::<Vec<_>>().await,
		vec![
			EpochStart::<Ethereum> {
				epoch_index: initial_epoch,
				block_number: initial_epoch_from_block_eth,
				current: true,
				participant: true,
				data: ()
			},
			EpochStart::<Ethereum> {
				epoch_index: new_epoch,
				block_number: new_epoch_from_block,
				current: true,
				participant: true,
				data: ()
			}
		]
	);
}

#[tokio::test]
async fn not_historical_to_authority_on_new_epoch() {
	let initial_epoch = 3;
	let initial_epoch_from_block_eth = 30;
	let new_epoch = 4;
	let new_epoch_from_block = 40;
	let initial_block_hash = H256::default();
	let account_id = AccountId::new([0; 32]);

	let mut state_chain_client = MockStateChainClient::new();

	state_chain_client.expect_account_id().once().return_once({
		let account_id = account_id.clone();
		|| account_id
	});

	state_chain_client.
expect_storage_map_entry::<pallet_cf_validator::HistoricalActiveEpochs<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash), eq(account_id.clone()))
		.once()
		.return_once(move |_, _| Ok(vec![]));
	state_chain_client
		.expect_storage_value::<pallet_cf_validator::CurrentEpoch<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(move |_| Ok(initial_epoch));
	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::EthereumInstance,
		>>()
		.with(eq(initial_block_hash), eq(initial_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault {
				public_key: Default::default(),
				active_from_block: initial_epoch_from_block_eth,
			}))
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::PolkadotInstance,
		>>()
		.with(eq(initial_block_hash), eq(initial_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: 20 }))
		});

	state_chain_client
		.expect_storage_value::<pallet_cf_environment::PolkadotVaultAccountId<
			state_chain_runtime::Runtime,
		>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(|_| Ok(Some(PolkadotAccountId::from([3u8; 32]))));

	state_chain_client
		.expect_storage_value::<pallet_cf_environment::BitcoinNetworkSelection<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(move |_| Ok(BitcoinNetwork::Testnet));

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::BitcoinInstance,
		>>()
		.with(eq(initial_block_hash), eq(initial_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: 89 }))
		});

	let empty_block_header = test_header(20);
	let new_epoch_block_header = test_header(21);
	let new_epoch_block_header_hash = new_epoch_block_header.hash();
	let sc_block_stream =
		tokio_stream::iter(vec![empty_block_header.clone(), new_epoch_block_header.clone()]);
	state_chain_client
		.expect_storage_value::<frame_system::Events<state_chain_runtime::Runtime>>()
		.with(eq(empty_block_header.hash()))
		.once()
		.return_once(move |_| Ok(vec![]));
	state_chain_client
		.expect_storage_value::<frame_system::Events<state_chain_runtime::Runtime>>()
		.with(eq(new_epoch_block_header_hash))
		.once()
		.return_once(move |_| {
			Ok(vec![Box::new(frame_system::EventRecord {
				phase: Phase::ApplyExtrinsic(0),
				event: state_chain_runtime::RuntimeEvent::Validator(
					pallet_cf_validator::Event::NewEpoch(new_epoch),
				),
				topics: vec![H256::default()],
			})])
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::EthereumInstance,
		>>()
		.with(eq(new_epoch_block_header_hash), eq(new_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault {
				public_key: Default::default(),
				active_from_block: new_epoch_from_block,
			}))
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::PolkadotInstance,
		>>()
		.with(eq(new_epoch_block_header_hash), eq(new_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: 80 }))
		});

	state_chain_client
		.expect_storage_value::<pallet_cf_environment::PolkadotVaultAccountId<
			state_chain_runtime::Runtime,
		>>()
		.with(eq(new_epoch_block_header_hash))
		.once()
		.return_once(|_| Ok(Some(PolkadotAccountId::from([3u8; 32]))));

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::BitcoinInstance,
		>>()
		.with(eq(new_epoch_block_header_hash), eq(new_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: 120 }))
		});

	state_chain_client.expect_storage_double_map_entry::<pallet_cf_validator::AuthorityIndex<state_chain_runtime::Runtime>>()
		.with(eq(new_epoch_block_header_hash), eq(new_epoch), eq(account_id.clone()))
		.once()
		.return_once(move |_, _, _| Ok(Some(1)));

	let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (epoch_start_sender, epoch_start_receiver) = async_broadcast::broadcast(10);

	let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

	let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_flip_ingress_sender, _eth_monitor_flip_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_usdc_ingress_sender, _eth_monitor_usdc_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (dot_epoch_start_sender, _dot_epoch_start_receiver_1) = async_broadcast::broadcast(10);

	let (dot_monitor_ingress_sender, _dot_monitor_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (dot_monitor_signature_sender, _dot_monitor_signature_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (btc_epoch_start_sender, _btc_epoch_start_receiver_1) = async_broadcast::broadcast(10);

	let (btc_monitor_signature_sender, _btc_monitor_signature_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	sc_observer::start(
		Arc::new(state_chain_client),
		sc_block_stream,
		EthBroadcaster::new_test(MockEthRpcApi::new()),
		DotBroadcaster::new(MockDotRpcApi::new()),
		BtcBroadcaster::new(MockBtcRpcApi::new()),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
		account_peer_mapping_change_sender,
		epoch_start_sender,
		EthAddressToMonitorSender {
			eth: eth_monitor_ingress_sender,
			flip: eth_monitor_flip_ingress_sender,
			usdc: eth_monitor_usdc_ingress_sender,
		},
		dot_epoch_start_sender,
		dot_monitor_ingress_sender,
		dot_monitor_signature_sender,
		btc_epoch_start_sender,
		btc_monitor_signature_sender,
		cfe_settings_update_sender,
		initial_block_hash,
	)
	.await
	.unwrap_err();

	assert_eq!(
		epoch_start_receiver.collect::<Vec<_>>().await,
		vec![
			EpochStart::<Ethereum> {
				epoch_index: initial_epoch,
				block_number: initial_epoch_from_block_eth,
				current: true,
				participant: false,
				data: ()
			},
			EpochStart::<Ethereum> {
				epoch_index: new_epoch,
				block_number: new_epoch_from_block,
				current: true,
				participant: true,
				data: ()
			}
		]
	);
}

#[tokio::test]
async fn current_authority_to_historical_on_new_epoch_event() {
	let initial_epoch = 3;
	let initial_epoch_from_block_eth = 30;
	let new_epoch = 4;
	let new_epoch_from_block = 40;
	let initial_block_hash = H256::default();
	let account_id = AccountId::new([0; 32]);

	let mut state_chain_client = MockStateChainClient::new();

	state_chain_client.expect_account_id().once().return_once({
		let account_id = account_id.clone();
		|| account_id
	});

	state_chain_client.
expect_storage_map_entry::<pallet_cf_validator::HistoricalActiveEpochs<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash), eq(account_id.clone()))
		.once()
		.return_once(move |_, _| Ok(vec![initial_epoch]));
	state_chain_client
		.expect_storage_value::<pallet_cf_validator::CurrentEpoch<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(move |_| Ok(initial_epoch));
	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::EthereumInstance,
		>>()
		.with(eq(initial_block_hash), eq(3))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault {
				public_key: Default::default(),
				active_from_block: initial_epoch_from_block_eth,
			}))
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::PolkadotInstance,
		>>()
		.with(eq(initial_block_hash), eq(initial_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: 20 }))
		});

	state_chain_client
		.expect_storage_value::<pallet_cf_environment::PolkadotVaultAccountId<
			state_chain_runtime::Runtime,
		>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(|_| Ok(Some(PolkadotAccountId::from([3u8; 32]))));

	state_chain_client
		.expect_storage_value::<pallet_cf_environment::BitcoinNetworkSelection<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(move |_| Ok(BitcoinNetwork::Testnet));

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::BitcoinInstance,
		>>()
		.with(eq(initial_block_hash), eq(initial_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: 120 }))
		});

	let empty_block_header = test_header(20);
	let new_epoch_block_header = test_header(21);
	let new_epoch_block_header_hash = new_epoch_block_header.hash();
	let sc_block_stream =
		tokio_stream::iter([empty_block_header.clone(), new_epoch_block_header.clone()]);

	state_chain_client
		.expect_storage_value::<frame_system::Events<state_chain_runtime::Runtime>>()
		.with(eq(empty_block_header.hash()))
		.once()
		.return_once(move |_| Ok(vec![]));
	state_chain_client
		.expect_storage_value::<frame_system::Events<state_chain_runtime::Runtime>>()
		.with(eq(new_epoch_block_header_hash))
		.once()
		.return_once(move |_| {
			Ok(vec![Box::new(frame_system::EventRecord {
				phase: Phase::ApplyExtrinsic(0),
				event: state_chain_runtime::RuntimeEvent::Validator(
					pallet_cf_validator::Event::NewEpoch(new_epoch),
				),
				topics: vec![H256::default()],
			})])
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::EthereumInstance,
		>>()
		.with(eq(new_epoch_block_header_hash), eq(new_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault {
				public_key: Default::default(),
				active_from_block: new_epoch_from_block,
			}))
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::PolkadotInstance,
		>>()
		.with(eq(new_epoch_block_header_hash), eq(new_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: 80 }))
		});

	state_chain_client
			.expect_storage_value::<pallet_cf_environment::PolkadotVaultAccountId<
				state_chain_runtime::Runtime,
			>>()
			.with(eq(new_epoch_block_header_hash))
			.once()
			.return_once(|_| Ok(Some(PolkadotAccountId::from([3u8; 32]))));

	state_chain_client
			.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
				state_chain_runtime::Runtime,
				state_chain_runtime::BitcoinInstance,
			>>()
			.with(eq(new_epoch_block_header_hash), eq(new_epoch))
			.once()
			.return_once(move |_, _| {
				Ok(Some(Vault { public_key: Default::default(), active_from_block: 120 }))
			});

	state_chain_client.expect_storage_double_map_entry::<pallet_cf_validator::AuthorityIndex<state_chain_runtime::Runtime>>()
		.with(eq(new_epoch_block_header_hash), eq(4), eq(account_id.clone()))
		.once()
		.return_once(move |_, _, _| Ok(None));

	let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (epoch_start_sender, epoch_start_receiver) = async_broadcast::broadcast(10);

	let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

	let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_flip_ingress_sender, _eth_monitor_flip_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_usdc_ingress_sender, _eth_monitor_usdc_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (dot_epoch_start_sender, _dot_epoch_start_receiver_1) = async_broadcast::broadcast(10);

	let (dot_monitor_ingress_sender, _dot_monitor_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (dot_monitor_signature_sender, _dot_monitor_signature_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (btc_epoch_start_sender, _btc_epoch_start_receiver_1) = async_broadcast::broadcast(10);

	let (btc_monitor_signature_sender, _btc_monitor_signature_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	sc_observer::start(
		Arc::new(state_chain_client),
		sc_block_stream,
		EthBroadcaster::new_test(MockEthRpcApi::new()),
		DotBroadcaster::new(MockDotRpcApi::new()),
		BtcBroadcaster::new(MockBtcRpcApi::new()),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
		account_peer_mapping_change_sender,
		epoch_start_sender,
		EthAddressToMonitorSender {
			eth: eth_monitor_ingress_sender,
			flip: eth_monitor_flip_ingress_sender,
			usdc: eth_monitor_usdc_ingress_sender,
		},
		dot_epoch_start_sender,
		dot_monitor_ingress_sender,
		dot_monitor_signature_sender,
		btc_epoch_start_sender,
		btc_monitor_signature_sender,
		cfe_settings_update_sender,
		initial_block_hash,
	)
	.await
	.unwrap_err();

	assert_eq!(
		epoch_start_receiver.collect::<Vec<_>>().await,
		vec![
			EpochStart::<Ethereum> {
				epoch_index: initial_epoch,
				block_number: initial_epoch_from_block_eth,
				current: true,
				participant: true,
				data: ()
			},
			EpochStart::<Ethereum> {
				epoch_index: new_epoch,
				block_number: new_epoch_from_block,
				current: true,
				participant: false,
				data: ()
			}
		]
	);
}

// TODO: We should test that this works for historical epochs too. We should be able to sign for
// historical epochs we were a part of
#[tokio::test]
async fn only_encodes_and_signs_when_specified() {
	let initial_block_hash = H256::default();
	let account_id = AccountId::new([0; 32]);

	let mut state_chain_client = MockStateChainClient::new();

	state_chain_client.expect_account_id().once().return_once({
		let account_id = account_id.clone();
		|| account_id
	});

	let initial_epoch = 3;
	let initial_epoch_from_block_eth = 30;

	state_chain_client.expect_storage_map_entry::<pallet_cf_validator::HistoricalActiveEpochs<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash), eq(account_id.clone()))
		.once()
		.return_once(move |_, _| Ok(vec![initial_epoch]));
	state_chain_client
		.expect_storage_value::<pallet_cf_validator::CurrentEpoch<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(move |_| Ok(initial_epoch));
	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::EthereumInstance,
		>>()
		.with(eq(initial_block_hash), eq(initial_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault {
				public_key: Default::default(),
				active_from_block: initial_epoch_from_block_eth,
			}))
		});

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::PolkadotInstance,
		>>()
		.with(eq(initial_block_hash), eq(initial_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: 80 }))
		});

	state_chain_client
		.expect_storage_value::<pallet_cf_environment::PolkadotVaultAccountId<
			state_chain_runtime::Runtime,
		>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(|_| Ok(Some(PolkadotAccountId::from([3u8; 32]))));

	state_chain_client
		.expect_storage_value::<pallet_cf_environment::BitcoinNetworkSelection<state_chain_runtime::Runtime>>()
		.with(eq(initial_block_hash))
		.once()
		.return_once(move |_| Ok(BitcoinNetwork::Testnet));

	state_chain_client
		.expect_storage_map_entry::<pallet_cf_vaults::Vaults<
			state_chain_runtime::Runtime,
			state_chain_runtime::BitcoinInstance,
		>>()
		.with(eq(initial_block_hash), eq(initial_epoch))
		.once()
		.return_once(move |_, _| {
			Ok(Some(Vault { public_key: Default::default(), active_from_block: 98 }))
		});

	let block_header = test_header(21);
	let sc_block_stream = tokio_stream::iter([block_header.clone()]);

	let mut eth_rpc_mock = MockEthRpcApi::new();

	// when we are selected to sign we must estimate gas and sign
	// NB: We only do this once, since we are only selected to sign once
	eth_rpc_mock
		.expect_estimate_gas()
		.once()
		.returning(|_| Ok(web3::types::U256::from(100_000)));

	eth_rpc_mock.expect_sign_transaction().once().return_once(|_, _| {
		// just a nothing signed transaction
		Ok(SignedTransaction {
			message_hash: web3::types::H256::default(),
			v: 1,
			r: web3::types::H256::default(),
			s: web3::types::H256::default(),
			raw_transaction: Bytes(Vec::new()),
			transaction_hash: web3::types::H256::default(),
		})
	});

	eth_rpc_mock
		.expect_send_raw_transaction()
		.once()
		.return_once(|tx| Ok(Keccak256::hash(&tx.0[..]).0.into()));

	state_chain_client
		.expect_storage_value::<frame_system::Events<state_chain_runtime::Runtime>>()
		.with(eq(block_header.hash()))
		.once()
		.return_once(move |_| {
			Ok(vec![
				Box::new(frame_system::EventRecord {
					phase: Phase::ApplyExtrinsic(0),
					event: state_chain_runtime::RuntimeEvent::EthereumBroadcaster(
						pallet_cf_broadcast::Event::TransactionBroadcastRequest {
							broadcast_attempt_id: BroadcastAttemptId::default(),
							nominee: account_id,
							transaction_payload: Transaction::default(),
						},
					),
					topics: vec![H256::default()],
				}),
				Box::new(frame_system::EventRecord {
					phase: Phase::ApplyExtrinsic(1),
					event: state_chain_runtime::RuntimeEvent::EthereumBroadcaster(
						pallet_cf_broadcast::Event::TransactionBroadcastRequest {
							broadcast_attempt_id: BroadcastAttemptId::default(),
							nominee: AccountId32::new([1; 32]), // NOT OUR ACCOUNT ID
							transaction_payload: Transaction::default(),
						},
					),
					topics: vec![H256::default()],
				}),
			])
		});

	let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (epoch_start_sender, _epoch_start_receiver) = async_broadcast::broadcast(10);

	let (cfe_settings_update_sender, _) = watch::channel::<CfeSettings>(CfeSettings::default());

	let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_flip_ingress_sender, _eth_monitor_flip_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (eth_monitor_usdc_ingress_sender, _eth_monitor_usdc_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (dot_epoch_start_sender, _dot_epoch_start_receiver_1) = async_broadcast::broadcast(10);

	let (dot_monitor_ingress_sender, _dot_monitor_ingress_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (dot_monitor_signature_sender, _dot_monitor_signature_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (btc_monitor_signature_sender, _btc_monitor_signature_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (btc_epoch_start_sender, _btc_epoch_start_receiver_1) = async_broadcast::broadcast(10);

	sc_observer::start(
		Arc::new(state_chain_client),
		sc_block_stream,
		EthBroadcaster::new_test(eth_rpc_mock),
		DotBroadcaster::new(MockDotRpcApi::new()),
		BtcBroadcaster::new(MockBtcRpcApi::new()),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
		MockMultisigClientApi::new(),
		account_peer_mapping_change_sender,
		epoch_start_sender,
		EthAddressToMonitorSender {
			eth: eth_monitor_ingress_sender,
			flip: eth_monitor_flip_ingress_sender,
			usdc: eth_monitor_usdc_ingress_sender,
		},
		dot_epoch_start_sender,
		dot_monitor_ingress_sender,
		dot_monitor_signature_sender,
		btc_epoch_start_sender,
		btc_monitor_signature_sender,
		cfe_settings_update_sender,
		initial_block_hash,
	)
	.await
	.unwrap_err();
}

// TODO: Test that when we return None for polkadot vault
// witnessing isn't started for dot, but is started for ETH

async fn should_handle_signing_request<C, I>()
where
	C: CryptoScheme + Send + Sync,
	I: 'static + Send + Sync,
	state_chain_runtime::Runtime: pallet_cf_threshold_signature::Config<I>,
	state_chain_runtime::RuntimeCall:
		std::convert::From<pallet_cf_threshold_signature::Call<state_chain_runtime::Runtime, I>>,
	<<state_chain_runtime::Runtime as pallet_cf_threshold_signature::Config<I>>::TargetChain as ChainCrypto>::ThresholdSignature: std::convert::From<<C as CryptoScheme>::Signature>,
	Vec<C::Signature>: SignatureToThresholdSignature<
		<state_chain_runtime::Runtime as pallet_cf_threshold_signature::Config<I>>::TargetChain
	>,
{
	let first_ceremony_id = 1;
	let key_id = KeyId { epoch_index: 1, public_key_bytes: vec![0u8; 32] };
	let payload = C::signing_payload_for_test();
	let our_account_id = AccountId32::new([0; 32]);
	let not_our_account_id = AccountId32::new([1u8; 32]);
	assert_ne!(our_account_id, not_our_account_id);

	let mut state_chain_client = MockStateChainClient::new();
	state_chain_client
		.expect_account_id()
		.times(2)
		.return_const(our_account_id.clone());
	state_chain_client.
expect_submit_signed_extrinsic::<pallet_cf_threshold_signature::Call<state_chain_runtime::Runtime,
I>>() 		.once()
		.return_once(|_| Ok(sp_core::H256::default()));
	let state_chain_client = Arc::new(state_chain_client);

	let mut multisig_client = MockMultisigClientApi::<C>::new();
	multisig_client
		.expect_update_latest_ceremony_id()
		.with(predicate::eq(first_ceremony_id))
		.once()
		.returning(|_| ());

	let next_ceremony_id = first_ceremony_id + 1;
	multisig_client
		.expect_initiate_signing()
		.with(
			predicate::eq(next_ceremony_id),
			predicate::eq(key_id.clone()),
			predicate::eq(BTreeSet::from_iter([our_account_id.clone()])),
			predicate::eq(vec![payload.clone()]),
		)
		.once()
		.return_once(|_, _, _, _| {
			futures::future::ready(Err((
				BTreeSet::new(),
				SigningFailureReason::InvalidParticipants,
			)))
			.boxed()
		});

	task_scope(|scope| {
		async {
			// Handle a signing request that we are not participating in
			sc_observer::handle_signing_request::<_, _, C, I>(
				scope,
				&multisig_client,
				state_chain_client.clone(),
				first_ceremony_id,
				key_id.clone(),
				BTreeSet::from_iter([not_our_account_id.clone()]),
				vec![payload.clone()],
			)
			.await;

			// Handle a signing request that we are participating in
			sc_observer::handle_signing_request::<_, _, C, I>(
				scope,
				&multisig_client,
				state_chain_client.clone(),
				next_ceremony_id,
				key_id,
				BTreeSet::from_iter([our_account_id]),
				vec![payload],
			)
			.await;

			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap();
}

// Test that the ceremony requests are calling the correct MultisigClientApi functions
// depending on whether we are participating in the ceremony or not.
#[tokio::test]
async fn should_handle_signing_request_eth() {
	should_handle_signing_request::<EthSigning, EthereumInstance>().await;
}

mod dot_signing {

	use crate::multisig::polkadot::PolkadotSigning;

	use super::*;
	use state_chain_runtime::PolkadotInstance;

	#[tokio::test]
	async fn should_handle_signing_request_dot() {
		should_handle_signing_request::<PolkadotSigning, PolkadotInstance>().await;
	}
}

async fn should_handle_keygen_request<C, I>()
where
	C: CryptoScheme<AggKey = <<state_chain_runtime::Runtime as pallet_cf_vaults::Config<I>>::Chain as ChainCrypto>::AggKey> + Send + Sync,
	I: 'static + Send + Sync,
	state_chain_runtime::Runtime: pallet_cf_vaults::Config<I>,
	state_chain_runtime::RuntimeCall:
		std::convert::From<pallet_cf_vaults::Call<state_chain_runtime::Runtime, I>>,
{
	let first_ceremony_id = 1;
	let our_account_id = AccountId32::new([0; 32]);
	let not_our_account_id = AccountId32::new([1u8; 32]);
	assert_ne!(our_account_id, not_our_account_id);

	let mut state_chain_client = MockStateChainClient::new();
	state_chain_client
		.expect_account_id()
		.times(2)
		.return_const(our_account_id.clone());
	state_chain_client
		.expect_submit_signed_extrinsic::<pallet_cf_vaults::Call<state_chain_runtime::Runtime, I>>()
		.once()
		.return_once(|_| Ok(H256::default()));
	let state_chain_client = Arc::new(state_chain_client);

	let mut multisig_client = MockMultisigClientApi::<C>::new();
	multisig_client
		.expect_update_latest_ceremony_id()
		.with(predicate::eq(first_ceremony_id))
		.once()
		.return_once(|_| ());

	let next_ceremony_id = first_ceremony_id + 1;
	// Set up the mock api to expect the keygen and sign calls for the ceremonies we are
	// participating in. It doesn't matter what failure reasons they return.
	multisig_client
		.expect_initiate_keygen()
		.with(
			predicate::eq(next_ceremony_id),
			predicate::eq(GENESIS_EPOCH),
			predicate::eq(BTreeSet::from_iter([our_account_id.clone()])),
		)
		.once()
		.return_once(|_, _, _| {
			futures::future::ready(Err((BTreeSet::new(), KeygenFailureReason::InvalidParticipants)))
				.boxed()
		});

	task_scope(|scope| {
		async {
			// Handle a keygen request that we are not participating in
			sc_observer::handle_keygen_request::<_, _, _, I>(
				scope,
				&multisig_client,
				state_chain_client.clone(),
				first_ceremony_id,
				GENESIS_EPOCH,
				BTreeSet::from_iter([not_our_account_id.clone()]),
			)
			.await;

			// Handle a keygen request that we are participating in
			sc_observer::handle_keygen_request::<_, _, _, I>(
				scope,
				&multisig_client,
				state_chain_client.clone(),
				next_ceremony_id,
				GENESIS_EPOCH,
				BTreeSet::from_iter([our_account_id]),
			)
			.await;
			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap();
}

#[tokio::test]
async fn should_handle_keygen_request_eth() {
	should_handle_keygen_request::<EthSigning, EthereumInstance>().await;
}

mod dot_keygen {
	use crate::multisig::polkadot::PolkadotSigning;

	use super::*;
	use state_chain_runtime::PolkadotInstance;
	#[tokio::test]
	async fn should_handle_keygen_request_dot() {
		should_handle_keygen_request::<PolkadotSigning, PolkadotInstance>().await;
	}
}

#[tokio::test]
#[ignore = "runs forever, useful for testing without having to start the whole CFE"]
async fn run_the_sc_observer() {
	task_scope(|scope| {
		async {
			let settings = Settings::new_test().unwrap();

			let (initial_block_hash, sc_block_stream, state_chain_client) =
				crate::state_chain_observer::client::StateChainClient::new(
					scope,
					&settings.state_chain,
					AccountRole::None,
					false,
				)
				.await
				.unwrap();

			let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
				tokio::sync::mpsc::unbounded_channel();

			let (epoch_start_sender, _epoch_start_receiver) = async_broadcast::broadcast(10);

			let (cfe_settings_update_sender, _) =
				watch::channel::<CfeSettings>(CfeSettings::default());

			let (eth_monitor_ingress_sender, _eth_monitor_ingress_receiver) =
				tokio::sync::mpsc::unbounded_channel();

			let (eth_monitor_flip_ingress_sender, _eth_monitor_flip_ingress_receiver) =
				tokio::sync::mpsc::unbounded_channel();

			let (eth_monitor_usdc_ingress_sender, _eth_monitor_usdc_ingress_receiver) =
				tokio::sync::mpsc::unbounded_channel();

			let (dot_epoch_start_sender, _dot_epoch_start_receiver_1) =
				async_broadcast::broadcast(10);

			let (dot_monitor_ingress_sender, _dot_monitor_ingress_receiver) =
				tokio::sync::mpsc::unbounded_channel();

			let (dot_monitor_signature_sender, _dot_monitor_signature_receiver) =
				tokio::sync::mpsc::unbounded_channel();

			let (btc_epoch_start_sender, _btc_epoch_start_receiver_1) =
				async_broadcast::broadcast(10);

			let (btc_monitor_ingress_sender, _btc_monitor_ingress_receiver) =
				tokio::sync::mpsc::unbounded_channel();

			sc_observer::start(
				state_chain_client,
				sc_block_stream,
				EthBroadcaster::new(
					&settings.eth,
					EthWsRpcClient::new(&settings.eth).await.unwrap().clone(),
				)
				.unwrap(),
				DotBroadcaster::new(MockDotRpcApi::new()),
				BtcBroadcaster::new(MockBtcRpcApi::new()),
				MockMultisigClientApi::new(),
				MockMultisigClientApi::new(),
				MockMultisigClientApi::new(),
				account_peer_mapping_change_sender,
				epoch_start_sender,
				EthAddressToMonitorSender {
					eth: eth_monitor_ingress_sender,
					flip: eth_monitor_flip_ingress_sender,
					usdc: eth_monitor_usdc_ingress_sender,
				},
				dot_epoch_start_sender,
				dot_monitor_ingress_sender,
				dot_monitor_signature_sender,
				btc_epoch_start_sender,
				btc_monitor_ingress_sender,
				cfe_settings_update_sender,
				initial_block_hash,
			)
			.await
			.unwrap_err();

			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap();
}
