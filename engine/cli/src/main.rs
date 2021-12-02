use cf_chains::eth::H256;
use chainflip_engine::{
    eth::{self, EthBroadcaster},
    state_chain::client::connect_to_state_chain,
};
use futures::StreamExt;
use settings::{CLICommandLineOptions, CLISettings};
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

    println!(
        "Connecting to state chain node at: `{}` and using private key located at: `{}`",
        cli_settings.state_chain.ws_endpoint,
        cli_settings.state_chain.signing_key_file.display()
    );

    let logger = chainflip_engine::logging::utils::new_discard_logger();

    match command_line_opts.cmd {
        Claim {
            amount,
            eth_address,
            should_register_claim,
        } => {
            println!("Should register claim: {:?}", should_register_claim);
            Ok(request_claim(
                amount,
                clean_eth_address(&eth_address)
                    .map_err(|_| anyhow::Error::msg("You supplied an invalid ETH address"))?,
                &cli_settings,
                should_register_claim,
                &logger,
            )
            .await?)
        }
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
        "Your claim has transaction hash: `{:?}`. Waiting for your request to be confirmed...",
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
                                let stake_manager_address = state_chain_client
                                    .get_environment_value(block_hash, "StakeManagerAddress")
                                    .await
                                    .expect("Failed to fetch StakeManagerAddress from State Chain");
                                let tx_hash = register_claim(
                                    settings,
                                    stake_manager_address,
                                    logger,
                                    claim_cert,
                                )
                                .await
                                .expect("Failed to register claim on ETH");

                                println!(
                                    "Submitted claim to Ethereum successfully with tx_hash: {:?}",
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
    stake_manager_address: H160,
    logger: &slog::Logger,
    claim_cert: Vec<u8>,
) -> Result<H256> {
    println!("Registering your claim on the Ethereum network.");

    let web3_client = eth::new_synced_web3_client(&settings.eth, &logger)
        .await
        .expect("Failed to create Web3 WebSocket");

    let eth_broadcaster = EthBroadcaster::new(&settings.eth, web3_client.clone())?;

    let unsigned_tx = cf_chains::eth::UnsignedTransaction {
        chain_id: 4,
        contract: stake_manager_address,
        data: claim_cert,
        ..Default::default()
    };

    let claim_signed = eth_broadcaster.encode_and_sign_tx(unsigned_tx).await?;

    eth_broadcaster.send(claim_signed.0).await
}

#[tokio::test]
async fn test_reg_claim() {
    let claim_cert = "555530527c2dc37729cd775ca818ac62ae575bc2955b7edbeaa3e8be21c182b6b702cca250a578eac2c4be59a2c6079c3890fdad3ede705eddfb3a3200e1c563258f6421000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000b181919af901b0ab6d0e433536e72bf3a706dc7f8898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f0400000000000000000000000000000000000000000000000000000002540be400000000000000000000000000f29ab9ebdb481be48b80699758e6e9a3dbd609c60000000000000000000000000000000000000000000000000000000061afbe60";
    let claim_cert = hex::decode(claim_cert).unwrap();

    let cli_settings = CLISettings::from_file(
        "/Users/kylezs/Documents/cf-repos/chainflip-backend/engine/config/Local.toml",
    )
    .unwrap();

    let logger = new_discard_logger();

    register_claim(&cli_settings, &logger, claim_cert)
        .await
        .expect("Register claim failed");
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
