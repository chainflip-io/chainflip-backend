use std::sync::{Arc, Mutex};

use chainflip_engine::{
    eth::{self, eth_broadcaster, eth_tx_encoding, key_manager, stake_manager},
    health::HealthMonitor,
    heartbeat,
    mq::nats_client::NatsMQClient,
    p2p::{self, rpc as p2p_rpc, AccountId},
    settings::Settings,
    signing::{self, db::PersistentKeyDB},
    state_chain::{self, runtime::StateChainRuntime},
    temp_event_mapper,
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
        // which won't necessarily always be the case, i.e. if we no longer have PeerId == AccountId
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

    let web3 = eth::new_synced_web3_client(&settings, &root_logger)
        .await
        .unwrap();

    futures::join!(
        // Start signing components
        signing::start(
            AccountId(key_pair.public().0),
            db,
            mq_client.clone(),
            shutdown_client_rx,
            &root_logger,
        ),
        p2p::conductor::start(
            p2p_rpc::connect(
                &url::Url::parse(settings.state_chain.ws_endpoint.as_str()).expect(&format!(
                    "Should be valid ws endpoint: {}",
                    settings.state_chain.ws_endpoint
                )),
                AccountId(pair_signer.lock().unwrap().signer().public().0)
            )
            .await
            .expect("unable to connect p2p rpc client"),
            mq_client.clone(),
            p2p_shutdown_rx,
            &root_logger.clone()
        ),
        heartbeat::start(subxt_client.clone(), pair_signer.clone(), &root_logger),
        // Start state chain components
        state_chain::sc_observer::start(mq_client.clone(), subxt_client.clone(), &root_logger),
        temp_event_mapper::start(mq_client.clone(), &root_logger),
        // Start eth components
        eth_broadcaster::start_eth_broadcaster(&web3, &settings, mq_client.clone(), &root_logger),
        eth_tx_encoding::set_agg_key_with_agg_key::start(
            &settings,
            mq_client.clone(),
            &root_logger
        ),
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
