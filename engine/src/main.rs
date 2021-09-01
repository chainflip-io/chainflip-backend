use std::sync::{Arc, Mutex};

use chainflip_engine::{
    eth::{self, key_manager, stake_manager, EthBroadcaster},
    health::HealthMonitor,
    heartbeat,
    mq::nats_client::NatsMQClient,
    p2p::{self, RpcP2PClient, ValidatorId},
    settings::Settings,
    signing,
    signing::{db::PersistentKeyDB, MultisigEvent, MultisigInstruction},
    state_chain::{self, runtime::StateChainRuntime},
};
use slog::{o, Drain};
use sp_core::Pair;
use substrate_subxt::ClientBuilder;

#[tokio::main]
async fn main() {
    let drain = slog_json::Json::new(std::io::stdout())
        .add_default_keys()
        .build()
        .fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    let root_logger = slog::Logger::root(drain, o!());
    slog::info!(root_logger, "Start the engines! :broom: :broom: "; o!());

    let settings = Settings::new().expect("Failed to initialise settings");

    HealthMonitor::new(&settings.health_check, &root_logger)
        .run()
        .await;

    slog::info!(
        &root_logger,
        "Connecting to NatsMQ at: {}",
        &settings.message_queue.endpoint
    );
    let mq_client = NatsMQClient::new(&settings.message_queue)
        .await
        .expect("Should connect to message queue");

    let subxt_client = ClientBuilder::<StateChainRuntime>::new()
        .set_url(&settings.state_chain.ws_endpoint)
        .build()
        .await
        .expect("Should create subxt client");

    let key_pair = sp_core::sr25519::Pair::from_seed(&{
        // This can be the same filepath as the p2p key --node-key-file <file> on the state chain
        // which won't necessarily always be the case, i.e. if we no longer have PeerId == ValidatorId
        use std::{convert::TryInto, fs};
        let seed: [u8; 32] = hex::decode(
            &fs::read_to_string(&settings.state_chain.p2p_private_key_file)
                .expect("Cannot read private key file")
                .replace("\"", ""),
        )
        .expect("Failed to decode seed")
        .try_into()
        .expect("Seed has wrong length");
        seed
    });

    let pair_signer = {
        use substrate_subxt::{system::AccountStoreExt, Signer};
        let mut pair_signer = substrate_subxt::PairSigner::new(key_pair.clone());
        let account_id = pair_signer.account_id();
        let nonce = subxt_client
            .account(&account_id, None)
            .await
            .expect("Should be able to fetch account info")
            .nonce;
        slog::info!(root_logger, "Initial state chain nonce is: {}", nonce);
        pair_signer.set_nonce(nonce);
        // Allow in the future many witnessers to increment the nonce of this signer
        Arc::new(Mutex::new(pair_signer))
    };

    // TODO: Investigate whether we want to encrypt it on disk
    let db = PersistentKeyDB::new(&settings.signing.db_file.as_path(), &root_logger);

    let (_, p2p_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (_, shutdown_client_rx) = tokio::sync::oneshot::channel::<()>();
    let (multisig_instruction_sender, multisig_instruction_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigInstruction>();
    let (multisig_event_sender, multisig_event_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MultisigEvent>();

    let web3 = eth::new_synced_web3_client(&settings, &root_logger)
        .await
        .unwrap();

    let eth_broadcaster = EthBroadcaster::new(&settings, web3.clone()).unwrap();

    futures::join!(
        // Start signing components
        signing::start(
            ValidatorId(key_pair.public().0),
            db,
            mq_client.clone(),
            multisig_instruction_receiver,
            multisig_event_sender,
            shutdown_client_rx,
            &root_logger,
        ),
        p2p::conductor::start(
            RpcP2PClient::new(
                url::Url::parse(settings.state_chain.ws_endpoint.as_str()).expect(&format!(
                    "Should be valid ws endpoint: {}",
                    settings.state_chain.ws_endpoint
                )),
                &root_logger
            ),
            mq_client.clone(),
            p2p_shutdown_rx,
            &root_logger
        ),
        heartbeat::start(subxt_client.clone(), pair_signer.clone(), &root_logger),
        // Start state chain components
        state_chain::sc_observer::start(
            &settings,
            subxt_client.clone(),
            pair_signer.clone(),
            eth_broadcaster,
            multisig_instruction_sender,
            multisig_event_receiver,
            &root_logger
        ),
        // Start eth components
        stake_manager::start_stake_manager_witness(
            &web3,
            &settings,
            pair_signer.clone(),
            subxt_client.clone(),
            &root_logger
        )
        .await
        .unwrap(),
        key_manager::start_key_manager_witness(
            &web3,
            &settings,
            pair_signer,
            subxt_client,
            &root_logger
        )
        .await
        .unwrap(),
    );
}
