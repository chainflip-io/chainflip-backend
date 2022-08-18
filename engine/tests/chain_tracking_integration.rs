use anyhow::{Context, Result};
use async_trait::async_trait;
use chainflip_engine::{
    eth::{
        self, build_broadcast_channel,
        chain_data_witnesser::{TransactionParticipantProvider, TransactionParticipants},
        rpc::{EthDualRpcClient, EthHttpRpcClient, EthWsRpcClient},
    },
    logging,
    settings::{CommandLineOptions, Settings},
    state_chain_observer::client::SubmitSignedExtrinsic,
};
use sp_core::H256;
use state_chain_runtime::CfeSettings;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use web3::types::Address;
struct LogClient {}

#[async_trait]
impl SubmitSignedExtrinsic for LogClient {
    async fn submit_signed_extrinsic<Call>(&self, call: Call, logger: &slog::Logger) -> Result<H256>
    where
        Call: Into<state_chain_runtime::Call> + Clone + std::fmt::Debug + Send + Sync,
    {
        slog::info!(logger, "Observing {:?}", call);
        Ok(H256::default())
    }
}

struct AliceToBobTransactionProvider {}

impl TransactionParticipantProvider for AliceToBobTransactionProvider {
    fn get_transaction_participants(&self) -> Vec<TransactionParticipants> {
        return vec![TransactionParticipants {
            from: Address::from_str("0x70997970c51812dc3a010c7d01b50e0d17dc79c8").unwrap(), // Alice
            to: Address::from_str("0x3c44cdddb6a900fa2b585dd299e03d12fa4293bc").unwrap(),   // Bob
        }];
    }
}

#[tokio::test]
pub async fn test_chain_tracking() -> anyhow::Result<()> {
    let log_client = Arc::new(LogClient {});
    let settings =
        Settings::from_file_and_env("config/Testing.toml", CommandLineOptions::default()).unwrap();

    let root_logger = logging::utils::new_json_logger_with_tag_filter(
        settings.log.whitelist.clone(),
        settings.log.blacklist.clone(),
    );

    slog::info!(root_logger, "Start chain tracking integration test ");

    // Init web3 and eth broadcaster before connecting to SC, so we can diagnose these config errors, before
    // we connect to the SC (which requires the user to be staked)
    let eth_ws_rpc_client = EthWsRpcClient::new(&settings.eth, &root_logger)
        .await
        .context("Failed to create EthWsRpcClient")?;

    let eth_http_rpc_client = EthHttpRpcClient::new(&settings.eth, &root_logger)
        .context("Failed to create EthHttpRpcClient")?;

    let eth_dual_rpc = EthDualRpcClient::new(eth_ws_rpc_client, eth_http_rpc_client, &root_logger);

    let (witnessing_instruction_sender, [witnessing_instruction_receiver]) =
        build_broadcast_channel(10);

    let cfe_settings = CfeSettings::default();

    let (_cfe_settings_update_sender, cfe_settings_update_receiver) =
        tokio::sync::watch::channel(cfe_settings);

    witnessing_instruction_sender.send(eth::EpochStart {
        index: 0,
        eth_block: 0,
        current: true,
        participant: true,
    })?;

    let poll_interval = Duration::from_secs(1);
    eth::chain_data_witnesser::start(
        eth_dual_rpc,
        log_client,
        witnessing_instruction_receiver,
        cfe_settings_update_receiver,
        Arc::new(AliceToBobTransactionProvider {}),
        poll_interval,
        &root_logger,
    )
    .await
    .unwrap_err();

    Ok(())
}
