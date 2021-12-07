use cf_chains::eth::H256;
use chainflip_engine::{
    eth::{self, EthBroadcaster},
    state_chain::client::connect_to_state_chain,
};
use futures::StreamExt;
use settings::{CLICommandLineOptions, CLISettings};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_finality_grandpa::AuthorityId as GrandpaId;
use state_chain_node::chain_spec::get_from_seed;
use state_chain_runtime::opaque::SessionKeys;
use structopt::StructOpt;
use web3::types::H160;

use crate::settings::CFCommand::*;
use anyhow::Result;
use utilities::clean_eth_address;

mod settings;

#[tokio::main]
async fn main() {
    std::process::exit(match run_cli().await {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("Error: {:?}", err);
            1
        }
    })
}

async fn run_cli() -> Result<()> {
    let command_line_opts = CLICommandLineOptions::from_args();
    let cli_settings = CLISettings::new(command_line_opts.clone()).map_err(|_| anyhow::Error::msg("Please ensure your config file path is configured correctly. Or set all required command line arguments."))?;

    let logger = chainflip_engine::logging::utils::new_discard_logger();

    println!(
        "Connecting to state chain node at: `{}` and using private key located at: `{}`",
        cli_settings.state_chain.ws_endpoint,
        cli_settings.state_chain.signing_key_file.display()
    );

    match command_line_opts.cmd {
        Claim {
            amount,
            eth_address,
            should_register_claim,
        } => {
            request_claim(
                amount,
                clean_eth_address(&eth_address)
                    .map_err(|_| anyhow::Error::msg("You supplied an invalid ETH address"))?,
                &cli_settings,
                should_register_claim,
                &logger,
            )
            .await
        }
        Rotate {} => rotate_keys(&cli_settings, &logger).await,
    }
}

async fn request_claim(
    amount: f64,
    eth_address: [u8; 20],
    settings: &CLISettings,
    should_register_claim: bool,
    logger: &slog::Logger,
) -> Result<()> {
    let atomic_amount: u128 = (amount * 10_f64.powi(18)) as u128;

    println!(
        "Submitting claim with amount `{}` FLIP (`{}` Flipperinos) to ETH address `0x{}`",
        amount,
        atomic_amount,
        hex::encode(eth_address)
    );

    if !confirm_submit() {
        return Ok(());
    }

    let (_, block_stream, state_chain_client) = connect_to_state_chain(&settings.state_chain).await.map_err(|_| anyhow::Error::msg("Failed to connect to state chain node. Please ensure your state_chain_ws_endpoint is pointing to a working node."))?;

    // Currently you have to redeem rewards before you can claim them - this may eventually be
    // wrapped into the claim call: https://github.com/chainflip-io/chainflip-backend/issues/769
    let _tx_hash_redeem = state_chain_client
        .submit_signed_extrinsic(logger, pallet_cf_rewards::Call::redeem_rewards())
        .await
        .expect("Failed to submit redeem extrinsic");

    let tx_hash = state_chain_client
        .submit_signed_extrinsic(
            logger,
            pallet_cf_staking::Call::claim(atomic_amount, eth_address),
        )
        .await
        .expect("Failed to submit claim extrinsic");

    println!(
        "Your claim has transaction hash: `{:#x}`. Waiting for your request to be confirmed...",
        tx_hash
    );

    let mut block_stream = Box::new(block_stream);
    let block_stream = block_stream.as_mut();

    let events = state_chain_client
        .watch_submitted_extrinsic(tx_hash, block_stream)
        .await
        .expect("Failed to watch extrinsic");

    for event in events {
        if let state_chain_runtime::Event::EthereumThresholdSigner(
            pallet_cf_threshold_signature::Event::ThresholdSignatureRequest(..),
        ) = event
        {
            println!("Your claim request is on chain.\nWaiting for signed claim data...");
            'outer: while let Some(result_header) = block_stream.next().await {
                let header = result_header.expect("Failed to get a valid block header");
                let block_hash = header.hash();
                let events = state_chain_client
                    .get_events(block_hash)
                    .await
                    .unwrap_or_else(|e| {
                        panic!("Failed to fetch events for block: {}, {}", header.number, e)
                    });
                for (_phase, event, _) in events {
                    if let state_chain_runtime::Event::Staking(
                        pallet_cf_staking::Event::ClaimSignatureIssued(validator_id, claim_cert),
                    ) = event
                    {
                        if validator_id == state_chain_client.our_account_id {
                            if should_register_claim {
                                println!(
                                    "Your claim certificate is: {:?}",
                                    hex::encode(claim_cert.clone())
                                );
                                let chain_id = state_chain_client
                                    .get_environment_value::<u64>(block_hash, "EthereumChainId")
                                    .await
                                    .expect("Failed to fetch EthereumChainId from the State Chain");
                                let stake_manager_address = state_chain_client
                                    .get_environment_value(block_hash, "StakeManagerAddress")
                                    .await
                                    .expect("Failed to fetch StakeManagerAddress from State Chain");
                                let tx_hash = register_claim(
                                    settings,
                                    chain_id,
                                    stake_manager_address,
                                    logger,
                                    claim_cert,
                                )
                                .await
                                .expect("Failed to register claim on ETH");

                                println!(
                                    "Submitted claim to Ethereum successfully with tx_hash: {:#x}",
                                    tx_hash
                                );
                                break 'outer;
                            } else {
                                println!("Your claim request has been successfully registered. Please proceed to the Staking UI to complete your claim. <LINK>");
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Register the claim certificate on Ethereum
async fn register_claim(
    settings: &CLISettings,
    chain_id: u64,
    stake_manager_address: H160,
    logger: &slog::Logger,
    claim_cert: Vec<u8>,
) -> Result<H256> {
    println!(
        "Registering your claim on the Ethereum network, to StakeManager address: {:?}",
        stake_manager_address
    );

    let eth_broadcaster = EthBroadcaster::new(
        &settings.eth,
        eth::new_synced_web3_client(&settings.eth, &logger)
            .await
            .expect("Failed to create Web3 WebSocket"),
    )?;

    eth_broadcaster
        .send(
            eth_broadcaster
                .encode_and_sign_tx(cf_chains::eth::UnsignedTransaction {
                    chain_id,
                    contract: stake_manager_address,
                    data: claim_cert,
                    ..Default::default()
                })
                .await?
                .0,
        )
        .await
}

async fn rotate_keys(settings: &CLISettings, logger: &slog::Logger) -> Result<()> {
    let (_, _, state_chain_client) = connect_to_state_chain(&settings.state_chain).await.map_err(|e| anyhow::Error::msg(format!("{:?} Failed to connect to state chain node. Please ensure your state_chain_ws_endpoint is pointing to a working node.", e)))?;
    let seed = state_chain_client
        .rotate_session_keys()
        .await
        .expect("Could not rotate session keys.");

    println!("New session key {:?}.", seed);

    let new_session_key = SessionKeys {
        aura: get_from_seed::<AuraId>(&seed),
        grandpa: get_from_seed::<GrandpaId>(&seed),
    };

    let tx_hash = state_chain_client
        .submit_signed_extrinsic(
            logger,
            pallet_cf_validator::Call::set_keys(new_session_key, [0; 8].to_vec()),
        )
        .await
        .expect("Failed to submit set_keys extrinsic");

    println!("Session key rotated at tx {:#x}.", tx_hash);

    Ok(())
}

fn confirm_submit() -> bool {
    use std::io;
    use std::io::*;

    loop {
        print!("Do you wish to proceed? [y/n] > ");
        std::io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("Error: Failed to get user input");

        let input = input.trim();

        match input {
            "y" | "yes" | "1" | "true" | "ofc" => {
                println!("Submitting...");
                return true;
            }
            "n" | "no" | "0" | "false" | "nah" => {
                println!("Ok, exiting...");
                return false;
            }
            _ => {
                continue;
            }
        }
    }
}
